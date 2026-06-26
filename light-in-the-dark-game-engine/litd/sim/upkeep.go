package sim

// Upkeep economy + inter-player transfer tax (#375, split from #373).
//
// Two distinct WC3 mechanics share this file because both are income
// taxes on the per-player resource ledger (harvest.go is the SoT):
//
//   1. UPKEEP — a food-tier income tax. As a player's food usage climbs
//      past configured brackets, a fraction of every harvested deposit is
//      withheld (WC3's no/low/high upkeep). The current fraction is read
//      back as PLAYER_STATE_GOLD_UPKEEP_RATE / PLAYER_STATE_LUMBER_UPKEEP_RATE
//      (derived live from food usage), and the cumulative amount withheld is
//      PLAYER_SCORE_GOLD_LOST_UPKEEP / PLAYER_SCORE_LUMBER_LOST_UPKEEP.
//
//   2. TRANSFER TAX — when one player hands resources to another
//      (TransferResource), a per-(source,other,resource) fraction is
//      withheld (GetPlayerTaxRate / SetPlayerTaxRate).
//
// Both default to zero everywhere: an unconfigured map plays untaxed and
// the harvest-deposit byte path is identical to pre-#375, so the golden
// determinism trace is undisturbed. Only an explicit BindUpkeep /
// SetTaxRate changes outcomes.

import (
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
)

// maxUpkeepTiers bounds the bracket table (WC3 uses 3: no/low/high).
const maxUpkeepTiers = 8

// UpkeepTier is one income-tax bracket. When a player's live food usage is
// >= Food, harvested income of resource r is taxed by Rate[r] (a fixed-point
// fraction in [0,1]) at deposit. The matching tier is the one with the
// greatest Food <= the player's food usage.
type UpkeepTier struct {
	Food int32
	Rate [data.MaxResourceTypes]fixed.F64
}

// clampRate pins a tax fraction to [0,1] — a negative tax would pay the
// player to mine and a >1 tax would mint negative resources.
func clampRate(v fixed.F64) fixed.F64 {
	if v < 0 {
		return 0
	}
	if v > fixed.One {
		return fixed.One
	}
	return v
}

// BindUpkeep installs the food-tier tax brackets. Tiers must be in strictly
// ascending Food order (each bracket starts where the previous ends); a
// non-ascending list or one exceeding maxUpkeepTiers is refused and leaves
// the table unchanged. Per-resource rates are clamped to [0,1]. An empty
// slice clears all upkeep (returns to the no-tax default).
func (w *World) BindUpkeep(tiers []UpkeepTier) bool {
	if len(tiers) > maxUpkeepTiers {
		return false
	}
	for i := 1; i < len(tiers); i++ {
		if tiers[i].Food <= tiers[i-1].Food {
			return false
		}
	}
	// validated — commit (clear first so a shrink drops stale rows).
	w.upkeepCount = len(tiers)
	for i := 0; i < maxUpkeepTiers; i++ {
		if i < len(tiers) {
			w.upkeepFood[i] = tiers[i].Food
			for r := 0; r < data.MaxResourceTypes; r++ {
				w.upkeepRate[i][r] = clampRate(tiers[i].Rate[r])
			}
		} else {
			w.upkeepFood[i] = 0
			w.upkeepRate[i] = [data.MaxResourceTypes]fixed.F64{}
		}
	}
	return true
}

// upkeepRateFor returns the tax fraction applied to player p's deposits of
// resource res given p's current food usage: the rate of the highest bracket
// whose Food threshold p has reached, or 0 if none (or no brackets bound).
func (w *World) upkeepRateFor(p, res uint8) fixed.F64 {
	if w.upkeepCount == 0 || p >= MaxPlayers || int(res) >= data.MaxResourceTypes {
		return 0
	}
	food := w.foodUsed[p]
	rate := fixed.F64(0)
	// brackets are ascending; the last one with Food <= food wins.
	for i := 0; i < w.upkeepCount; i++ {
		if food >= w.upkeepFood[i] {
			rate = w.upkeepRate[i][res]
		} else {
			break
		}
	}
	return rate
}

// applyUpkeep splits a gross deposit of resource res for player p into the
// kept amount (returned) and the taxed amount (accumulated into the
// upkeep-lost counter). Called from the deposit chokepoint only.
func (w *World) applyUpkeep(p, res uint8, gross int64) int64 {
	if gross <= 0 {
		return gross
	}
	rate := w.upkeepRateFor(p, res)
	if rate <= 0 {
		return gross
	}
	tax := scaleI64(gross, rate)
	if tax > gross {
		tax = gross
	}
	w.upkeepLost[p][res] += tax
	return gross - tax
}

// UpkeepRate reads the tax fraction currently applied to player p's deposits
// of resource res, derived live from p's food usage (PLAYER_STATE_*_UPKEEP_RATE).
func (w *World) UpkeepRate(p uint8, res int) fixed.F64 {
	if res < 0 || res >= data.MaxResourceTypes {
		return 0
	}
	return w.upkeepRateFor(p, uint8(res))
}

// UpkeepLost reads the cumulative resource withheld from player p's deposits
// by the upkeep tax (PLAYER_SCORE_*_LOST_UPKEEP).
func (w *World) UpkeepLost(p uint8, res int) int64 {
	if p >= MaxPlayers || res < 0 || res >= data.MaxResourceTypes {
		return 0
	}
	return w.upkeepLost[p][res]
}

// ---- inter-player transfer tax (GetPlayerTaxRate / SetPlayerTaxRate) ----

// TaxRate reads source's transfer-tax fraction toward other for one resource.
// A player has no tax toward itself.
func (w *World) TaxRate(source, other uint8, res int) fixed.F64 {
	if source >= MaxPlayers || other >= MaxPlayers || source == other ||
		res < 0 || res >= data.MaxResourceTypes {
		return 0
	}
	return w.taxRate[source][other][res]
}

// SetTaxRate sets source's transfer-tax fraction toward other for one
// resource (clamped to [0,1]). No-op for out-of-range or self.
func (w *World) SetTaxRate(source, other uint8, res int, rate fixed.F64) {
	if source >= MaxPlayers || other >= MaxPlayers || source == other ||
		res < 0 || res >= data.MaxResourceTypes {
		return
	}
	w.taxRate[source][other][res] = clampRate(rate)
}

// TransferResource moves up to amount of resource res from player `from` to
// player `to`, applying from's transfer tax toward `to`. The full debited
// amount leaves `from`; the taxed fraction is destroyed; the remainder is
// credited to `to`. Returns the net amount delivered. No-op (returns 0) for
// out-of-range players, self-transfer, non-positive amount, an unbound
// economy, or insufficient balance reduces the move to the available amount.
func (w *World) TransferResource(from, to uint8, res int, amount int64) int64 {
	if from >= MaxPlayers || to >= MaxPlayers || from == to ||
		res < 0 || res >= w.resourceCount || amount <= 0 {
		return 0
	}
	avail := w.resources[from][res]
	if amount > avail {
		amount = avail
	}
	if amount <= 0 {
		return 0
	}
	tax := scaleI64(amount, w.taxRate[from][to][res])
	if tax > amount {
		tax = amount
	}
	net := amount - tax
	w.resources[from][res] -= amount
	w.resources[to][res] += net
	return net
}

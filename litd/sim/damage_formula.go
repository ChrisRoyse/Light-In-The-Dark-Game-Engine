package sim

// Staged, overridable damage-formula pipeline (#473, ADR #453). The single
// post-mitigation amount a packet applies is computed by an ORDERED list of
// named stages over a mutable DamageCtx. The engine ships a base formula —
// coeff-lookup → armor-reduction → handicap → script-modifier → clamp — and a
// world may replace the whole formula (SetDamageFormula) or any single stage
// (ReplaceStage), deterministically. The base algorithm is the default;
// overriding is always explicit, never silent.
//
// Determinism: stages are fixed-point only (fixed.F64 — no float, no NaN, so
// bit-identical cross-platform) and must be pure (no PRNG, no waits). The
// pipeline runs inside the combat phase against a reused DamageCtx, so steady
// state is zero-alloc (R-GC-1). Override IDENTITY (which stages were replaced)
// folds into the ECA setup-identity sub-hash and serializes with the save, so
// an overriding world hashes differently and round-trips its override on
// save/load — while a base (unmodified) world contributes nothing and keeps a
// byte-identical hash.

import (
	"errors"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/statehash"
)

var (
	errEmptyFormula = errors.New("sim: SetDamageFormula: empty stage list")
	errBadStage     = errors.New("sim: damage stage: empty name or nil Fn")
	errUnknownStage = errors.New("sim: ReplaceStage: unknown stage name")
	errBadArmorK    = errors.New("sim: SetArmorCoefficient: coefficient must be positive")
)

// DamageCtx is the mutable per-packet context a damage stage reads and writes.
// damageApplySystem reuses one instance across packets (zero-alloc); it resets
// the fields each packet before running the pipeline. Stages mutate Amount;
// the value left in Amount after the last stage is the damage applied.
type DamageCtx struct {
	Source, Target EntityID  // packet endpoints (live target guaranteed)
	AttackType     uint8      // matrix row (validated in range before the pipeline)
	ArmorType      uint8      // matrix column (validated in range)
	ArmorValue     int        // the target's base armor value (pre-buff)
	Raw            fixed.F64  // the original queued amount (stages should treat as read-only)
	Amount         fixed.F64  // running value; final value = applied damage
	w              *World     // world access for the base stages
}

// DamageStage is one named step in the damage-formula pipeline. Name is the
// stable identity used by ReplaceStage and folded into the hash; Fn is the
// deterministic transform over the running context.
type DamageStage struct {
	Name string
	Fn   func(c *DamageCtx)
}

// maxDamageStages bounds the stage count a save may declare (corrupt-save
// guard). A real formula is a handful of stages; this is generous headroom.
const maxDamageStages = 4096

// Base stage names — the stable identity of the shipped formula.
const (
	StageCoeff    = "coeff-lookup"
	StageArmor    = "armor-reduction"
	StageHandicap = "handicap"
	StageScript   = "script-modifier"
	StageClamp    = "clamp"
)

// installBaseDamageFormula registers the shipped formula in order. Called once
// at NewWorld. The stages read world state through c.w.
func (w *World) installBaseDamageFormula() {
	w.formula = []DamageStage{
		{StageCoeff, stageCoeff},
		{StageArmor, stageArmor},
		{StageHandicap, stageHandicap},
		{StageScript, stageScript},
		{StageClamp, stageClamp},
	}
	w.fOverride = make([]bool, len(w.formula))
	w.fReplaced = false
}

// stageCoeff multiplies the raw amount by the per-mille attack×armor
// coefficient. The matrix and the type indices are validated before the
// pipeline runs (out-of-range packets are dropped+counted upstream).
func stageCoeff(c *DamageCtx) {
	coeff := c.w.coeff[c.AttackType][c.ArmorType]
	c.Amount = c.Amount.Mul(fixed.FromInt(coeff)).Div(fixed.FromInt(1000))
}

// stageArmor applies the buffed-armor damage-reduction LUT (#162). The LUT is
// per-world and rebuilt from a configurable coefficient (#474); a default world
// uses the shipped 0.06 curve.
func stageArmor(c *DamageCtx) {
	armor := c.w.BuffedArmor(c.Target, c.ArmorValue)
	if armor < ArmorLUTMin {
		armor = ArmorLUTMin
	} else if armor > ArmorLUTMax {
		armor = ArmorLUTMax
	}
	c.Amount = c.Amount.Mul(c.w.armorLUT[armor-ArmorLUTMin])
	if c.Amount < 0 {
		c.Amount = 0
	}
}

// SetArmorCoefficient rebuilds the armor-reduction LUT from a new positive-
// branch coefficient k (#474), keeping the #330 negative-armor piecewise branch.
// Default is 0.06; passing it (or an equal value) restores the shipped curve.
// Fail-closed: k must be positive. The coefficient is config that changes damage
// outcomes, so a non-default value folds into the override-identity hash and the
// save (a default world contributes nothing — byte-identical hash). Setup-time.
func (w *World) SetArmorCoefficient(k fixed.F64) error {
	if k <= 0 {
		return errBadArmorK
	}
	w.armorK = k
	w.armorLUT = buildArmorLUTk(k)
	w.armorKOver = k != defaultArmorK
	return nil
}

// ArmorMultiplier returns the damage multiplier this world applies at armor
// value a (clamped to the LUT bounds) — for dumps, tests, and observability.
func (w *World) ArmorMultiplier(a int) fixed.F64 {
	if a < ArmorLUTMin {
		a = ArmorLUTMin
	} else if a > ArmorLUTMax {
		a = ArmorLUTMax
	}
	return w.armorLUT[a-ArmorLUTMin]
}

// ArmorCoefficient returns the current positive-branch reduction coefficient.
func (w *World) ArmorCoefficient() fixed.F64 { return w.armorK }

// stageHandicap scales by the source's damage handicap and the target's
// incoming handicap (#373). Both default to 1.0 → no-op for an unconfigured
// match (golden trace stable).
func stageHandicap(c *DamageCtx) {
	w := c.w
	if sor := w.Owners.Row(c.Source); sor != -1 {
		c.Amount = c.Amount.Mul(w.players.handicapDamage[w.Owners.Player[sor]])
	}
	if tor := w.Owners.Row(c.Target); tor != -1 {
		c.Amount = c.Amount.Mul(w.players.handicap[w.Owners.Player[tor]])
	}
	if c.Amount < 0 {
		c.Amount = 0
	}
}

// stageScript applies the legacy writable-damage hook (#219) if installed. The
// hook stays non-hashed script wiring (SetDamageModifier); a world that wants a
// hashed, save-surviving change replaces a stage via ReplaceStage instead.
func stageScript(c *DamageCtx) {
	if c.w.damageMod != nil {
		c.Amount = c.w.damageMod(c.Source, c.Target, c.Amount)
		if c.Amount < 0 {
			c.Amount = 0
		}
	}
}

// stageClamp is the fail-closed floor: damage never heals.
func stageClamp(c *DamageCtx) {
	if c.Amount < 0 {
		c.Amount = 0
	}
}

// runDamageFormula executes the pipeline over the reused context and returns
// the applied amount, floored at 0 (a custom formula that omits clamp or goes
// negative is still fail-closed — never a heal).
func (w *World) runDamageFormula(src, dst EntityID, attackType, armorType uint8, armorValue int, raw fixed.F64) fixed.F64 {
	c := &w.dmgCtx
	c.Source = src
	c.Target = dst
	c.AttackType = attackType
	c.ArmorType = armorType
	c.ArmorValue = armorValue
	c.Raw = raw
	c.Amount = raw
	c.w = w
	for i := range w.formula {
		w.formula[i].Fn(c)
	}
	if c.Amount < 0 {
		return 0
	}
	return c.Amount
}

// SetDamageFormula replaces the whole pipeline with a custom ordered stage
// list, deterministically (ADR #453). Fail-closed: an empty list or a stage
// with an empty name or nil Fn is refused and the existing formula is kept.
// Marks the formula as wholesale-overridden so its identity folds into the hash
// and the save. Setup-time (not the hot path) — allocation here is fine.
func (w *World) SetDamageFormula(stages []DamageStage) error {
	if len(stages) == 0 {
		return errEmptyFormula
	}
	for i := range stages {
		if stages[i].Name == "" || stages[i].Fn == nil {
			return errBadStage
		}
	}
	w.formula = append(w.formula[:0:0], stages...)
	w.fOverride = make([]bool, len(w.formula))
	for i := range w.fOverride {
		w.fOverride[i] = true
	}
	w.fReplaced = true
	return nil
}

// ReplaceStage swaps the Fn of the single named stage in place, keeping order.
// Fail-closed: an unknown name or a nil Fn is refused. Marks that stage
// overridden (identity folds into hash + save).
func (w *World) ReplaceStage(name string, fn func(c *DamageCtx)) error {
	if fn == nil {
		return errBadStage
	}
	for i := range w.formula {
		if w.formula[i].Name == name {
			w.formula[i].Fn = fn
			w.fOverride[i] = true
			return nil
		}
	}
	return errUnknownStage
}

// FormulaStageNames returns the ordered stage names (for dumps/observability).
func (w *World) FormulaStageNames() []string {
	out := make([]string, len(w.formula))
	for i := range w.formula {
		out[i] = w.formula[i].Name
	}
	return out
}

// formulaOverridden reports whether the formula differs from the shipped base
// (any stage replaced, or a wholesale custom formula). A base world reports
// false and contributes nothing to the override-identity hash/save.
func (w *World) formulaOverridden() bool {
	if w.fReplaced {
		return true
	}
	for _, o := range w.fOverride {
		if o {
			return true
		}
	}
	return false
}

// hashDamageFormula folds the override identity into a hasher. Contributes
// NOTHING when the formula is the unmodified base — so a non-overriding world's
// state hash is byte-identical to before #473. When overridden, it writes the
// ordered (name, overridden-bit) pairs so a divergent override is caught in the
// hash, not only at load (mirrors the #455 handler-identity discipline).
func (w *World) hashDamageFormula(h *statehash.Hasher) {
	if !w.formulaOverridden() && !w.armorKOver {
		return // base formula + default armor: contribute nothing (golden-stable)
	}
	// formula stage identity (only when stages were overridden — keeps a
	// pure-armor-coefficient override byte-identical here to a #473-era world
	// with no formula override).
	if w.formulaOverridden() {
		h.WriteU8(1)
		h.WriteU32(uint32(len(w.formula)))
		for i := range w.formula {
			hashString(h, w.formula[i].Name)
			if w.fOverride[i] {
				h.WriteU8(1)
			} else {
				h.WriteU8(0)
			}
		}
	} else {
		h.WriteU8(0)
	}
	// armor coefficient identity (#474), only when non-default.
	if w.armorKOver {
		h.WriteU8(1)
		h.WriteU64(uint64(w.armorK))
	} else {
		h.WriteU8(0)
	}
}

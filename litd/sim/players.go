package sim

// Player roster (#218). Per-player metadata (controller, race, color,
// team, start location, name, allied-victory) plus the asymmetric
// alliance relation. Resources and the food ledger live in the economy
// section (harvest.go); this file completes the player-state matrix and
// adds the relation that IsAlly/IsEnemy read.
//
// Alliance is one-directional, mirroring WC3: A's stance toward B is
// independent of B's stance toward A (porting hazard 2). Each ordered
// pair holds a flags bitset; the canonical "ally" notion is the passive
// (not-at-war) bit. Combat targeting still runs on the per-unit team
// model this milestone — the alliance table is parallel relation state
// (decision recorded on #218) so the golden determinism trace is not
// perturbed.

import "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"

// Alliance flag bits — one per JASS alliancetype. Stored per ordered
// (source, other) pair so the relation stays asymmetric.
const (
	AlliancePassive          uint16 = 1 << 0 // not at war (the "ally" bit)
	AllianceHelpRequest      uint16 = 1 << 1
	AllianceHelpResponse     uint16 = 1 << 2
	AllianceSharedXP         uint16 = 1 << 3
	AllianceSharedSpells     uint16 = 1 << 4
	AllianceSharedVision     uint16 = 1 << 5
	AllianceSharedControl    uint16 = 1 << 6
	AllianceSharedAdvControl uint16 = 1 << 7
	AllianceRescuable        uint16 = 1 << 8
	AllianceSharedVisionForce uint16 = 1 << 9

	allianceFlagMask uint16 = 1<<10 - 1
)

// Controller kinds. Ordered so the zero value is an empty/unconfigured
// slot — a fresh World has every player "none" until a map populates it.
// The public mapcontrol numbering (USER=0…) is mapped at the API layer.
const (
	ControllerNone uint8 = iota
	ControllerUser
	ControllerComputer
	ControllerRescuable
	ControllerNeutral
	ControllerCreep
)

// playerRoster is the per-player metadata table. Fixed [MaxPlayers]
// arrays (no allocation, no map iteration) keep it deterministic and
// trivially hashable in slot order.
type playerRoster struct {
	name          [MaxPlayers]string
	controller    [MaxPlayers]uint8
	race          [MaxPlayers]uint8
	color         [MaxPlayers]uint8
	team          [MaxPlayers]uint8
	startX        [MaxPlayers]fixed.F64
	startY        [MaxPlayers]fixed.F64
	alliedVictory [MaxPlayers]bool
	// alliance[a][b]: a's flags toward b. Asymmetric by construction.
	alliance [MaxPlayers][MaxPlayers]uint16
}

// initPlayers seeds the FFA default: each player on its own team, default
// player color = slot, no alliances set (everyone an enemy of everyone
// else, nobody their own ally/enemy). Called once from NewWorld.
func (w *World) initPlayers() {
	for p := 0; p < MaxPlayers; p++ {
		w.players.team[p] = uint8(p)
		w.players.color[p] = uint8(p)
		w.players.controller[p] = ControllerNone
	}
	// alliance defaults to all-zero (enemy); IsAlly/IsEnemy guard self.
}

// ---- player-state accessors (D5 over the roster table) ----

// PlayerName / SetPlayerName read and write a player's display name.
func (w *World) PlayerName(p uint8) string {
	if p >= MaxPlayers {
		return ""
	}
	return w.players.name[p]
}
func (w *World) SetPlayerName(p uint8, name string) {
	if p < MaxPlayers {
		w.players.name[p] = name
	}
}

// Controller / SetController read and write a player's controller kind.
func (w *World) Controller(p uint8) uint8 {
	if p >= MaxPlayers {
		return ControllerNone
	}
	return w.players.controller[p]
}
func (w *World) SetController(p uint8, c uint8) {
	if p < MaxPlayers && c <= ControllerCreep {
		w.players.controller[p] = c
	}
}

// PlayerRace / SetPlayerRace read and write a player's race id (0 = none).
func (w *World) PlayerRace(p uint8) uint8 {
	if p >= MaxPlayers {
		return 0
	}
	return w.players.race[p]
}
func (w *World) SetPlayerRace(p uint8, r uint8) {
	if p < MaxPlayers {
		w.players.race[p] = r
	}
}

// PlayerColor / SetPlayerColor read and write a player's color slot.
func (w *World) PlayerColor(p uint8) uint8 {
	if p >= MaxPlayers {
		return 0
	}
	return w.players.color[p]
}
func (w *World) SetPlayerColor(p uint8, c uint8) {
	if p < MaxPlayers {
		w.players.color[p] = c
	}
}

// PlayerTeam / SetPlayerTeam read and write a player's roster team. This
// is FFA/scoring metadata; it does not retroactively re-team a player's
// already-spawned units (per-unit team is set at spawn / ChangeOwner).
func (w *World) PlayerTeam(p uint8) uint8 {
	if p >= MaxPlayers {
		return 0
	}
	return w.players.team[p]
}
func (w *World) SetPlayerTeam(p uint8, t uint8) {
	if p < MaxPlayers {
		w.players.team[p] = t
	}
}

// PlayerStart / SetPlayerStart read and write a player's start location.
func (w *World) PlayerStart(p uint8) (x, y fixed.F64) {
	if p >= MaxPlayers {
		return 0, 0
	}
	return w.players.startX[p], w.players.startY[p]
}
func (w *World) SetPlayerStart(p uint8, x, y fixed.F64) {
	if p < MaxPlayers {
		w.players.startX[p] = x
		w.players.startY[p] = y
	}
}

// AlliedVictory / SetAlliedVictory read and write the allied-victory flag.
func (w *World) AlliedVictory(p uint8) bool {
	if p >= MaxPlayers {
		return false
	}
	return w.players.alliedVictory[p]
}
func (w *World) SetAlliedVictory(p uint8, on bool) {
	if p < MaxPlayers {
		w.players.alliedVictory[p] = on
	}
}

// ---- resource setters (the economy ledger is the SoT, harvest.go) ----

// SetResource sets a player's counter for one resource index, clamped to
// non-negative. No-op if the index is out of range or the economy is
// unbound (resourceCount == 0).
func (w *World) SetResource(player uint8, resource int, value int64) {
	if player >= MaxPlayers || resource < 0 || resource >= w.resourceCount {
		return
	}
	if value < 0 {
		value = 0
	}
	w.resources[player][resource] = value
}

// AddResource adds delta (may be negative) to a player's counter, clamped
// to non-negative. No-op on a bad index / unbound economy.
func (w *World) AddResource(player uint8, resource int, delta int64) {
	if player >= MaxPlayers || resource < 0 || resource >= w.resourceCount {
		return
	}
	v := w.resources[player][resource] + delta
	if v < 0 {
		v = 0
	}
	w.resources[player][resource] = v
}

// SetFoodCap overrides a player's supply cap directly (clamped to >= 0).
// SetFoodUsed is intentionally absent — used is derived from live units.
func (w *World) SetFoodCap(player uint8, cap int32) {
	if player >= MaxPlayers {
		return
	}
	if cap < 0 {
		cap = 0
	}
	w.foodCap[player] = cap
}

// ---- alliance relation ----

// SetAlliance replaces source's entire flag bitset toward other. One
// directional: it does not touch other→source. No-op for out-of-range or
// self (a player has no alliance stance toward itself).
func (w *World) SetAlliance(source, other uint8, flags uint16) {
	if source >= MaxPlayers || other >= MaxPlayers || source == other {
		return
	}
	w.players.alliance[source][other] = flags & allianceFlagMask
}

// SetAllianceFlag sets or clears a single alliance bit of source toward
// other, leaving the other bits intact.
func (w *World) SetAllianceFlag(source, other uint8, flag uint16, on bool) {
	if source >= MaxPlayers || other >= MaxPlayers || source == other {
		return
	}
	flag &= allianceFlagMask
	if on {
		w.players.alliance[source][other] |= flag
	} else {
		w.players.alliance[source][other] &^= flag
	}
}

// Alliance returns source's raw flag bitset toward other (0 for self or
// out-of-range).
func (w *World) Alliance(source, other uint8) uint16 {
	if source >= MaxPlayers || other >= MaxPlayers || source == other {
		return 0
	}
	return w.players.alliance[source][other]
}

// HasAllianceFlag reports whether source has flag set toward other.
func (w *World) HasAllianceFlag(source, other uint8, flag uint16) bool {
	return w.Alliance(source, other)&flag != 0
}

// IsAlly reports whether source is passive (not at war) toward other. A
// player is neither ally nor enemy of itself.
func (w *World) IsAlly(source, other uint8) bool {
	if source == other {
		return false
	}
	return w.HasAllianceFlag(source, other, AlliancePassive)
}

// IsEnemy reports whether source is at war with other (the complement of
// IsAlly over distinct players).
func (w *World) IsEnemy(source, other uint8) bool {
	if source == other || source >= MaxPlayers || other >= MaxPlayers {
		return false
	}
	return !w.HasAllianceFlag(source, other, AlliancePassive)
}

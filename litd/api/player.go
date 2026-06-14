package litd

// Players & forces (#218; jass-mapping/players-and-forces.md). The
// player/playerstate/playercolor/alliance native families collapse onto
// methods of the Player value type. Per the dedup policy:
//   D5 — GetPlayerState/SetPlayerState × PLAYER_STATE_* become typed
//        accessors (Gold/Lumber/FoodCap…).
//   D2 — the SetPlayerAlliance*BJ preset zoo collapses onto one bitset
//        call, SetAlliance(other, AllianceFlags).
//   D1 — passthrough BJs (GetPlayerNameBJ…) drop onto the plain method.
// Alliance stays one-directional (porting hazard 2): A→B is independent
// of B→A. GetLocalPlayer is structurally eliminated — per-player
// presentation takes the player as a parameter, never a global fork.

import "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"

// resource indices into the economy ledger (gold/lumber are the two
// always-present counters; see sim.BindEconomy).
const (
	resGold   = 0
	resLumber = 1
)

// Controller names how a player slot is driven. Mirrors JASS mapcontrol
// (USER=0 … NONE=5); the sim stores its own ordering, mapped here.
type Controller uint8

const (
	ControllerUser Controller = iota
	ControllerComputer
	ControllerRescuable
	ControllerNeutral
	ControllerCreep
	ControllerNone
)

// AllianceFlags is the per-direction alliance relationship bitset. The
// bit positions mirror JASS alliancetype so a Go script reads like the
// original. Combine with |.
type AllianceFlags uint16

const (
	AllyPassive          AllianceFlags = 1 << 0 // not at war (the "ally" bit)
	AllyHelpRequest      AllianceFlags = 1 << 1
	AllyHelpResponse     AllianceFlags = 1 << 2
	AllySharedXP         AllianceFlags = 1 << 3
	AllySharedSpells     AllianceFlags = 1 << 4
	AllySharedVision     AllianceFlags = 1 << 5
	AllySharedControl    AllianceFlags = 1 << 6
	AllySharedAdvControl AllianceFlags = 1 << 7
	AllyRescuable        AllianceFlags = 1 << 8
	AllySharedVisionForce AllianceFlags = 1 << 9
)

// controllerToSim / controllerFromSim bridge the public mapcontrol order
// to the sim's zero-is-none ordering.
func controllerToSim(c Controller) uint8 {
	switch c {
	case ControllerUser:
		return sim.ControllerUser
	case ControllerComputer:
		return sim.ControllerComputer
	case ControllerRescuable:
		return sim.ControllerRescuable
	case ControllerNeutral:
		return sim.ControllerNeutral
	case ControllerCreep:
		return sim.ControllerCreep
	default:
		return sim.ControllerNone
	}
}

func controllerFromSim(c uint8) Controller {
	switch c {
	case sim.ControllerUser:
		return ControllerUser
	case sim.ControllerComputer:
		return ControllerComputer
	case sim.ControllerRescuable:
		return ControllerRescuable
	case sim.ControllerNeutral:
		return ControllerNeutral
	case sim.ControllerCreep:
		return ControllerCreep
	default:
		return ControllerNone
	}
}

// Player returns the handle for player slot, or the zero-value Player
// (no-op) when slot is outside the fixed range. JASS: Player(n).
func (g *Game) Player(slot int) Player {
	if g == nil || g.w == nil || slot < 0 || slot >= sim.MaxPlayers {
		return Player{}
	}
	return Player{idx: int32(slot), g: g}
}

// Neutral player slots (porting hazard 3): the four fixed high slots.
// In the 16-slot model these are the top four.
func (g *Game) NeutralHostile() Player { return g.Player(sim.MaxPlayers - 4) } // aggressive
func (g *Game) NeutralVictim() Player  { return g.Player(sim.MaxPlayers - 3) }
func (g *Game) NeutralExtra() Player   { return g.Player(sim.MaxPlayers - 2) }
func (g *Game) NeutralPassive() Player { return g.Player(sim.MaxPlayers - 1) }

// Slot returns the player's slot index, or -1 on an invalid handle.
func (p Player) Slot() int {
	if !p.Valid() {
		return -1
	}
	return int(p.idx)
}

// SetName sets the player's display name. JASS: there is no SetPlayerName
// native; this is the writable companion to GetPlayerName for the
// world-builder surface.
func (p Player) SetName(s string) {
	if !p.Valid() {
		p.g.reportInvalid("Player.SetName")
		return
	}
	p.g.w.SetPlayerName(uint8(p.idx), s)
}

// ---- D5 player-state accessors ----

// Gold / SetGold read and write PLAYER_STATE_RESOURCE_GOLD.
func (p Player) Gold() int {
	if !p.Valid() {
		return 0
	}
	return int(p.g.w.Resources(uint8(p.idx), resGold))
}
func (p Player) SetGold(v int) {
	if !p.Valid() {
		p.g.reportInvalid("Player.SetGold")
		return
	}
	p.g.w.SetResource(uint8(p.idx), resGold, int64(v))
}

// Lumber / SetLumber read and write PLAYER_STATE_RESOURCE_LUMBER.
func (p Player) Lumber() int {
	if !p.Valid() {
		return 0
	}
	return int(p.g.w.Resources(uint8(p.idx), resLumber))
}
func (p Player) SetLumber(v int) {
	if !p.Valid() {
		p.g.reportInvalid("Player.SetLumber")
		return
	}
	p.g.w.SetResource(uint8(p.idx), resLumber, int64(v))
}

// FoodUsed reads PLAYER_STATE_RESOURCE_FOOD_USED (derived from live
// units; read-only).
func (p Player) FoodUsed() int {
	if !p.Valid() {
		return 0
	}
	return int(p.g.w.FoodUsed(uint8(p.idx)))
}

// FoodCap / SetFoodCap read and write PLAYER_STATE_RESOURCE_FOOD_CAP.
func (p Player) FoodCap() int {
	if !p.Valid() {
		return 0
	}
	return int(p.g.w.FoodCap(uint8(p.idx)))
}
func (p Player) SetFoodCap(v int) {
	if !p.Valid() {
		p.g.reportInvalid("Player.SetFoodCap")
		return
	}
	p.g.w.SetFoodCap(uint8(p.idx), int32(v))
}

// Race / SetRace read and write the player's race. JASS: GetPlayerRace /
// SetPlayerRacePreference (the preference collapses to the race).
func (p Player) Race() Race {
	if !p.Valid() {
		return RaceNone
	}
	return Race(p.g.w.PlayerRace(uint8(p.idx)))
}
func (p Player) SetRace(r Race) {
	if !p.Valid() {
		p.g.reportInvalid("Player.SetRace")
		return
	}
	p.g.w.SetPlayerRace(uint8(p.idx), uint8(r))
}

// Color / SetColor read and write the player's color slot (0..23). JASS:
// GetPlayerColor / SetPlayerColor.
func (p Player) Color() int {
	if !p.Valid() {
		return 0
	}
	return int(p.g.w.PlayerColor(uint8(p.idx)))
}
func (p Player) SetColor(c int) {
	if !p.Valid() {
		p.g.reportInvalid("Player.SetColor")
		return
	}
	if c < 0 {
		c = 0
	}
	p.g.w.SetPlayerColor(uint8(p.idx), uint8(c))
}

// Team / SetTeam read and write the player's roster team (FFA/scoring
// metadata; does not re-team already-spawned units). JASS: GetPlayerTeam
// / SetPlayerTeam.
func (p Player) Team() int {
	if !p.Valid() {
		return 0
	}
	return int(p.g.w.PlayerTeam(uint8(p.idx)))
}
func (p Player) SetTeam(t int) {
	if !p.Valid() {
		p.g.reportInvalid("Player.SetTeam")
		return
	}
	if t < 0 {
		t = 0
	}
	p.g.w.SetPlayerTeam(uint8(p.idx), uint8(t))
}

// Controller / SetController read and write how the slot is driven.
// JASS: GetPlayerController / SetPlayerController (via mapcontrol).
func (p Player) Controller() Controller {
	if !p.Valid() {
		return ControllerNone
	}
	return controllerFromSim(p.g.w.Controller(uint8(p.idx)))
}
func (p Player) SetController(c Controller) {
	if !p.Valid() {
		p.g.reportInvalid("Player.SetController")
		return
	}
	p.g.w.SetController(uint8(p.idx), controllerToSim(c))
}

// StartLocation / SetStartLocation read and write the player's start
// point. JASS: GetPlayerStartLocation / start-location coords.
func (p Player) StartLocation() Vec2 {
	if !p.Valid() {
		return Vec2{}
	}
	x, y := p.g.w.PlayerStart(uint8(p.idx))
	return Vec2{X: toFloat(x), Y: toFloat(y)}
}
func (p Player) SetStartLocation(loc Vec2) {
	if !p.Valid() {
		p.g.reportInvalid("Player.SetStartLocation")
		return
	}
	p.g.w.SetPlayerStart(uint8(p.idx), fromFloat(loc.X), fromFloat(loc.Y))
}

// AlliedVictory / SetAlliedVictory read and write
// PLAYER_STATE_ALLIED_VICTORY.
func (p Player) AlliedVictory() bool {
	if !p.Valid() {
		return false
	}
	return p.g.w.AlliedVictory(uint8(p.idx))
}
func (p Player) SetAlliedVictory(on bool) {
	if !p.Valid() {
		p.g.reportInvalid("Player.SetAlliedVictory")
		return
	}
	p.g.w.SetAlliedVictory(uint8(p.idx), on)
}

// ---- alliance relation (one-directional) ----

// SetAlliance replaces this player's entire alliance bitset toward
// other. One-directional — it does not touch other→p. The D2 collapse of
// the SetPlayerAllianceState*BJ preset family. No-op for an invalid or
// foreign other, or for self.
func (p Player) SetAlliance(other Player, flags AllianceFlags) {
	if !p.Valid() {
		p.g.reportInvalid("Player.SetAlliance")
		return
	}
	if other.g != p.g || !other.Valid() {
		p.g.reportInvalid("Player.SetAlliance (other not from this game)")
		return
	}
	p.g.w.SetAlliance(uint8(p.idx), uint8(other.idx), uint16(flags))
}

// SetAllianceFlag sets or clears a single alliance bit toward other,
// leaving the rest intact. JASS: SetPlayerAlliance(p, other, type, bool).
func (p Player) SetAllianceFlag(other Player, flag AllianceFlags, on bool) {
	if !p.Valid() {
		p.g.reportInvalid("Player.SetAllianceFlag")
		return
	}
	if other.g != p.g || !other.Valid() {
		p.g.reportInvalid("Player.SetAllianceFlag (other not from this game)")
		return
	}
	p.g.w.SetAllianceFlag(uint8(p.idx), uint8(other.idx), uint16(flag), on)
}

// AllianceWith returns this player's raw alliance bitset toward other
// (0 for self/foreign/invalid). JASS: GetPlayerAlliance readback.
func (p Player) AllianceWith(other Player) AllianceFlags {
	if !p.Valid() || other.g != p.g || !other.Valid() {
		return 0
	}
	return AllianceFlags(p.g.w.Alliance(uint8(p.idx), uint8(other.idx)))
}

// IsAlly reports whether this player is passive (not at war) toward
// other. JASS: IsPlayerAlly.
func (p Player) IsAlly(other Player) bool {
	if !p.Valid() || other.g != p.g || !other.Valid() {
		return false
	}
	return p.g.w.IsAlly(uint8(p.idx), uint8(other.idx))
}

// IsEnemy reports whether this player is at war with other. JASS:
// IsPlayerEnemy.
func (p Player) IsEnemy(other Player) bool {
	if !p.Valid() || other.g != p.g || !other.Valid() {
		return false
	}
	return p.g.w.IsEnemy(uint8(p.idx), uint8(other.idx))
}

// ---- difficulty handicaps (#373) ----
//
// Each handicap is a fraction where 1.0 = no effect (the JASS native form;
// the *BJ percent variants pass 60.0 for 0.60 and collapse onto these).
// They read and write the real applied multipliers — a handicapped
// player's units deal/take scaled damage, its heroes earn scaled XP, and
// its hero revives take scaled time. Negative inputs clamp to 0.

// Handicap / SetHandicap is the damage-TAKEN multiplier for this player's
// units. JASS: GetPlayerHandicap / SetPlayerHandicap.
func (p Player) Handicap() float64 {
	if !p.Valid() {
		return 1
	}
	return toFloat(p.g.w.Handicap(uint8(p.idx)))
}
func (p Player) SetHandicap(v float64) {
	if !p.Valid() {
		p.g.reportInvalid("Player.SetHandicap")
		return
	}
	p.g.w.SetHandicap(uint8(p.idx), fromFloat(v))
}

// HandicapDamage / SetHandicapDamage is the damage-DEALT multiplier for
// this player's units. JASS: GetPlayerHandicapDamage / SetPlayerHandicapDamage.
func (p Player) HandicapDamage() float64 {
	if !p.Valid() {
		return 1
	}
	return toFloat(p.g.w.HandicapDamage(uint8(p.idx)))
}
func (p Player) SetHandicapDamage(v float64) {
	if !p.Valid() {
		p.g.reportInvalid("Player.SetHandicapDamage")
		return
	}
	p.g.w.SetHandicapDamage(uint8(p.idx), fromFloat(v))
}

// HandicapXP / SetHandicapXP is the kill-XP multiplier for this player's
// heroes. JASS: GetPlayerHandicapXP / SetPlayerHandicapXP.
func (p Player) HandicapXP() float64 {
	if !p.Valid() {
		return 1
	}
	return toFloat(p.g.w.HandicapXP(uint8(p.idx)))
}
func (p Player) SetHandicapXP(v float64) {
	if !p.Valid() {
		p.g.reportInvalid("Player.SetHandicapXP")
		return
	}
	p.g.w.SetHandicapXP(uint8(p.idx), fromFloat(v))
}

// HandicapReviveTime / SetHandicapReviveTime is the hero-revive-time
// multiplier for this player. JASS: GetPlayerHandicapReviveTime /
// SetPlayerHandicapReviveTime.
func (p Player) HandicapReviveTime() float64 {
	if !p.Valid() {
		return 1
	}
	return toFloat(p.g.w.HandicapReviveTime(uint8(p.idx)))
}
func (p Player) SetHandicapReviveTime(v float64) {
	if !p.Valid() {
		p.g.reportInvalid("Player.SetHandicapReviveTime")
		return
	}
	p.g.w.SetHandicapReviveTime(uint8(p.idx), fromFloat(v))
}

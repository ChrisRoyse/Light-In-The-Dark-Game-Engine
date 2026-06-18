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

// The Controller values name how a player slot is driven (see Controller).
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

// The Ally* bits are the per-direction alliance relationship flags (see
// AllianceFlags); their positions mirror JASS alliancetype. Combine with |.
const (
	AllyPassive           AllianceFlags = 1 << 0 // not at war (the "ally" bit)
	AllyHelpRequest       AllianceFlags = 1 << 1
	AllyHelpResponse      AllianceFlags = 1 << 2
	AllySharedXP          AllianceFlags = 1 << 3
	AllySharedSpells      AllianceFlags = 1 << 4
	AllySharedVision      AllianceFlags = 1 << 5
	AllySharedControl     AllianceFlags = 1 << 6
	AllySharedAdvControl  AllianceFlags = 1 << 7
	AllyRescuable         AllianceFlags = 1 << 8
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
// JASS: Player
func (g *Game) Player(slot int) Player {
	if g == nil || g.w == nil || slot < 0 || slot >= sim.MaxPlayers {
		return Player{}
	}
	return Player{idx: int32(slot), g: g}
}

// Neutral player slots (porting hazard 3): the four fixed high slots.
// In the 16-slot model these are the top four.
// JASS: GetPlayerNeutralAggressive
func (g *Game) NeutralHostile() Player { return g.Player(sim.MaxPlayers - 4) } // aggressive

// NeutralVictim returns the second-from-top fixed neutral slot (porting hazard 3).
func (g *Game) NeutralVictim() Player { return g.Player(sim.MaxPlayers - 3) }

// NeutralExtra returns the third-from-top fixed neutral slot (porting hazard 3).
func (g *Game) NeutralExtra() Player { return g.Player(sim.MaxPlayers - 2) }

// NeutralPassive returns the top fixed neutral slot — the passive owner of
// shops and critters (porting hazard 3).
// JASS: GetPlayerNeutralPassive
func (g *Game) NeutralPassive() Player { return g.Player(sim.MaxPlayers - 1) }

// Slot returns the player's slot index, or -1 on an invalid handle.
// JASS: GetPlayerId
func (p Player) Slot() int {
	if !p.Valid() {
		return -1
	}
	return int(p.idx)
}

// SetName sets the player's display name. JASS: there is no SetPlayerName
// native; this is the writable companion to GetPlayerName for the
// world-builder surface.
// JASS: SetPlayerName
func (p Player) SetName(s string) {
	if !p.Valid() {
		p.g.reportInvalid("Player.SetName")
		return
	}
	p.g.w.SetPlayerName(uint8(p.idx), s)
}

// ---- D5 player-state accessors ----

// Gold / SetGold read and write PLAYER_STATE_RESOURCE_GOLD. The resource ledger
// is unallocated until Game.DefineEconomy is called (#388/#396): before that,
// Gold reads 0 and SetGold is a no-op — by design, not a bug. A game that uses
// resources must call DefineEconomy during setup.
// JASS: GetPlayerState, GetPlayerStateBJ
func (p Player) Gold() int {
	if !p.Valid() {
		return 0
	}
	return int(p.g.w.Resources(uint8(p.idx), resGold))
}

// SetGold writes PLAYER_STATE_RESOURCE_GOLD (see Gold). No-op on an invalid
// handle, or until Game.DefineEconomy initialises the resource ledger (#388).
// JASS: SetPlayerState, SetPlayerStateBJ
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

// SetLumber writes PLAYER_STATE_RESOURCE_LUMBER (see Lumber). No-op on an
// invalid handle, or until Game.DefineEconomy initialises the resource ledger (#388).
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

// SetFoodCap writes PLAYER_STATE_RESOURCE_FOOD_CAP (see FoodCap). No-op on an invalid handle.
func (p Player) SetFoodCap(v int) {
	if !p.Valid() {
		p.g.reportInvalid("Player.SetFoodCap")
		return
	}
	p.g.w.SetFoodCap(uint8(p.idx), int32(v))
}

// Race / SetRace read and write the player's race. JASS: GetPlayerRace /
// SetPlayerRacePreference (the preference collapses to the race).
// JASS: GetPlayerRace
func (p Player) Race() Race {
	if !p.Valid() {
		return RaceNone
	}
	return Race(p.g.w.PlayerRace(uint8(p.idx)))
}

// SetRace writes the player's race (see Race). JASS: SetPlayerRacePreference.
// No-op on an invalid handle.
// JASS: SetPlayerRacePreference, SetPlayerRaceSelectable
func (p Player) SetRace(r Race) {
	if !p.Valid() {
		p.g.reportInvalid("Player.SetRace")
		return
	}
	p.g.w.SetPlayerRace(uint8(p.idx), uint8(r))
}

// Color / SetColor read and write the player's color slot (0..23). JASS:
// GetPlayerColor / SetPlayerColor.
// JASS: GetPlayerColor
func (p Player) Color() int {
	if !p.Valid() {
		return 0
	}
	return int(p.g.w.PlayerColor(uint8(p.idx)))
}

// SetColor writes the player's color slot, clamped to >= 0 (see Color).
// JASS: SetPlayerColor, SetPlayerColorBJ
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
// JASS: GetPlayerTeam
func (p Player) Team() int {
	if !p.Valid() {
		return 0
	}
	return int(p.g.w.PlayerTeam(uint8(p.idx)))
}

// SetTeam writes the player's roster team, clamped to >= 0 (see Team). Roster
// metadata only — does not re-team already-spawned units. JASS: SetPlayerTeam.
// No-op on an invalid handle.
// JASS: SetPlayerTeam
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
// JASS: GetPlayerController
func (p Player) Controller() Controller {
	if !p.Valid() {
		return ControllerNone
	}
	return controllerFromSim(p.g.w.Controller(uint8(p.idx)))
}

// SetController writes how the slot is driven (see Controller). JASS:
// SetPlayerController. No-op on an invalid handle.
// JASS: SetPlayerController
func (p Player) SetController(c Controller) {
	if !p.Valid() {
		p.g.reportInvalid("Player.SetController")
		return
	}
	p.g.w.SetController(uint8(p.idx), controllerToSim(c))
}

// StartLocation / SetStartLocation read and write the player's start
// point. JASS: GetPlayerStartLocation / start-location coords.
// JASS: GetPlayerStartLocation, GetPlayerStartLocationLoc, GetPlayerStartLocationX, GetPlayerStartLocationY
func (p Player) StartLocation() Vec2 {
	if !p.Valid() {
		return Vec2{}
	}
	x, y := p.g.w.PlayerStart(uint8(p.idx))
	return Vec2{X: toFloat(x), Y: toFloat(y)}
}

// SetStartLocation writes the player's start point (see StartLocation). No-op
// on an invalid handle.
// JASS: ForcePlayerStartLocation, SetPlayerStartLocation
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

// SetAlliedVictory writes PLAYER_STATE_ALLIED_VICTORY (see AlliedVictory).
// No-op on an invalid handle.
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
// JASS: SetForceAllianceStateBJ, SetPlayerAlliance, SetPlayerAllianceBJ, SetPlayerAllianceStateAllyBJ, SetPlayerAllianceStateBJ, SetPlayerAllianceStateControlBJ, SetPlayerAllianceStateFullControlBJ, SetPlayerAllianceStateVisionBJ
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
// JASS: GetPlayerAlliance
func (p Player) AllianceWith(other Player) AllianceFlags {
	if !p.Valid() || other.g != p.g || !other.Valid() {
		return 0
	}
	return AllianceFlags(p.g.w.Alliance(uint8(p.idx), uint8(other.idx)))
}

// IsAlly reports whether this player is passive (not at war) toward
// other. JASS: IsPlayerAlly.
// JASS: IsPlayerAlly, PlayersAreCoAllied
func (p Player) IsAlly(other Player) bool {
	if !p.Valid() || other.g != p.g || !other.Valid() {
		return false
	}
	return p.g.w.IsAlly(uint8(p.idx), uint8(other.idx))
}

// IsEnemy reports whether this player is at war with other. JASS:
// IsPlayerEnemy.
// JASS: IsPlayerEnemy
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
// JASS: GetPlayerHandicap, GetPlayerHandicapBJ
func (p Player) Handicap() float64 {
	if !p.Valid() {
		return 1
	}
	return toFloat(p.g.w.Handicap(uint8(p.idx)))
}

// SetHandicap writes the damage-taken multiplier (see Handicap). 1.0 = no
// effect; negative inputs clamp to 0. No-op on an invalid handle.
// JASS: SetPlayerHandicap, SetPlayerHandicapBJ
func (p Player) SetHandicap(v float64) {
	if !p.Valid() {
		p.g.reportInvalid("Player.SetHandicap")
		return
	}
	p.g.w.SetHandicap(uint8(p.idx), fromFloat(v))
}

// HandicapDamage / SetHandicapDamage is the damage-DEALT multiplier for
// this player's units. JASS: GetPlayerHandicapDamage / SetPlayerHandicapDamage.
// JASS: GetPlayerHandicapDamage, GetPlayerHandicapDamageBJ
func (p Player) HandicapDamage() float64 {
	if !p.Valid() {
		return 1
	}
	return toFloat(p.g.w.HandicapDamage(uint8(p.idx)))
}

// SetHandicapDamage writes the damage-dealt multiplier (see HandicapDamage).
// 1.0 = no effect; negative inputs clamp to 0. No-op on an invalid handle.
// JASS: SetPlayerHandicapDamage, SetPlayerHandicapDamageBJ
func (p Player) SetHandicapDamage(v float64) {
	if !p.Valid() {
		p.g.reportInvalid("Player.SetHandicapDamage")
		return
	}
	p.g.w.SetHandicapDamage(uint8(p.idx), fromFloat(v))
}

// HandicapXP / SetHandicapXP is the kill-XP multiplier for this player's
// heroes. JASS: GetPlayerHandicapXP / SetPlayerHandicapXP.
// JASS: GetPlayerHandicapXP, GetPlayerHandicapXPBJ
func (p Player) HandicapXP() float64 {
	if !p.Valid() {
		return 1
	}
	return toFloat(p.g.w.HandicapXP(uint8(p.idx)))
}

// SetHandicapXP writes the hero kill-XP multiplier (see HandicapXP). 1.0 = no
// effect; negative inputs clamp to 0. No-op on an invalid handle.
// JASS: SetPlayerHandicapXP, SetPlayerHandicapXPBJ
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
// JASS: GetPlayerHandicapReviveTime, GetPlayerHandicapReviveTimeBJ
func (p Player) HandicapReviveTime() float64 {
	if !p.Valid() {
		return 1
	}
	return toFloat(p.g.w.HandicapReviveTime(uint8(p.idx)))
}

// SetHandicapReviveTime writes the hero-revive-time multiplier (see
// HandicapReviveTime). 1.0 = no effect; negative inputs clamp to 0. No-op on an
// invalid handle.
// JASS: SetPlayerHandicapReviveTime, SetPlayerHandicapReviveTimeBJ
func (p Player) SetHandicapReviveTime(v float64) {
	if !p.Valid() {
		p.g.reportInvalid("Player.SetHandicapReviveTime")
		return
	}
	p.g.w.SetHandicapReviveTime(uint8(p.idx), fromFloat(v))
}

// ---- upkeep economy + transfer tax (#375) ----

// UpkeepTier is one food-tier income-tax bracket: when a player's food
// usage reaches Food, harvested deposits of resource r are taxed by Rate[r]
// (a fraction in [0,1]). Rate is indexed by resource (0=gold, 1=lumber, …);
// a short or nil slice taxes the omitted resources at 0. Tiers passed to
// Game.SetUpkeep must be in ascending Food order.
type UpkeepTier struct {
	// Food is the supply count at or above which this upkeep tier applies.
	Food int
	// Rate is the per-resource income tax fraction charged in this tier
	// (indexed by resource).
	Rate []float64
}

// SetUpkeep installs the food-tier upkeep brackets (JASS upkeep economy
// behind PLAYER_STATE_*_UPKEEP_RATE). Ascending Food order required; an
// empty slice clears upkeep (no tax). Returns false (no change) if the
// tiers are invalid. Brackets are global (apply to every player by their
// own food usage), mirroring WC3.
func (g *Game) SetUpkeep(tiers []UpkeepTier) bool {
	if g == nil || g.w == nil {
		return false
	}
	st := make([]sim.UpkeepTier, len(tiers))
	for i, t := range tiers {
		st[i].Food = int32(t.Food)
		for r := 0; r < len(t.Rate) && r < len(st[i].Rate); r++ {
			st[i].Rate[r] = fromFloat(t.Rate[r])
		}
	}
	return g.w.BindUpkeep(st)
}

// UpkeepRate reads the tax fraction currently applied to this player's
// deposits of the given resource, derived live from food usage
// (PLAYER_STATE_GOLD_UPKEEP_RATE / PLAYER_STATE_LUMBER_UPKEEP_RATE).
func (p Player) UpkeepRate(resource int) float64 {
	if !p.Valid() {
		return 0
	}
	return toFloat(p.g.w.UpkeepRate(uint8(p.idx), resource))
}

// GoldUpkeepRate / LumberUpkeepRate are the gold/lumber conveniences for
// UpkeepRate (the two WC3 upkeep player-states).
func (p Player) GoldUpkeepRate() float64 { return p.UpkeepRate(resGold) }

// LumberUpkeepRate is the lumber convenience for UpkeepRate.
func (p Player) LumberUpkeepRate() float64 { return p.UpkeepRate(resLumber) }

// LostToUpkeep reads the cumulative amount of the given resource this player
// has had withheld by the upkeep tax (PLAYER_SCORE_*_LOST_UPKEEP).
func (p Player) LostToUpkeep(resource int) int {
	if !p.Valid() {
		return 0
	}
	return int(p.g.w.UpkeepLost(uint8(p.idx), resource))
}

// GoldLostToUpkeep / LumberLostToUpkeep are the gold/lumber conveniences.
func (p Player) GoldLostToUpkeep() int { return p.LostToUpkeep(resGold) }

// LumberLostToUpkeep is the lumber convenience for LostToUpkeep.
func (p Player) LumberLostToUpkeep() int { return p.LostToUpkeep(resLumber) }

// TaxRate / SetTaxRate read and write this player's transfer-tax fraction
// toward another player for one resource (GetPlayerTaxRate / SetPlayerTaxRate).
// The tax is withheld in Game.TransferResource. A player has no tax toward
// itself. Rate clamps to [0,1].
// JASS: GetPlayerTaxRate, GetPlayerTaxRateBJ
func (p Player) TaxRate(other Player, resource int) float64 {
	if !p.Valid() || !other.Valid() {
		return 0
	}
	return toFloat(p.g.w.TaxRate(uint8(p.idx), uint8(other.idx), resource))
}

// SetTaxRate writes this player's transfer-tax fraction toward other for one
// resource (see TaxRate). Rate clamps to [0,1]; no tax toward itself. JASS:
// SetPlayerTaxRate. No-op on an invalid handle.
// JASS: SetPlayerTaxRate, SetPlayerTaxRateBJ
func (p Player) SetTaxRate(other Player, resource int, rate float64) {
	if !p.Valid() || !other.Valid() {
		p.g.reportInvalid("Player.SetTaxRate")
		return
	}
	p.g.w.SetTaxRate(uint8(p.idx), uint8(other.idx), resource, fromFloat(rate))
}

// TransferResource moves up to amount of a resource from one player to
// another, applying the giver's transfer tax toward the receiver. The full
// amount leaves the giver; the taxed fraction is destroyed; the remainder
// is credited to the receiver. Returns the net amount delivered.
func (g *Game) TransferResource(from, to Player, resource, amount int) int {
	if g == nil || g.w == nil || !from.Valid() || !to.Valid() {
		return 0
	}
	return int(g.w.TransferResource(uint8(from.idx), uint8(to.idx), resource, int64(amount)))
}

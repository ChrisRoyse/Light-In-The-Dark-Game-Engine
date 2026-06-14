package litd

import "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"

// unit.go is the canonical units-category surface (jass-mapping/units.md):
// the ~235 common.j unit natives + ~125 blizzard.j BJs collapse onto methods
// of Unit (and Game.CreateUnit). Life/MaxLife/SetLife/Owner live in
// handle_validity.go; the value math and Valid()/IsZero() skeleton in
// handles.go. This file adds the lifecycle and transform verbs.
//
// Every verb keeps WC3's forgiving semantics (R-API-5): a call on an invalid
// or destroyed handle is a silent no-op (debug mode reports the call site),
// getters return the zero value. Reads/writes go straight to the sim component
// stores — the deterministic Source of Truth — never to render (R-API-6).

// UnitType names a bound unit type — the WC3 unit code ('hfoo') resolved to a
// stable ref. Obtain one from Game.UnitType; the zero value is the null type
// (rejected by CreateUnit). Opaque value type (D-2026-06-13 / #361), mirroring
// AbilityRef.
type UnitType struct {
	ref uint16 // typeID + 1; 0 = null
}

// IsZero reports whether this is the null unit type.
func (t UnitType) IsZero() bool { return t.ref == 0 }

// UnitType resolves a unit code (e.g. "hfoo") to its bound type, or the null
// UnitType if the code is unknown or no unit table is bound. JASS: the 'xxxx'
// rawcodes passed to CreateUnit.
func (g *Game) UnitType(code string) UnitType {
	if g == nil || g.w == nil {
		return UnitType{}
	}
	if id, ok := g.w.UnitTypeID(code); ok {
		return UnitType{ref: id + 1}
	}
	return UnitType{}
}

// CreateUnit spawns a unit of type typ for owner at pos facing the given angle,
// returning its handle (the zero Unit on failure — null/unknown type, foreign
// owner, or the unit cap reached). JASS: CreateUnit, CreateUnitAtLoc and the
// CreateUnitAtLocSaveLast family collapse here (D2); the returned handle
// replaces the bj_lastCreatedUnit side channel (GetLastCreatedUnit tombstoned).
//
// Team currently defaults to the owner's player slot (FFA); alliance/team
// assignment lands with players-and-forces (#218), per the #361 decision.
func (g *Game) CreateUnit(owner Player, typ UnitType, pos Vec2, facing Angle) Unit {
	if g == nil || g.w == nil {
		return Unit{}
	}
	if typ.IsZero() {
		g.reportInvalid("Game.CreateUnit (null UnitType)")
		return Unit{}
	}
	if owner.g != g {
		g.reportInvalid("Game.CreateUnit (owner not from this game)")
		return Unit{}
	}
	slot := uint8(owner.idx)
	id, ok := g.w.SpawnFromTable(typ.ref-1, slot, slot, vec(pos))
	if !ok {
		g.reportInvalid("Game.CreateUnit (spawn failed: unit cap or unbound type)")
		return Unit{}
	}
	if r := g.w.Transforms.Row(id); r >= 0 {
		g.w.Transforms.Facing[r] = angleToBrad(facing)
		g.w.MarkSnap(id)
	}
	return Unit{id: id, g: g}
}

// Type returns the unit's bound type — the same UnitType passed to CreateUnit —
// or the null UnitType on an invalid handle or a unit with no type row. The
// returned value round-trips with Game.CreateUnit and Game.UnitType. JASS:
// GetUnitTypeId returns the raw integer code; here it surfaces as the opaque
// UnitType (the integer rawcode never leaks into the public surface, #361).
func (u Unit) Type() UnitType {
	if !u.Valid() {
		u.g.reportInvalid("Unit.Type")
		return UnitType{}
	}
	r := u.g.w.UnitTypes.Row(u.id)
	if r < 0 {
		return UnitType{}
	}
	return UnitType{ref: u.g.w.UnitTypes.TypeID[r] + 1}
}

// UserData returns the unit's custom value — an arbitrary integer scripts
// attach for their own bookkeeping (the sim never reads it). Zero on an invalid
// handle or a unit that was never assigned one. JASS: GetUnitUserData.
func (u Unit) UserData() int {
	if !u.Valid() {
		u.g.reportInvalid("Unit.UserData")
		return 0
	}
	return int(u.g.w.UserData(u.id))
}

// SetUserData assigns the unit's custom value. No-op on an invalid handle.
// JASS: SetUnitUserData.
func (u Unit) SetUserData(v int) {
	if !u.Valid() {
		u.g.reportInvalid("Unit.SetUserData")
		return
	}
	u.g.w.SetUserData(u.id, int32(v))
}

// PointValue returns the unit type's score/bounty weight — a static data-table
// property, not per-unit state. Zero on an invalid handle, an untyped unit, or
// a type with no point value. JASS: GetUnitPointValue.
func (u Unit) PointValue() int {
	if !u.Valid() {
		u.g.reportInvalid("Unit.PointValue")
		return 0
	}
	return int(u.g.w.UnitPointValue(u.id))
}

// Level returns the unit's level: a hero's current level for heroes,
// otherwise the type's configured design level. Zero on an invalid
// handle, an untyped unit, or a non-hero type with no level set.
// JASS: GetUnitLevel.
func (u Unit) Level() int {
	if !u.Valid() {
		u.g.reportInvalid("Unit.Level")
		return 0
	}
	return int(u.g.w.UnitLevel(u.id))
}

// Invulnerable reports whether the unit currently ignores all incoming damage,
// false on an invalid handle or a unit with no health row. JASS:
// BlzIsUnitInvulnerable.
func (u Unit) Invulnerable() bool {
	if !u.Valid() {
		u.g.reportInvalid("Unit.Invulnerable")
		return false
	}
	r := u.g.w.Healths.Row(u.id)
	if r < 0 {
		return false
	}
	return u.g.w.Healths.Invulnerable[r]
}

// SetInvulnerable sets the unit's damage immunity. While on, every incoming
// damage packet is skipped at the damage step — the unit takes no damage, is
// not killed by damage, and grants no kill XP. No-op on an invalid handle or a
// unit with no health row. JASS: SetUnitInvulnerable.
func (u Unit) SetInvulnerable(on bool) {
	if !u.Valid() {
		u.g.reportInvalid("Unit.SetInvulnerable")
		return
	}
	r := u.g.w.Healths.Row(u.id)
	if r < 0 {
		return
	}
	u.g.w.Healths.Invulnerable[r] = on
}

// Armor returns the unit's effective armor — the base armor value plus any
// active buff/debuff modifiers (the displayed armor), or 0 on an invalid handle
// or a unit with no health row. JASS: BlzGetUnitArmor.
func (u Unit) Armor() float64 {
	if !u.Valid() {
		u.g.reportInvalid("Unit.Armor")
		return 0
	}
	r := u.g.w.Healths.Row(u.id)
	if r < 0 {
		return 0
	}
	return float64(u.g.w.BuffedArmor(u.id, int(u.g.w.Healths.ArmorValue[r])))
}

// Alive reports whether the unit is alive — a valid handle with positive life.
// A corpse (life 0, awaiting decay) and an invalid/removed handle are both not
// alive. This is the WC3 life-based definition (UNIT_STATE_LIFE > 0); the
// dead-check is its complement (!Alive). JASS: IsUnitAliveBJ, IsUnitDeadBJ.
func (u Unit) Alive() bool {
	if !u.Valid() {
		return false
	}
	r := u.g.w.Healths.Row(u.id)
	if r < 0 {
		return false
	}
	return u.g.w.Healths.Life[r] > 0
}

// Position returns the unit's current world position, or the zero Vec2 on an
// invalid handle. JASS: GetUnitX/GetUnitY, GetUnitLoc (D3 → one Vec2).
func (u Unit) Position() Vec2 {
	if !u.Valid() {
		u.g.reportInvalid("Unit.Position")
		return Vec2{}
	}
	r := u.g.w.Transforms.Row(u.id)
	if r < 0 {
		return Vec2{}
	}
	p := u.g.w.Transforms.Pos[r]
	return Vec2{X: toFloat(p.X), Y: toFloat(p.Y)}
}

// PositionOption configures SetPosition (R-API-3 functional option).
type PositionOption func(*positionConfig)

type positionConfig struct {
	skipPathing bool
}

// Teleport places the unit at the exact target, ignoring pathing and
// collision — the raw SetUnitX/SetUnitY capability. Without it, SetPosition
// respects static pathing and nudges an unpathable target to the nearest
// walkable cell (units.md hazard 3: the capability is preserved, not averaged
// away).
func Teleport() PositionOption {
	return func(c *positionConfig) { c.skipPathing = true }
}

// SetPosition relocates the unit. By default it respects static pathing
// (the SetUnitPosition semantics): a target on an unpathable cell is nudged to
// the nearest walkable cell; an already-pathable target keeps its exact
// coordinates. With Teleport() it places raw (the SetUnitX/SetUnitY teleport
// semantics). No-op on an invalid handle. JASS: SetUnitX, SetUnitY,
// SetUnitPosition, SetUnitPositionLoc all collapse here (D3).
func (u Unit) SetPosition(pos Vec2, opts ...PositionOption) {
	if !u.Valid() {
		u.g.reportInvalid("Unit.SetPosition")
		return
	}
	var c positionConfig
	for _, o := range opts {
		o(&c)
	}
	u.g.w.PlaceUnit(u.id, vec(pos), c.skipPathing)
}

// Facing returns the unit's facing angle, or the zero Angle on an invalid
// handle. JASS: GetUnitFacing.
func (u Unit) Facing() Angle {
	if !u.Valid() {
		u.g.reportInvalid("Unit.Facing")
		return Angle{}
	}
	r := u.g.w.Transforms.Row(u.id)
	if r < 0 {
		return Angle{}
	}
	return angleFromBrad(u.g.w.Transforms.Facing[r])
}

// SetFacing instantly orients the unit to a. This is the snap form
// (SetUnitFacing); the gradual turn-to-face is an order, issued separately.
// No-op on an invalid handle. JASS: SetUnitFacing, SetUnitFacingTimed (the
// timed variant's instant endpoint).
func (u Unit) SetFacing(a Angle) {
	if !u.Valid() {
		u.g.reportInvalid("Unit.SetFacing")
		return
	}
	r := u.g.w.Transforms.Row(u.id)
	if r < 0 {
		return
	}
	u.g.w.Transforms.Facing[r] = angleToBrad(a)
	u.g.w.MarkSnap(u.id) // discontinuity: render must not lerp across the snap
}

// Mana returns the unit's current mana, or 0 on an invalid handle or a unit
// with no mana pool (non-casters). D5: GetUnitState(UNIT_STATE_MANA).
func (u Unit) Mana() float64 {
	if !u.Valid() {
		u.g.reportInvalid("Unit.Mana")
		return 0
	}
	r := u.g.w.Abilities.Row(u.id)
	if r < 0 {
		return 0
	}
	return toFloat(u.g.w.Abilities.Mana[r])
}

// MaxMana returns the unit's maximum mana, or 0 on an invalid handle / a unit
// with no mana pool. D5: GetUnitState(UNIT_STATE_MAX_MANA).
func (u Unit) MaxMana() float64 {
	if !u.Valid() {
		u.g.reportInvalid("Unit.MaxMana")
		return 0
	}
	r := u.g.w.Abilities.Row(u.id)
	if r < 0 {
		return 0
	}
	return toFloat(u.g.w.Abilities.MaxMana[r])
}

// SetMaxMana sets the unit's maximum mana (D5 typed accessor over the unitstate
// table; JASS SetUnitState with UNIT_STATE_MAX_MANA). The new max floors at 0 —
// a non-caster legitimately has zero max mana. Current mana is left where it is,
// except it is clamped down when the new max drops below it. No-op on an invalid
// handle or a unit without an Ability (mana) component.
func (u Unit) SetMaxMana(v float64) {
	if !u.Valid() {
		u.g.reportInvalid("Unit.SetMaxMana")
		return
	}
	r := u.g.w.Abilities.Row(u.id)
	if r < 0 {
		return
	}
	nv := fromFloat(v)
	if nv < 0 {
		nv = 0
	}
	u.g.w.Abilities.MaxMana[r] = nv
	if u.g.w.Abilities.Mana[r] > nv {
		u.g.w.Abilities.Mana[r] = nv
	}
}

// ManaPercent returns the unit's current mana as a percentage of its
// maximum, in [0,100]. Returns 0 on an invalid handle or a unit with no
// mana pool (non-casters: MaxMana==0). D4: GetUnitManaPercent,
// GetUnitStatePercent(MANA, MAX_MANA) = 100*Mana/MaxMana.
func (u Unit) ManaPercent() float64 {
	if !u.Valid() {
		u.g.reportInvalid("Unit.ManaPercent")
		return 0
	}
	r := u.g.w.Abilities.Row(u.id)
	if r < 0 {
		return 0
	}
	max := u.g.w.Abilities.MaxMana[r]
	if max == 0 {
		return 0
	}
	return toFloat(u.g.w.Abilities.Mana[r]) / toFloat(max) * 100
}

// SetMana sets the unit's current mana, clamped to [0, MaxMana]. No-op on an
// invalid handle or a unit with no mana pool. D5: SetUnitState(UNIT_STATE_MANA).
func (u Unit) SetMana(v float64) {
	if !u.Valid() {
		u.g.reportInvalid("Unit.SetMana")
		return
	}
	r := u.g.w.Abilities.Row(u.id)
	if r < 0 {
		return
	}
	nv := fromFloat(v)
	if nv < 0 {
		nv = 0
	}
	if max := u.g.w.Abilities.MaxMana[r]; nv > max {
		nv = max
	}
	u.g.w.Abilities.Mana[r] = nv
}

// MoveSpeed returns the unit's movement speed in world units per second, or 0
// on an invalid handle / a unit with no movement. The sim stores a per-tick
// rate; this is the per-second value modders set in the data tables.
// JASS: GetUnitMoveSpeed.
func (u Unit) MoveSpeed() float64 {
	if !u.Valid() {
		u.g.reportInvalid("Unit.MoveSpeed")
		return 0
	}
	r := u.g.w.Movements.Row(u.id)
	if r < 0 {
		return 0
	}
	return toFloat(u.g.w.Movements.Speed[r]) * float64(data.TicksPerSecond)
}

// SetMoveSpeed sets the unit's movement speed in world units per second
// (quantized to the per-tick rate). Negative values clamp to 0. No-op on an
// invalid handle or a unit with no movement. JASS: SetUnitMoveSpeed.
func (u Unit) SetMoveSpeed(v float64) {
	if !u.Valid() {
		u.g.reportInvalid("Unit.SetMoveSpeed")
		return
	}
	r := u.g.w.Movements.Row(u.id)
	if r < 0 {
		return
	}
	if v < 0 {
		v = 0
	}
	u.g.w.Movements.Speed[r] = fromFloat(v / float64(data.TicksPerSecond))
}

// TurnSpeed returns the unit's maximum turn rate in radians per second, or 0 on
// an invalid handle / a unit with no movement. The sim stores a per-tick brad
// delta; this surfaces it as the per-second radian rate modders set in the data
// tables. JASS: GetUnitTurnSpeed.
func (u Unit) TurnSpeed() float64 {
	if !u.Valid() {
		u.g.reportInvalid("Unit.TurnSpeed")
		return 0
	}
	r := u.g.w.Movements.Row(u.id)
	if r < 0 {
		return 0
	}
	return angleFromBrad(u.g.w.Movements.TurnRate[r]).Radians() * float64(data.TicksPerSecond)
}

// SetTurnSpeed sets the unit's maximum turn rate in radians per second
// (quantized to the per-tick brad delta). Negative values clamp to 0. No-op on
// an invalid handle or a unit with no movement. JASS: SetUnitTurnSpeed.
func (u Unit) SetTurnSpeed(radPerSec float64) {
	if !u.Valid() {
		u.g.reportInvalid("Unit.SetTurnSpeed")
		return
	}
	r := u.g.w.Movements.Row(u.id)
	if r < 0 {
		return
	}
	if radPerSec < 0 {
		radPerSec = 0
	}
	u.g.w.Movements.TurnRate[r] = angleToBrad(Rad(radPerSec / float64(data.TicksPerSecond)))
}

// AcquireRange returns the unit's auto-acquisition range in world units (the
// radius within which it auto-targets hostiles), or 0 on an invalid handle / a
// unit with no combat row. JASS: GetUnitAcquireRange.
func (u Unit) AcquireRange() float64 {
	if !u.Valid() {
		u.g.reportInvalid("Unit.AcquireRange")
		return 0
	}
	r := u.g.w.Combats.Row(u.id)
	if r < 0 {
		return 0
	}
	return toFloat(u.g.w.Combats.AcquisitionRange[r])
}

// SetAcquireRange sets the unit's auto-acquisition range in world units.
// Negative values clamp to 0 (no acquisition). No-op on an invalid handle or a
// unit with no combat row. JASS: SetUnitAcquireRange.
func (u Unit) SetAcquireRange(v float64) {
	if !u.Valid() {
		u.g.reportInvalid("Unit.SetAcquireRange")
		return
	}
	r := u.g.w.Combats.Row(u.id)
	if r < 0 {
		return
	}
	if v < 0 {
		v = 0
	}
	u.g.w.Combats.AcquisitionRange[r] = fromFloat(v)
}

// DefaultMoveSpeed returns the unit type's base movement speed in world
// units/second — the value the unit spawned with, before any SetMoveSpeed.
// Zero on an invalid handle or an untyped unit. JASS: GetUnitDefaultMoveSpeed.
func (u Unit) DefaultMoveSpeed() float64 {
	if !u.Valid() {
		u.g.reportInvalid("Unit.DefaultMoveSpeed")
		return 0
	}
	return toFloat(u.g.w.UnitDefaultMoveSpeed(u.id)) * float64(data.TicksPerSecond)
}

// DefaultAcquireRange returns the unit type's base auto-acquisition range in
// world units. Zero on an invalid handle or an untyped unit. JASS:
// GetUnitDefaultAcquireRange.
func (u Unit) DefaultAcquireRange() float64 {
	if !u.Valid() {
		u.g.reportInvalid("Unit.DefaultAcquireRange")
		return 0
	}
	return toFloat(u.g.w.UnitDefaultAcquireRange(u.id))
}

// DefaultTurnSpeed returns the unit type's base turn rate in radians/second.
// Zero on an invalid handle or an untyped unit. JASS: GetUnitDefaultTurnSpeed.
func (u Unit) DefaultTurnSpeed() float64 {
	if !u.Valid() {
		u.g.reportInvalid("Unit.DefaultTurnSpeed")
		return 0
	}
	return angleFromBrad(u.g.w.UnitDefaultTurnSpeed(u.id)).Radians() * float64(data.TicksPerSecond)
}

// IsHero reports whether the unit is a hero (carries a hero record with level,
// experience, and attributes). False on an invalid handle. JASS: no direct
// native; the engine's hero test behind GetHero* / IsUnitType(HERO).
func (u Unit) IsHero() bool {
	if !u.Valid() {
		u.g.reportInvalid("Unit.IsHero")
		return false
	}
	return u.g.w.IsHero(u.id)
}

// HeroLevel returns the unit's hero level, or 0 if it is not a hero. JASS:
// GetHeroLevel.
func (u Unit) HeroLevel() int {
	if !u.Valid() {
		u.g.reportInvalid("Unit.HeroLevel")
		return 0
	}
	return int(u.g.w.HeroLevel(u.id))
}

// HeroXP returns the unit's accumulated hero experience, or 0 if it is not a
// hero. JASS: GetHeroXP.
func (u Unit) HeroXP() int {
	if !u.Valid() {
		u.g.reportInvalid("Unit.HeroXP")
		return 0
	}
	return int(u.g.w.HeroXP(u.id))
}

// Strength returns the hero's strength attribute (integer part), or 0 if the
// unit is not a hero. The engine has no separate attribute-bonus layer, so the
// JASS includeBonuses parameter is dropped. JASS: GetHeroStr.
func (u Unit) Strength() int {
	if !u.Valid() {
		u.g.reportInvalid("Unit.Strength")
		return 0
	}
	return int(u.g.w.HeroStr(u.id).Floor())
}

// Agility returns the hero's agility attribute (integer part), or 0 if the unit
// is not a hero. JASS: GetHeroAgi (includeBonuses dropped; see Strength).
func (u Unit) Agility() int {
	if !u.Valid() {
		u.g.reportInvalid("Unit.Agility")
		return 0
	}
	return int(u.g.w.HeroAgi(u.id).Floor())
}

// Intelligence returns the hero's intelligence attribute (integer part), or 0
// if the unit is not a hero. JASS: GetHeroInt (includeBonuses dropped; see
// Strength).
func (u Unit) Intelligence() int {
	if !u.Valid() {
		u.g.reportInvalid("Unit.Intelligence")
		return 0
	}
	return int(u.g.w.HeroInt(u.id).Floor())
}

// AddExperience grants the hero experience, leveling it up at the curve
// boundaries (capped at the curve top). No-op on an invalid handle, a non-hero,
// or a non-positive amount. The JASS showEyeCandy flag is dropped (cosmetic).
// JASS: AddHeroXP.
func (u Unit) AddExperience(xp int) {
	if !u.Valid() {
		u.g.reportInvalid("Unit.AddExperience")
		return
	}
	u.g.w.AddXP(u.id, int64(xp))
}

// ExperienceSuspended reports whether the hero's experience gain is currently
// suspended. False on an invalid handle or a non-hero. JASS: IsSuspendedXP.
func (u Unit) ExperienceSuspended() bool {
	if !u.Valid() {
		u.g.reportInvalid("Unit.ExperienceSuspended")
		return false
	}
	return u.g.w.IsHeroXPSuspended(u.id)
}

// SuspendExperience turns the hero's experience gain off (suspend=true) or back
// on (false). While suspended the hero gains no XP and cannot level. No-op on an
// invalid handle or a non-hero. JASS: SuspendHeroXP.
func (u Unit) SuspendExperience(suspend bool) {
	if !u.Valid() {
		u.g.reportInvalid("Unit.SuspendExperience")
		return
	}
	u.g.w.SuspendHeroXP(u.id, suspend)
}

// SetHeroXP sets the hero's experience to an absolute value, leveling it up as
// needed. Experience never decreases (WC3 heroes do not de-level), and an
// explicit set applies even while experience is suspended. No-op on an invalid
// handle or a non-hero. JASS: SetHeroXP (showEyeCandy dropped — cosmetic).
func (u Unit) SetHeroXP(xp int) {
	if !u.Valid() {
		u.g.reportInvalid("Unit.SetHeroXP")
		return
	}
	u.g.w.SetHeroXP(u.id, int64(xp))
}

// SetHeroLevel raises the hero to the given level, granting the experience
// needed to reach it (clamped to [1, max]; never lowers). No-op on an invalid
// handle or a non-hero. JASS: SetHeroLevel (showEyeCandy dropped — cosmetic).
func (u Unit) SetHeroLevel(level int) {
	if !u.Valid() {
		u.g.reportInvalid("Unit.SetHeroLevel")
		return
	}
	u.g.w.SetHeroLevel(u.id, level)
}

// SkillPoints returns the hero's unspent skill points, or 0 if not a hero.
// JASS: GetHeroSkillPoints.
func (u Unit) SkillPoints() int {
	if !u.Valid() {
		u.g.reportInvalid("Unit.SkillPoints")
		return 0
	}
	return int(u.g.w.HeroSkillPoints(u.id))
}

// ModifySkillPoints adds delta to the hero's unspent skill points (clamped to
// [0, 255]), returning true on success and false on an invalid handle or a
// non-hero. JASS: UnitModifySkillPoints.
func (u Unit) ModifySkillPoints(delta int) bool {
	if !u.Valid() {
		u.g.reportInvalid("Unit.ModifySkillPoints")
		return false
	}
	return u.g.w.ModifySkillPoints(u.id, delta)
}

// SetStrength sets the hero's strength, updating its max life and regen
// accordingly. No-op on an invalid handle or a non-hero. The JASS `permanent`
// parameter is dropped — the engine has no temporary attribute layer (#366).
// JASS: SetHeroStr.
func (u Unit) SetStrength(v int) {
	if !u.Valid() {
		u.g.reportInvalid("Unit.SetStrength")
		return
	}
	u.g.w.SetHeroStr(u.id, fromFloat(float64(v)))
}

// SetAgility sets the hero's agility, refolding its agility-derived stats
// (armor, attack cooldown). No-op on an invalid handle or a non-hero. JASS:
// SetHeroAgi (permanent dropped; see SetStrength).
func (u Unit) SetAgility(v int) {
	if !u.Valid() {
		u.g.reportInvalid("Unit.SetAgility")
		return
	}
	u.g.w.SetHeroAgi(u.id, fromFloat(float64(v)))
}

// SetIntelligence sets the hero's intelligence, updating its max mana and regen.
// No-op on an invalid handle or a non-hero. JASS: SetHeroInt (permanent dropped;
// see SetStrength).
func (u Unit) SetIntelligence(v int) {
	if !u.Valid() {
		u.g.reportInvalid("Unit.SetIntelligence")
		return
	}
	u.g.w.SetHeroInt(u.id, fromFloat(float64(v)))
}

// InventorySize returns the number of item slots the unit can carry — six for
// a unit with an inventory, zero otherwise. Zero on an invalid handle. JASS:
// UnitInventorySize.
func (u Unit) InventorySize() int {
	if !u.Valid() {
		u.g.reportInvalid("Unit.InventorySize")
		return 0
	}
	return int(u.g.w.UnitInventorySize(u.id))
}

// UnitClass is a classification tested by Unit.IsType — the subset of WC3's
// UNIT_TYPE_* constants the engine can derive today (structural + weapon
// classes). Status classes (stunned, poisoned, polymorphed, ethereal, …) and
// race/tag classes (undead, mechanical, summoned, …) are deferred until their
// backing state/tags are modeled.
type UnitClass uint8

const (
	ClassHero      UnitClass = iota // UNIT_TYPE_HERO
	ClassDead                       // UNIT_TYPE_DEAD
	ClassStructure                  // UNIT_TYPE_STRUCTURE (building footprint)
	ClassFlying                     // UNIT_TYPE_FLYING (air pathing)
	ClassGround                     // UNIT_TYPE_GROUND (ground pathing)
	ClassMelee                      // UNIT_TYPE_MELEE_ATTACKER (instant weapon)
	ClassRanged                     // UNIT_TYPE_RANGED_ATTACKER (projectile weapon)
	ClassAttacksGround              // UNIT_TYPE_ATTACKS_GROUND
	ClassAttacksFlying              // UNIT_TYPE_ATTACKS_FLYING
)

// IsType reports whether the unit belongs to the given structural class.
// False on an invalid handle or an unrecognized class. JASS: IsUnitType (the
// structural UNIT_TYPE_* subset; see UnitClass).
func (u Unit) IsType(class UnitClass) bool {
	if !u.Valid() {
		u.g.reportInvalid("Unit.IsType")
		return false
	}
	switch class {
	case ClassHero:
		return u.g.w.IsHero(u.id)
	case ClassDead:
		return !u.Alive()
	case ClassStructure:
		return u.g.w.UnitIsStructure(u.id)
	case ClassFlying:
		return u.g.w.UnitIsFlying(u.id)
	case ClassGround:
		return u.g.w.UnitIsGround(u.id)
	case ClassMelee:
		return u.g.w.UnitIsMelee(u.id)
	case ClassRanged:
		return u.g.w.UnitIsRanged(u.id)
	case ClassAttacksGround:
		return u.g.w.UnitAttacksGround(u.id)
	case ClassAttacksFlying:
		return u.g.w.UnitAttacksFlying(u.id)
	default:
		return false
	}
}

// Race is a unit's faction race (GetUnitRace). Values mirror the WC3 race
// constants so the underlying integer round-trips. RaceNone is the unset
// default for an untyped or unconfigured unit.
type Race uint8

const (
	RaceNone     Race = 0 // unset / untyped
	RaceHuman    Race = 1 // RACE_HUMAN
	RaceOrc      Race = 2 // RACE_ORC
	RaceUndead   Race = 3 // RACE_UNDEAD
	RaceNightElf Race = 4 // RACE_NIGHTELF
	RaceDemon    Race = 5 // RACE_DEMON
	RaceOther    Race = 7 // RACE_OTHER
)

// Race returns the unit type's race, or RaceNone on an invalid handle, an
// untyped unit, or a type with no race configured. JASS: GetUnitRace.
func (u Unit) Race() Race {
	if !u.Valid() {
		u.g.reportInvalid("Unit.Race")
		return RaceNone
	}
	return Race(u.g.w.UnitRace(u.id))
}

// IsRace reports whether the unit's race equals r. False on an invalid handle.
// Note a unit with no configured race (RaceNone) only matches IsRace(RaceNone).
// JASS: IsUnitRace.
func (u Unit) IsRace(r Race) bool {
	if !u.Valid() {
		u.g.reportInvalid("Unit.IsRace")
		return false
	}
	return Race(u.g.w.UnitRace(u.id)) == r
}

// InRange reports whether this unit is within distance world units of other
// (center-to-center, inclusive). False on an invalid handle, an invalid other,
// or a negative distance. JASS: IsUnitInRange.
func (u Unit) InRange(other Unit, distance float64) bool {
	if !u.Valid() {
		u.g.reportInvalid("Unit.InRange")
		return false
	}
	if !other.Valid() || other.g != u.g {
		return false
	}
	return u.g.w.UnitsInRange(u.id, other.id, fromFloat(distance))
}

// InRangeOf reports whether this unit is within distance world units of point
// (center-to-center, inclusive). False on an invalid handle or a negative
// distance. JASS: IsUnitInRangeXY, IsUnitInRangeLoc (D3 → one Vec2 overload).
func (u Unit) InRangeOf(point Vec2, distance float64) bool {
	if !u.Valid() {
		u.g.reportInvalid("Unit.InRangeOf")
		return false
	}
	return u.g.w.UnitInRangeOfPoint(u.id, vec(point), fromFloat(distance))
}

// RallyPoint returns the building's rally point — where newly produced units
// gather. For a point rally it is the set point; for a unit rally it is the
// rally target's current position. The zero Vec2 when the unit has no rally,
// no produce capability, or an invalid handle. JASS: GetUnitRallyPoint.
func (u Unit) RallyPoint() Vec2 {
	if !u.Valid() {
		u.g.reportInvalid("Unit.RallyPoint")
		return Vec2{}
	}
	_, pt, _ := u.g.w.UnitRally(u.id)
	return Vec2{X: toFloat(pt.X), Y: toFloat(pt.Y)}
}

// RallyUnit returns the unit this building is rallied to, or the zero Unit
// when the rally is a point (not a unit), absent, or the handle is invalid.
// JASS: GetUnitRallyUnit.
func (u Unit) RallyUnit() Unit {
	if !u.Valid() {
		u.g.reportInvalid("Unit.RallyUnit")
		return Unit{}
	}
	ent, ok := u.g.w.UnitRallyUnit(u.id)
	if !ok {
		return Unit{}
	}
	return Unit{id: ent, g: u.g}
}

// Name returns the unit's display name. Currently this is the unit type's
// proper name (per-instance rename via BlzSetUnitName is deferred). Empty string
// on an invalid handle or an unnamed/untyped unit. JASS: GetUnitName.
func (u Unit) Name() string {
	if !u.Valid() {
		u.g.reportInvalid("Unit.Name")
		return ""
	}
	return u.g.w.UnitName(u.id)
}

// SetName sets a per-instance display name override, shadowing the unit type's
// proper name (subsequent Name calls return it). No-op on an invalid handle.
// JASS: BlzSetUnitName.
func (u Unit) SetName(name string) {
	if !u.Valid() {
		u.g.reportInvalid("Unit.SetName")
		return
	}
	u.g.w.SetUnitName(u.id, name)
}

// FoodUsed returns the food (upkeep) the unit's type consumes from its owner's
// food total. Zero on an invalid handle or an untyped unit. JASS:
// GetUnitFoodUsed.
func (u Unit) FoodUsed() int {
	if !u.Valid() {
		u.g.reportInvalid("Unit.FoodUsed")
		return 0
	}
	return int(u.g.w.UnitFoodUsed(u.id))
}

// FoodMade returns the food the unit's type provides (raising its owner's food
// cap, e.g. a farm). Zero on an invalid handle or an untyped unit. JASS:
// GetUnitFoodMade.
func (u Unit) FoodMade() int {
	if !u.Valid() {
		u.g.reportInvalid("Unit.FoodMade")
		return 0
	}
	return int(u.g.w.UnitFoodMade(u.id))
}

// IsHidden reports whether the unit is suppressed from rendering and
// selection (it still exists in the simulation). False on an invalid handle.
// JASS: IsUnitHidden.
func (u Unit) IsHidden() bool {
	if !u.Valid() {
		u.g.reportInvalid("Unit.IsHidden")
		return false
	}
	return u.g.w.IsUnitHidden(u.id)
}

// Show reveals (show=true) or hides (show=false) the unit. Hiding suppresses it
// from rendering and selection without removing it from the simulation. No-op
// on an invalid handle. JASS: ShowUnit.
func (u Unit) Show(show bool) {
	if !u.Valid() {
		u.g.reportInvalid("Unit.Show")
		return
	}
	u.g.w.ShowUnit(u.id, show)
}

// Paused reports whether the unit is frozen by SetPaused. A paused unit still
// exists (life/position/owner persist) but does not advance orders, move,
// acquire targets, or attack. False on an invalid handle. JASS: IsUnitPaused
// (collapses IsUnitPausedBJ, D1).
func (u Unit) Paused() bool {
	if !u.Valid() {
		u.g.reportInvalid("Unit.Paused")
		return false
	}
	return u.g.w.IsUnitPaused(u.id)
}

// SetPaused freezes (paused=true) or resumes (paused=false) the unit. While
// paused the orders, movement, acquisition and attack systems skip it, so it
// holds its current state until resumed. No-op on an invalid handle. The bit is
// deterministic state and persists across save/load. JASS: PauseUnit
// (collapses PauseUnitBJ, D1).
func (u Unit) SetPaused(paused bool) {
	if !u.Valid() {
		u.g.reportInvalid("Unit.SetPaused")
		return
	}
	u.g.w.PauseUnit(u.id, paused)
}

// Kill kills the unit (marked this tick; resolved in the sim step, firing the
// death event). A unit already dead or invalid is a no-op. JASS: KillUnit,
// KillUnitBJ (D1 passthrough collapses here).
func (u Unit) Kill() {
	if !u.Valid() {
		u.g.reportInvalid("Unit.Kill")
		return
	}
	u.g.w.KillUnit(u.id)
}

// Remove deletes the unit and all its component rows immediately, with no
// death event — the unit simply ceases to exist (corpse, selection, and
// orders all released). No-op on an invalid handle. JASS: RemoveUnit,
// RemoveUnitBJ.
func (u Unit) Remove() {
	if !u.Valid() {
		u.g.reportInvalid("Unit.Remove")
		return
	}
	u.g.w.DestroyUnit(u.id)
}

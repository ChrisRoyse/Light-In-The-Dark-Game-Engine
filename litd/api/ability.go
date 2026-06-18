package litd

// Ability noun verbs (public-api-design.md §2 row 8): public refs and
// fields stay API-owned values, while methods translate into sim
// ability slots, fixed-point override rows, and runtime definitions.

import (
	"math"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
)

// AbilityRef names an ability definition registered in the game. Zero
// is the empty/null ref and is rejected by Unit.AddAbility.
type AbilityRef uint16

// IsZero reports whether this is the empty ability ref.
func (r AbilityRef) IsZero() bool { return r == 0 }

// AbilityField names one public per-instance ability field. It mirrors
// the sim field matrix without exposing sim types in API signatures.
type AbilityField uint8

// The AbilityField values name the public per-instance ability fields, in the
// order the Ability field accessors expect.
const (
	AbilityFieldCooldown AbilityField = iota
	AbilityFieldManaCost
	AbilityFieldRange
	AbilityFieldDamage
	AbilityFieldDuration
	AbilityFieldAreaOfEffect
	AbilityFieldCastTime
)

// AbilityDef is the setup-time runtime ability registration payload.
// Durations are public float64 seconds and are quantized to sim ticks
// on registration. CastRange is in world units; zero means self/no
// target for abilities without an effect composition.
type AbilityDef struct {
	// ID is the ability's stable string identifier.
	ID string
	// Name is the display name.
	Name string

	// ManaCost is the mana consumed per cast.
	ManaCost int

	// Cooldown is the recharge time in seconds before the ability can recast.
	Cooldown float64
	// CastPoint is the pre-cast wind-up delay in seconds before the effect fires.
	CastPoint float64
	// Backswing is the post-cast recovery delay in seconds.
	Backswing float64
	// Channel is the channelling duration in seconds (0 = instant, non-channelled).
	Channel float64
	// CastRange is the maximum target distance in world units (0 = self/no target).
	CastRange float64
}

// RegisterAbility appends a runtime ability definition and returns its
// stable ref, or zero on validation failure. Debug mode reports invalid
// definitions through OnInvalidHandle.
func (g *Game) RegisterAbility(def AbilityDef) AbilityRef {
	if g == nil || g.w == nil {
		return 0
	}
	if def.ManaCost < 0 || def.ManaCost > int(^uint32(0)>>1) {
		g.reportInvalid("Game.RegisterAbility (mana cost out of range)")
		return 0
	}
	cooldown, ok := abilitySecondsToTicks(g, "Cooldown", def.Cooldown)
	if !ok {
		return 0
	}
	castPoint, ok := abilitySecondsToTicks(g, "CastPoint", def.CastPoint)
	if !ok {
		return 0
	}
	backswing, ok := abilitySecondsToTicks(g, "Backswing", def.Backswing)
	if !ok {
		return 0
	}
	channel, ok := abilitySecondsToTicks(g, "Channel", def.Channel)
	if !ok {
		return 0
	}
	castRange, ok := abilityFixedFromFloat(def.CastRange)
	if !ok {
		g.reportInvalid("Game.RegisterAbility (cast range out of range)")
		return 0
	}
	ref, registered := g.w.RegisterAbilityDef(data.Ability{
		ID:             def.ID,
		Name:           def.Name,
		ManaCost:       int32(def.ManaCost),
		CooldownTicks:  cooldown,
		CastPointTicks: castPoint,
		BackswingTicks: backswing,
		ChannelTicks:   channel,
		CastRange:      castRange,
	})
	if !registered {
		g.reportInvalid("Game.RegisterAbility (invalid def)")
		return 0
	}
	return AbilityRef(ref)
}

func abilitySecondsToTicks(g *Game, field string, seconds float64) (uint16, bool) {
	ticks, err := data.SecondsToTicks(seconds)
	if err != nil {
		g.reportInvalid("Game.RegisterAbility (" + field + " out of range)")
		return 0, false
	}
	return ticks, true
}

// AddAbility equips ref in the first available unit slot and returns
// the per-unit Ability handle. If the ref is already equipped, the
// existing handle is returned.
// JASS: UnitAddAbility, UnitAddAbilityBJ
func (u Unit) AddAbility(ref AbilityRef) Ability {
	if !u.Valid() {
		u.g.reportInvalid("Unit.AddAbility")
		return Ability{}
	}
	r := uint16(ref)
	if r == 0 || int(r) > u.g.w.AbilityDefCount() {
		u.g.reportInvalid("Unit.AddAbility (unknown ability ref)")
		return Ability{}
	}
	ar := u.g.w.Abilities.Row(u.id)
	if ar != -1 {
		for slot := 0; slot < sim.AbilitySlots; slot++ {
			if u.g.w.Abilities.AbilityID[ar][slot] == r {
				return Ability{owner: u.id, ref: uint32(r), g: u.g}
			}
		}
	}
	slot := -1
	if ar == -1 {
		if !u.g.w.Abilities.Add(u.g.w.Ents, u.id) {
			u.g.reportInvalid("Unit.AddAbility (ability row exhausted)")
			return Ability{}
		}
		ar = u.g.w.Abilities.Row(u.id)
	}
	for i := 0; i < sim.AbilitySlots; i++ {
		if u.g.w.Abilities.AbilityID[ar][i] == 0 {
			slot = i
			break
		}
	}
	if slot == -1 {
		u.g.reportInvalid("Unit.AddAbility (slots full)")
		return Ability{}
	}
	if !u.g.w.SetAbilityRef(u.id, slot, r) {
		u.g.reportInvalid("Unit.AddAbility (set failed)")
		return Ability{}
	}
	ar = u.g.w.Abilities.Row(u.id)
	if ar != -1 && u.g.w.Abilities.Level[ar][slot] == 0 {
		u.g.w.Abilities.Level[ar][slot] = 1
	}
	return Ability{owner: u.id, ref: uint32(r), g: u.g}
}

// RemoveAbility removes ref from the unit, clearing all per-instance
// overrides for the slot. It returns false on invalid handles, unknown
// refs, or refs not equipped by the unit.
// JASS: UnitRemoveAbility, UnitRemoveAbilityBJ
func (u Unit) RemoveAbility(ref AbilityRef) bool {
	if !u.Valid() {
		u.g.reportInvalid("Unit.RemoveAbility")
		return false
	}
	r := uint16(ref)
	if r == 0 {
		u.g.reportInvalid("Unit.RemoveAbility (zero ability ref)")
		return false
	}
	ar := u.g.w.Abilities.Row(u.id)
	if ar == -1 {
		u.g.reportInvalid("Unit.RemoveAbility (ability not equipped)")
		return false
	}
	for slot := 0; slot < sim.AbilitySlots; slot++ {
		if u.g.w.Abilities.AbilityID[ar][slot] == r {
			if !u.g.w.RemoveAbility(u.id, slot) {
				u.g.reportInvalid("Unit.RemoveAbility (remove failed)")
				return false
			}
			return true
		}
	}
	u.g.reportInvalid("Unit.RemoveAbility (ability not equipped)")
	return false
}

// Level returns the current unit ability level, or zero on an invalid
// handle.
// JASS: GetUnitAbilityLevel, GetUnitAbilityLevelSwapped
func (a Ability) Level() int {
	ar, slot, ok := a.g.abilitySlot(a.owner, a.ref)
	if !ok {
		a.g.reportInvalid("Ability.Level")
		return 0
	}
	return int(a.g.w.Abilities.Level[ar][slot])
}

// SetLevel updates the unit ability level. Levels outside uint8 range
// fail closed and leave the store unchanged.
// JASS: DecUnitAbilityLevelSwapped, SetUnitAbilityLevel, SetUnitAbilityLevelSwapped
func (a Ability) SetLevel(level int) {
	ar, slot, ok := a.g.abilitySlot(a.owner, a.ref)
	if !ok {
		a.g.reportInvalid("Ability.SetLevel")
		return
	}
	if level < 0 || level > 255 {
		a.g.reportInvalid("Ability.SetLevel (level out of range)")
		return
	}
	a.g.w.Abilities.Level[ar][slot] = uint8(level)
}

// IncLevel raises the ability level by one and returns the new level,
// or 0 on an invalid handle. The level saturates at 255 (the store's
// uint8 ceiling) — it never wraps. JASS: IncUnitAbilityLevel.
// JASS: IncUnitAbilityLevel, IncUnitAbilityLevelSwapped
func (a Ability) IncLevel() int {
	ar, slot, ok := a.g.abilitySlot(a.owner, a.ref)
	if !ok {
		a.g.reportInvalid("Ability.IncLevel")
		return 0
	}
	lvl := a.g.w.Abilities.Level[ar][slot]
	if lvl < 255 {
		lvl++
		a.g.w.Abilities.Level[ar][slot] = lvl
	}
	return int(lvl)
}

// DecLevel lowers the ability level by one and returns the new level,
// or 0 on an invalid handle. It floors at 1: an equipped ability is
// always at least level 1, so removal is explicit (RemoveAbility) and
// never an accidental side effect of decrementing. (WC3's native drops
// the ability at level 0; this engine keeps removal an explicit verb so
// the Ability handle stays valid — documented divergence.) JASS:
// DecUnitAbilityLevel.
// JASS: DecUnitAbilityLevel
func (a Ability) DecLevel() int {
	ar, slot, ok := a.g.abilitySlot(a.owner, a.ref)
	if !ok {
		a.g.reportInvalid("Ability.DecLevel")
		return 0
	}
	lvl := a.g.w.Abilities.Level[ar][slot]
	if lvl > 1 {
		lvl--
		a.g.w.Abilities.Level[ar][slot] = lvl
	}
	return int(lvl)
}

// Field resolves an ability field override or its definition default,
// returning zero on invalid handles or unknown fields.
// JASS: BlzGetAbilityBooleanField, BlzGetAbilityCooldown, BlzGetAbilityIntegerField, BlzGetAbilityManaCost, BlzGetAbilityRealField, BlzGetAbilityStringField, BlzGetUnitAbilityCooldown, BlzGetUnitAbilityCooldownRemaining, BlzGetUnitAbilityManaCost
func (a Ability) Field(field AbilityField) float64 {
	_, slot, ok := a.g.abilitySlot(a.owner, a.ref)
	if !ok {
		a.g.reportInvalid("Ability.Field")
		return 0
	}
	sf, ok := toSimAbilityField(field)
	if !ok {
		a.g.reportInvalid("Ability.Field (unknown field)")
		return 0
	}
	v, ok := a.g.w.ResolveAbilityField(a.owner, slot, sf)
	if !ok {
		a.g.reportInvalid("Ability.Field (resolve failed)")
		return 0
	}
	return toFloat(v)
}

// SetField writes a per-instance override row for this ability.
// JASS: BlzSetAbilityBooleanField, BlzSetAbilityBooleanFieldBJ, BlzSetAbilityIntegerField, BlzSetAbilityIntegerFieldBJ, BlzSetAbilityRealField, BlzSetAbilityRealFieldBJ, BlzSetAbilityStringField, BlzSetAbilityStringFieldBJ
func (a Ability) SetField(field AbilityField, value float64) {
	_, slot, ok := a.g.abilitySlot(a.owner, a.ref)
	if !ok {
		a.g.reportInvalid("Ability.SetField")
		return
	}
	sf, ok := toSimAbilityField(field)
	if !ok {
		a.g.reportInvalid("Ability.SetField (unknown field)")
		return
	}
	fv, ok := abilityFixedFromFloat(value)
	if !ok {
		a.g.reportInvalid("Ability.SetField (value out of range)")
		return
	}
	if !a.g.w.SetAbilityField(a.owner, slot, sf, fv) {
		a.g.reportInvalid("Ability.SetField (set failed)")
	}
}

// Cooldown is the typed shortcut for Field(AbilityFieldCooldown).
func (a Ability) Cooldown() float64 { return a.Field(AbilityFieldCooldown) }

// ManaCost is the typed shortcut for Field(AbilityFieldManaCost).
func (a Ability) ManaCost() float64 { return a.Field(AbilityFieldManaCost) }

func (g *Game) abilitySlot(owner sim.EntityID, ref uint32) (int32, int, bool) {
	if g == nil || g.w == nil || ref == 0 || ref > uint32(^uint16(0)) || !g.w.Ents.Alive(owner) {
		return -1, -1, false
	}
	ar := g.w.Abilities.Row(owner)
	if ar == -1 {
		return -1, -1, false
	}
	r := uint16(ref)
	for slot := 0; slot < sim.AbilitySlots; slot++ {
		if g.w.Abilities.AbilityID[ar][slot] == r {
			return ar, slot, true
		}
	}
	return -1, -1, false
}

func toSimAbilityField(field AbilityField) (sim.AbilityField, bool) {
	switch field {
	case AbilityFieldCooldown:
		return sim.AbilityFieldCooldown, true
	case AbilityFieldManaCost:
		return sim.AbilityFieldManaCost, true
	case AbilityFieldRange:
		return sim.AbilityFieldRange, true
	case AbilityFieldDamage:
		return sim.AbilityFieldDamage, true
	case AbilityFieldDuration:
		return sim.AbilityFieldDuration, true
	case AbilityFieldAreaOfEffect:
		return sim.AbilityFieldAreaOfEffect, true
	case AbilityFieldCastTime:
		return sim.AbilityFieldCastTime, true
	default:
		return 0, false
	}
}

func abilityFixedFromFloat(v float64) (fixed.F64, bool) {
	if math.IsNaN(v) || math.IsInf(v, 0) {
		return 0, false
	}
	if v > float64(fixed.MaxF64)/f64Scale || v < float64(fixed.MinF64)/f64Scale {
		return 0, false
	}
	return fromFloat(v), true
}

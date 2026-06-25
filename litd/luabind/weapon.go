package luabind

// Live weapon override bindings (#476): a script re-arms a unit's weapon at
// runtime. Hand-written because Unit.SetWeaponField/etc. + the WeaponField enum
// are LitD new-capabilities (no JASS mapping). The WEAPON_* integer constants
// name the overridable fields; values are plain integers (an attack-type id, a
// die count, ticks) except WEAPON_RANGE/WEAPON_PROJECTILE_SPEED which are raw
// fixed-point bits.

import (
	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	lua "github.com/yuin/gopher-lua"
)

func registerScriptWeapon(L *lua.LState, g *api.Game) {
	// Field-id constants.
	for name, field := range map[string]api.WeaponField{
		"WEAPON_ATTACK_TYPE":      api.WeaponAttackType,
		"WEAPON_DAMAGE_BASE":      api.WeaponDamageBase,
		"WEAPON_DICE":             api.WeaponDice,
		"WEAPON_SIDES":            api.WeaponSides,
		"WEAPON_COOLDOWN":         api.WeaponCooldown,
		"WEAPON_RANGE":            api.WeaponRange,
		"WEAPON_DAMAGE_POINT":     api.WeaponDamagePoint,
		"WEAPON_BACKSWING":        api.WeaponBackswing,
		"WEAPON_PROJECTILE_SPEED": api.WeaponProjectileSpeed,
	} {
		L.SetGlobal(name, lua.LNumber(field))
	}

	// Unit_SetWeaponField(unit, slot, field, value) -> bool
	L.SetGlobal("Unit_SetWeaponField", L.NewFunction(func(L *lua.LState) int {
		ok := argUnit(L, 1).SetWeaponField(L.CheckInt(2), api.WeaponField(L.CheckInt(3)), L.CheckInt(4))
		L.Push(lua.LBool(ok))
		return 1
	}))
	// Unit_ClearWeaponField(unit, slot, field) -> bool
	L.SetGlobal("Unit_ClearWeaponField", L.NewFunction(func(L *lua.LState) int {
		ok := argUnit(L, 1).ClearWeaponField(L.CheckInt(2), api.WeaponField(L.CheckInt(3)))
		L.Push(lua.LBool(ok))
		return 1
	}))
	// Unit_ClearWeapon(unit, slot) -> int (count cleared)
	L.SetGlobal("Unit_ClearWeapon", L.NewFunction(func(L *lua.LState) int {
		L.Push(lua.LNumber(argUnit(L, 1).ClearWeapon(L.CheckInt(2))))
		return 1
	}))
	// Unit_WeaponField(unit, slot, field) -> value, ok
	L.SetGlobal("Unit_WeaponField", L.NewFunction(func(L *lua.LState) int {
		v, ok := argUnit(L, 1).WeaponField(L.CheckInt(2), api.WeaponField(L.CheckInt(3)))
		L.Push(lua.LNumber(v))
		L.Push(lua.LBool(ok))
		return 2
	}))

	// Unit_CastAbility(caster, ability, target) -> bool — the public cast-ability
	// order (#479): runs through the cast machine, firing a trigger-bound ability
	// (#478) at its EFFECT edge.
	L.SetGlobal("Unit_CastAbility", L.NewFunction(func(L *lua.LState) int {
		L.Push(lua.LBool(argUnit(L, 1).Cast(argAbility(L, 2), argUnit(L, 3))))
		return 1
	}))

	// Unit_CastAbilityRef(caster, ref, target) -> bool — save-safe cast-by-ref
	// (#667, from the #663 footgun). Re-derives the ability handle from its REF
	// internally via the idempotent Unit.AddAbility, so a script never has to hold
	// an Ability handle across a tick — an Ability upvalue is not marshalable and
	// kills savegame.Write (#663). A closure that captures only the ref (an int from
	// Game_AbilityRef) plus the unit/target handles is save-safe and casts through
	// the exact same cast machine as Unit_CastAbility.
	L.SetGlobal("Unit_CastAbilityRef", L.NewFunction(func(L *lua.LState) int {
		caster := argUnit(L, 1)
		ability := caster.AddAbility(argAbilityRef(L, 2))
		L.Push(lua.LBool(caster.Cast(ability, argUnit(L, 3))))
		return 1
	}))
}

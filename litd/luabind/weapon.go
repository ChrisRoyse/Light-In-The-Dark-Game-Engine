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
}

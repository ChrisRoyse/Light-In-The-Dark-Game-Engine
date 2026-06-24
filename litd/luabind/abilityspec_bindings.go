package luabind

// abilityspec_bindings.go — hand-written Lua binding for the composable-ability
// authoring surface (PRD2 06, #599). RegisterAbilitySpec{...} mirrors the Go
// Game.RegisterAbilitySpec exactly: same field names, same op vocabulary, same
// fail-closed validation — so an ability authored in Lua compiles to the same
// spec and casts identically to one authored in Go (R-ABL-5). Returns the
// ability ref (number, 0 on error) and, on error, the validator message.

import (
	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	lua "github.com/yuin/gopher-lua"
)

// bindRegisterAbilitySpec: RegisterAbilitySpec(table) -> ref, errString.
func (b gameBinder) bindRegisterAbilitySpec(L *lua.LState) int {
	t := L.CheckTable(1)
	def := abilitySpecFromTable(t)
	ref, err := b.g.RegisterAbilitySpec(def)
	L.Push(lua.LNumber(ref))
	if err != nil {
		L.Push(lua.LString(err.Error()))
		return 2
	}
	return 1
}

func abilitySpecFromTable(t *lua.LTable) api.AbilitySpecDef {
	str := func(k string) string {
		if s, ok := t.RawGetString(k).(lua.LString); ok {
			return string(s)
		}
		return ""
	}
	num := func(k string) float64 {
		if n, ok := t.RawGetString(k).(lua.LNumber); ok {
			return float64(n)
		}
		return 0
	}
	def := api.AbilitySpecDef{
		ID: str("id"), Name: str("name"), CastType: str("cast_type"), Indicator: str("indicator"),
		CastRange: num("cast_range"), ManaCost: int(num("mana_cost")), Cooldown: num("cooldown"),
		Precast: num("precast"), CastPoint: num("cast_point"), Backswing: num("backswing"),
	}
	if ops, ok := t.RawGetString("on_cast").(*lua.LTable); ok {
		def.OnCast = abilityOpsFromTable(ops)
	}
	return def
}

func abilityOpsFromTable(arr *lua.LTable) []api.AbilityOpDef {
	n := arr.Len()
	if n == 0 {
		return nil
	}
	out := make([]api.AbilityOpDef, 0, n)
	for i := 1; i <= n; i++ {
		ot, ok := arr.RawGetInt(i).(*lua.LTable)
		if !ok {
			continue
		}
		out = append(out, abilityOpFromTable(ot))
	}
	return out
}

func abilityOpFromTable(t *lua.LTable) api.AbilityOpDef {
	str := func(k string) string {
		if s, ok := t.RawGetString(k).(lua.LString); ok {
			return string(s)
		}
		return ""
	}
	num := func(k string) float64 {
		if n, ok := t.RawGetString(k).(lua.LNumber); ok {
			return float64(n)
		}
		return 0
	}
	op := api.AbilityOpDef{
		Op: str("op"), Mover: str("mover"), Effects: str("effects"), Event: str("event"), Key: str("key"),
		Cont: uint16(num("cont")), Speed: num("speed"), Range: num("range"), Radius: num("radius"),
		Amount: num("amount"), Arg: int64(num("arg")), Count: int(num("count")),
		HitMask: uint16(num("hitmask")), Pierce: int(num("pierce")),
	}
	if kids, ok := t.RawGetString("children").(*lua.LTable); ok {
		op.Children = abilityOpsFromTable(kids)
	}
	return op
}

// registerAbilitySpecs installs the composable-ability authoring verb.
func registerAbilitySpecs(L *lua.LState, b gameBinder) {
	L.SetGlobal("RegisterAbilitySpec", L.NewFunction(b.bindRegisterAbilitySpec))
}

package luabind

import (
	"testing"

	lua "github.com/yuin/gopher-lua"
)

// #619 — Lua custom-event surface end-to-end. SoT = a Go mark() the Lua
// handler calls, proving register→subscribe→emit→dispatch works from Lua.
func TestLuaCustomEventEndToEnd(t *testing.T) {
	g, L, _ := newScriptGame(t)
	var gotArg int64 = -1
	var fires int
	L.SetGlobal("mark", L.NewFunction(func(L *lua.LState) int {
		gotArg = int64(L.CheckNumber(1))
		fires++
		return 0
	}))

	if err := L.DoString(`
		k = RegisterEvent("wave")
		again = RegisterEvent("wave")   -- idempotent
		OnEvent(k, function(e) mark(4) end)
		Emit(k, nil, nil, 2 + 2)
	`); err != nil {
		t.Fatalf("lua: %v", err)
	}
	if k := int(lua.LVAsNumber(L.GetGlobal("k"))); k != int(lua.LVAsNumber(L.GetGlobal("again"))) || k == 0 {
		t.Fatalf("RegisterEvent not idempotent/zero: %d", k)
	}
	g.Advance(1) // dispatch tick
	if fires != 1 || gotArg != 4 {
		t.Fatalf("Lua custom-event handler fires=%d arg=%d, want 1 and 4", fires, gotArg)
	}
}

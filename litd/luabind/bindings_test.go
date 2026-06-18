package luabind

// #267 binding-layer FSV. The generated Lua bindings (bindings_gen.go +
// bindings_dispatch_gen.go) are meant to be a faithful skin over the Go api:
// the same scenario driven from Lua must leave the sim in the SAME state as the
// same scenario driven by direct Go calls. SoT = Game.StateHash() compared
// across the two drivers (NOT the absence of a script error).
//
// Scope: this locks edge 4 (Go-vs-Lua identical hash) and edge 3 (Vec2
// round-trip) from the issue, using only verbs known to be bound. The remaining
// #267 work (full TestLuaConformance subset, 0-alloc pooled userdata) stays open
// — pooled userdata is gated on LState pooling (#265).

import (
	"fmt"
	"testing"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	lua "github.com/yuin/gopher-lua"
)

// TestNumericEnumArgBindingFSV — #267 generator coverage: the numeric-enum arg
// types (UnitClass here) now marshal from a Lua number, so Unit_IsType binds.
// SoT = parity between the Go call u.IsType(class) and the bound Lua call
// Unit_IsType(u, int(class)) across several classes — a wrong marshal would flip
// a result.
func TestNumericEnumArgBindingFSV(t *testing.T) {
	g := loaderGame(t, 1)
	L := boundState(t, g)
	defer L.Close()
	u := g.CreateUnit(g.Player(0), g.UnitType("hfoo"), api.Vec2{X: 64, Y: 64}, api.Deg(0))
	ud := L.NewUserData()
	ud.Value = u
	L.SetGlobal("u", ud)
	for _, cl := range []struct {
		name string
		c    api.UnitClass
	}{{"hero", api.ClassHero}, {"structure", api.ClassStructure}, {"ground", api.ClassGround}} {
		goRes := u.IsType(cl.c)
		if err := L.DoString(fmt.Sprintf("res = Unit_IsType(u, %d)", int(cl.c))); err != nil {
			t.Fatalf("Unit_IsType(%s) must be bound + callable (#267): %v", cl.name, err)
		}
		luaRes := lua.LVAsBool(L.GetGlobal("res"))
		t.Logf("FSV #267 enum-arg: IsType(%-9s) Go=%v Lua=%v", cl.name, goRes, luaRes)
		if goRes != luaRes {
			t.Fatalf("Unit_IsType(%s) Go=%v != Lua=%v — UnitClass marshalled wrong", cl.name, goRes, luaRes)
		}
	}
}

// TestWidgetArgBindingFSV — #267: the combat verb Unit.Damage binds now that
// Widget (a Unit/Destructable handle-interface) marshals. SoT = the target's
// Life dropping after a Lua-driven Unit_Damage.
func TestWidgetArgBindingFSV(t *testing.T) {
	g := loaderGame(t, 1)
	L := boundState(t, g)
	defer L.Close()
	atk := g.CreateUnit(g.Player(0), g.UnitType("hfoo"), api.Vec2{X: 0, Y: 0}, api.Deg(0))
	tgtGo := g.CreateUnit(g.Player(1), g.UnitType("hfoo"), api.Vec2{X: 32, Y: 0}, api.Deg(0))
	tgtLua := g.CreateUnit(g.Player(1), g.UnitType("hfoo"), api.Vec2{X: 64, Y: 0}, api.Deg(0))
	a := L.NewUserData()
	a.Value = atk
	L.SetGlobal("atk", a)
	d := L.NewUserData()
	d.Value = tgtLua
	L.SetGlobal("tgt", d)

	// Same damage, once via Go and once via the Lua Widget-arg binding. SoT =
	// parity of the two targets' Life after the queued damage resolves (combat
	// semantics are identical, so a faithful binding produces an equal result).
	goQueued := atk.Damage(tgtGo, 30)
	if err := L.DoString(`ok = Unit_Damage(atk, tgt, 30)`); err != nil {
		t.Fatalf("Unit_Damage must bind via the Widget arg (#267): %v", err)
	}
	luaQueued := lua.LVAsBool(L.GetGlobal("ok"))
	g.Advance(1) // resolve the queued DamagePackets
	lifeGo, lifeLua := tgtGo.Life(), tgtLua.Life()
	t.Logf("FSV #267 Widget-arg: queued Go=%v Lua=%v; resolved Life Go=%.1f Lua=%.1f", goQueued, luaQueued, lifeGo, lifeLua)
	if !luaQueued {
		t.Fatal("Unit_Damage via Lua returned false — the Widget userdata was not resolved to the target")
	}
	if goQueued != luaQueued || lifeGo != lifeLua {
		t.Fatalf("Go vs Lua Unit_Damage diverged: queued %v/%v, Life %.1f/%.1f — Widget marshalled wrong", goQueued, luaQueued, lifeGo, lifeLua)
	}
}

// TestVariadicNoOptionsBindingFSV — #267 ADR #402: variadic-options verbs bind in
// their no-options form, and passing options from Lua fails closed (raises),
// never silently drops them. SoT = (a) Go↔Lua parity for the no-options call,
// (b) a non-nil error when an extra (option) arg is passed.
func TestVariadicNoOptionsBindingFSV(t *testing.T) {
	g := loaderGame(t, 1)
	L := boundState(t, g)
	defer L.Close()
	u := g.CreateUnit(g.Player(0), g.UnitType("hfoo"), api.Vec2{X: 64, Y: 64}, api.Deg(0))
	ud := L.NewUserData()
	ud.Value = u
	L.SetGlobal("u", ud)

	// No-options form binds and matches the Go call (no item in slot 0 → false).
	goRes := u.UseItem(0)
	if err := L.DoString(`r = Unit_UseItem(u, 0)`); err != nil {
		t.Fatalf("Unit_UseItem no-options form must be callable (#267): %v", err)
	}
	luaRes := lua.LVAsBool(L.GetGlobal("r"))
	t.Logf("FSV #267 variadic no-opts: Unit_UseItem(u,0) Go=%v Lua=%v", goRes, luaRes)
	if goRes != luaRes {
		t.Fatalf("Unit_UseItem parity broken: Go=%v Lua=%v", goRes, luaRes)
	}

	// Fail-closed: an extra (option) arg must raise, not silently drop (§1.3).
	err := L.DoString(`Unit_UseItem(u, 0, 1)`)
	t.Logf("FSV #267 variadic guard: extra-arg -> %v", oneLineErr(err))
	if err == nil {
		t.Fatal("passing an option arg to a variadic verb must raise (fail-closed), not silently drop")
	}
}

// TestGoVsLuaIdenticalHashFSV — edge 4: an identical CreateUnit/SetPosition/Kill
// sequence, once via direct Go api and once via the generated Lua bindings,
// against two same-seed games, produces a bit-identical state hash after 200
// ticks. If the binding layer marshalled an argument wrong or dispatched to a
// different verb, the hashes would diverge.
func TestGoVsLuaIdenticalHashFSV(t *testing.T) {
	const seed = 99
	const ticks = 200

	// --- Go driver: direct api calls. ---
	gGo := loaderGame(t, seed)
	{
		p := gGo.Player(1)
		typ := gGo.UnitType("hfoo")
		u0 := gGo.CreateUnit(p, typ, api.Vec2{X: 0, Y: 0}, api.Deg(0))
		u1 := gGo.CreateUnit(p, typ, api.Vec2{X: 10, Y: 0}, api.Deg(0))
		u2 := gGo.CreateUnit(p, typ, api.Vec2{X: 20, Y: 0}, api.Deg(0))
		u0.SetPosition(api.Vec2{X: 100, Y: 50})
		u1.SetPosition(api.Vec2{X: 200, Y: 75})
		u2.Kill()
	}
	gGo.Advance(ticks)
	hGo := gGo.StateHash()

	// --- Lua driver: the SAME sequence through the generated bindings. ---
	gLua := loaderGame(t, seed)
	L := boundState(t, gLua)
	defer L.Close()
	script := `
u0 = Game_CreateUnit(Game_Player(1), Game_UnitType("hfoo"), {x = 0,  y = 0}, 0)
u1 = Game_CreateUnit(Game_Player(1), Game_UnitType("hfoo"), {x = 10, y = 0}, 0)
u2 = Game_CreateUnit(Game_Player(1), Game_UnitType("hfoo"), {x = 20, y = 0}, 0)
Unit_SetPosition(u0, {x = 100, y = 50})
Unit_SetPosition(u1, {x = 200, y = 75})
Unit_Kill(u2)
`
	if err := L.DoString(script); err != nil {
		t.Fatalf("Lua scenario must run through the bindings: %v", err)
	}
	gLua.Advance(ticks)
	hLua := gLua.StateHash()

	t.Logf("FSV #267 edge4: Go hash=%#x  Lua hash=%#x (seed %d, %d ticks)", hGo, hLua, seed, ticks)
	if hGo != hLua {
		t.Fatalf("Go and Lua drivers diverged: %#x != %#x — bindings are NOT a faithful skin", hGo, hLua)
	}
}

// TestBindingTypeConfusionFSV — #267 edge 2 / #319 "binding-layer type
// confusion". Feeding a binding the wrong argument type (a primitive where a
// handle is expected, or a valid handle of the WRONG noun type) must raise a
// loud, located Lua error — never silently no-op and never panic the host
// process. SoT = a non-nil error from each attempt with the host still alive.
func TestBindingTypeConfusionFSV(t *testing.T) {
	g := loaderGame(t, 1)
	L := boundState(t, g)
	defer L.Close()
	if err := L.DoString(`u = Game_CreateUnit(Game_Player(0), Game_UnitType("hfoo"), {x=0, y=0}, 0)`); err != nil {
		t.Fatalf("setup unit: %v", err)
	}
	cases := []struct{ name, src string }{
		{"string where Player expected", `Game_CreateUnit("nope", Game_UnitType("hfoo"), {x=0,y=0}, 0)`},
		{"string where Unit expected", `Unit_Kill("nope")`},
		{"nil where Unit expected", `Unit_Kill(nil)`},
		{"number where Vec2 table expected", `Unit_SetPosition(u, 42)`},
		{"wrong noun handle (Player passed as Unit)", `Unit_Kill(Game_Player(0))`},
	}
	for _, c := range cases {
		err := L.DoString(c.src)
		t.Logf("FSV #267/#319 type-confusion %-42s -> %v", c.name, oneLineErr(err))
		if err == nil {
			t.Fatalf("type confusion NOT caught: %q returned without error", c.src)
		}
	}
}

func oneLineErr(err error) string {
	if err == nil {
		return "<nil>"
	}
	s := err.Error()
	for i := 0; i < len(s); i++ {
		if s[i] == '\n' {
			return s[:i]
		}
	}
	return s
}

// TestVec2RoundTripFSV — edge 3: a Vec2 written from Lua as {x=,y=} and read
// back through Unit_Position must compare field-equal. SoT = the values Lua sees
// after the Go sim stored them.
func TestVec2RoundTripFSV(t *testing.T) {
	g := loaderGame(t, 1)
	L := boundState(t, g)
	defer L.Close()
	script := `
u = Game_CreateUnit(Game_Player(0), Game_UnitType("hfoo"), {x = 0, y = 0}, 0)
Unit_SetPosition(u, {x = 128, y = 256})
local p = Unit_Position(u)
rtx = p.x
rty = p.y
`
	if err := L.DoString(script); err != nil {
		t.Fatalf("round-trip script: %v", err)
	}
	rtx, rty := luaNum(t, L, "rtx"), luaNum(t, L, "rty")
	t.Logf("FSV #267 edge3: Vec2 Lua->Go->Lua = {%.4f, %.4f}", rtx, rty)
	if rtx != 128 || rty != 256 {
		t.Fatalf("Vec2 round-trip lost data: got {%v,%v}, want {128,256}", rtx, rty)
	}
}

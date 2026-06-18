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
	"testing"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
)

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

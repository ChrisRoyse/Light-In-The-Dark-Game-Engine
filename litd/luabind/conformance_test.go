package luabind

// Go-vs-Lua binding CONFORMANCE suite (#267 step 4). For each mutating verb we
// build two identically-seeded games, apply the verb through the Go api on one
// and through the generated Lua binding on its twin, then assert the two games
// have an identical Game.StateHash (#267). StateHash digests the FULL
// authoritative sim state, so this catches ANY divergence — wrong field, wrong
// magnitude, an extra side effect, a missed write — not just the one value a
// targeted assertion would check. This is the parity gate the per-category FSV
// tests generalize.
//
// SoT = the authoritative sim state digest after the operation, compared
// between the Go-driven game and the Lua-driven game.

import (
	"testing"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	lua "github.com/yuin/gopher-lua"
)

// confGame builds a fixed game with one unit "hero" owned by player 1 at
// {100,100}. Two calls with the same seed are bit-identical (asserted via
// StateHash in the test before any op runs).
func confGame(t *testing.T, seed int64) (*api.Game, api.Unit) {
	t.Helper()
	g, err := api.NewGame(api.GameOptions{MaxUnits: 16, Seed: seed})
	if err != nil {
		t.Fatalf("NewGame: %v", err)
	}
	if err := g.DefineUnits([]data.Unit{
		{ID: "hfoo", Life: 100, MoveSpeedPerTick: 8 * fixed.One, TurnRatePerTick: 65535, CollisionSize: 16},
	}); err != nil {
		t.Fatalf("DefineUnits: %v", err)
	}
	u := g.CreateUnit(g.Player(1), g.UnitType("hfoo"), api.Vec2{X: 100, Y: 100}, api.Deg(0))
	if !u.Valid() {
		t.Fatal("confGame: hero invalid")
	}
	return g, u
}

// newConfState registers g on a fresh LState and presets the handle globals the
// conformance Lua snippets reference: hero (u), p1/p2 (players), ut (UnitType).
func newConfState(t *testing.T, g *api.Game, u api.Unit) *lua.LState {
	t.Helper()
	L := lua.NewState()
	if err := Register(L, g); err != nil {
		t.Fatalf("Register: %v", err)
	}
	set := func(name string, v any) {
		ud := L.NewUserData()
		ud.Value = v
		L.SetGlobal(name, ud)
	}
	set("hero", u)
	set("p1", g.Player(1))
	set("p2", g.Player(2))
	set("ut", g.UnitType("hfoo"))
	return L
}

// TestConformanceGateDetectsDivergence is the negative control: it proves the
// StateHash comparison actually CATCHES a Go-vs-Lua mismatch. We deliberately
// apply DIFFERENT magnitudes (Go SetLife 33, Lua SetLife 34) and require the
// hashes to diverge — if they matched, the conformance gate above would be
// worthless (a test that cannot fail).
func TestConformanceGateDetectsDivergence(t *testing.T) {
	const seed = 21
	gGo, uGo := confGame(t, seed)
	gLua, uLua := confGame(t, seed)
	if gGo.StateHash() != gLua.StateHash() {
		t.Fatalf("twin setup diverges: %#x != %#x", gGo.StateHash(), gLua.StateHash())
	}

	uGo.SetLife(33)
	L := newConfState(t, gLua, uLua)
	defer L.Close()
	if err := L.DoString(`Unit_SetLife(hero, 34)`); err != nil {
		t.Fatalf("lua: %v", err)
	}
	hGo, hLua := gGo.StateHash(), gLua.StateHash()
	t.Logf("divergence control: go(life=33)=%#x  lua(life=34)=%#x", hGo, hLua)
	if hGo == hLua {
		t.Fatal("gate is blind: different unit life produced the same StateHash")
	}
}

func TestLuaBindingConformance(t *testing.T) {
	cases := []struct {
		name string
		goOp func(g *api.Game, u api.Unit) // the Go api path
		lua  string                        // the Lua binding path (hero/p1/p2/ut preset)
	}{
		{"Unit_SetLife", func(g *api.Game, u api.Unit) { u.SetLife(33) }, `Unit_SetLife(hero, 33)`},
		{"Unit_SetFacing", func(g *api.Game, u api.Unit) { u.SetFacing(api.Deg(90)) }, `Unit_SetFacing(hero, 90)`},
		{"Unit_SetPosition", func(g *api.Game, u api.Unit) { u.SetPosition(api.Vec2{X: 120, Y: 140}) }, `Unit_SetPosition(hero, {x = 120, y = 140})`},
		{"Unit_SetOwner", func(g *api.Game, u api.Unit) { u.SetOwner(g.Player(2), false) }, `Unit_SetOwner(hero, p2, false)`},
		{"Player_SetRace", func(g *api.Game, u api.Unit) { g.Player(1).SetRace(2) }, `Player_SetRace(p1, 2)`},
		{"Player_SetController", func(g *api.Game, u api.Unit) { g.Player(1).SetController(1) }, `Player_SetController(p1, 1)`},
		{"Game_CreateUnit", func(g *api.Game, u api.Unit) {
			g.CreateUnit(g.Player(2), g.UnitType("hfoo"), api.Vec2{X: 200, Y: 64}, api.Deg(45))
		}, `Game_CreateUnit(p2, ut, {x = 200, y = 64}, 45)`},
	}

	for _, c := range cases {
		t.Run(c.name, func(t *testing.T) {
			const seed = 21
			gGo, uGo := confGame(t, seed)
			gLua, uLua := confGame(t, seed)

			// Guard: the twins start bit-identical (so any post-op divergence is
			// the operation's, not the setup's).
			if gGo.StateHash() != gLua.StateHash() {
				t.Fatalf("twin setup diverges before op: %#x != %#x", gGo.StateHash(), gLua.StateHash())
			}
			before := gGo.StateHash()

			// Go path.
			c.goOp(gGo, uGo)

			// Lua path on the twin: bind the twin's handles as globals, run the verb.
			L := newConfState(t, gLua, uLua)
			defer L.Close()
			if err := L.DoString(c.lua); err != nil {
				t.Fatalf("lua %q: %v", c.lua, err)
			}

			hGo, hLua := gGo.StateHash(), gLua.StateHash()
			t.Logf("%s: before=%#x  go=%#x  lua=%#x", c.name, before, hGo, hLua)
			if hGo == before {
				t.Fatalf("%s: Go path did not change state (test is vacuous)", c.name)
			}
			if hGo != hLua {
				t.Fatalf("%s: CONFORMANCE FAIL — Go and Lua produce different sim state: go=%#x lua=%#x", c.name, hGo, hLua)
			}
		})
	}
}

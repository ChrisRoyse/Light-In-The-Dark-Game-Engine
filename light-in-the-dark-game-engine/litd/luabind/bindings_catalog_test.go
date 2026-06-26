package luabind_test

// #393 FSV: the hand-written Game type-catalog resolvers (Game_UnitType etc.)
// let a world resolve the types it spawns from its OWN Lua — no host-injected
// type globals. SoT = the live sim state the resolved-type spawn produced, and
// Go-vs-Lua StateHash parity.

import (
	"testing"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/luabind"
	lua "github.com/yuin/gopher-lua"
)

func catalogGame(t *testing.T, seed int64) *api.Game {
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
	return g
}

// TestGameUnitTypeSelfContainedFSV: a script resolves its unit type via
// Game_UnitType (no injected global) and spawns — proving worlds are
// self-contained on the catalog seam.
func TestGameUnitTypeSelfContainedFSV(t *testing.T) {
	g := catalogGame(t, 1)
	L := lua.NewState()
	defer L.Close()
	if err := luabind.Register(L, g); err != nil {
		t.Fatalf("Register: %v", err)
	}
	if err := L.DoString(`Game_CreateUnit(Game_Player(0), Game_UnitType("hfoo"), { x = 200, y = 200 }, 0)`); err != nil {
		t.Fatalf("self-contained spawn script: %v", err)
	}
	units := g.UnitsInRange(api.Vec2{X: 200, Y: 200}, 8, nil)
	t.Logf("FSV Game_UnitType self-contained: resolved \"hfoo\" + spawned, unitsNearSpawn=%d", len(units))
	if len(units) != 1 {
		t.Fatalf("want 1 unit at (200,200), got %d (Game_UnitType did not resolve to a usable type)", len(units))
	}
}

// TestGameUnitTypeParityFSV: resolving the type from Lua (Game_UnitType) and
// from Go (g.UnitType) produces identical sim state after an identical spawn.
func TestGameUnitTypeParityFSV(t *testing.T) {
	const seed = 21
	gGo := catalogGame(t, seed)
	gLua := catalogGame(t, seed)
	if gGo.StateHash() != gLua.StateHash() {
		t.Fatalf("twin setup diverges: %#x != %#x", gGo.StateHash(), gLua.StateHash())
	}

	gGo.CreateUnit(gGo.Player(0), gGo.UnitType("hfoo"), api.Vec2{X: 300, Y: 300}, api.Deg(0))

	L := lua.NewState()
	defer L.Close()
	if err := luabind.Register(L, gLua); err != nil {
		t.Fatalf("Register: %v", err)
	}
	if err := L.DoString(`Game_CreateUnit(Game_Player(0), Game_UnitType("hfoo"), { x = 300, y = 300 }, 0)`); err != nil {
		t.Fatalf("lua spawn: %v", err)
	}

	hGo, hLua := gGo.StateHash(), gLua.StateHash()
	t.Logf("FSV catalog parity: go=%#x lua=%#x", hGo, hLua)
	if hGo != hLua {
		t.Fatalf("Game_UnitType parity FAIL: go=%#x lua=%#x", hGo, hLua)
	}
}

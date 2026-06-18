package luabind

// Catalog query bindings (#267): the nil-filter variants of Game.AllUnits /
// Players / UnitsInRange / UnitsIn, bound by hand because their filtered form
// takes a callback (#265-gated). SoT = the set the Lua binding returns matches
// the set the Go api returns (count + element validity), against the same game.

import (
	"testing"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	lua "github.com/yuin/gopher-lua"
)

func TestCatalogQueryBindingsFSV(t *testing.T) {
	g, err := api.NewGame(api.GameOptions{MaxUnits: 32, Seed: 7})
	if err != nil {
		t.Fatalf("NewGame: %v", err)
	}
	if err := g.DefineUnits([]data.Unit{
		{ID: "hfoo", Life: 100, MoveSpeedPerTick: 8 * fixed.One, TurnRatePerTick: 65535, CollisionSize: 16},
	}); err != nil {
		t.Fatalf("DefineUnits: %v", err)
	}
	ut := g.UnitType("hfoo")
	// Five units: three clustered near origin, two far away.
	for _, p := range []api.Vec2{{X: 0, Y: 0}, {X: 10, Y: 0}, {X: 20, Y: 0}, {X: 5000, Y: 5000}, {X: 6000, Y: 6000}} {
		if !g.CreateUnit(g.Player(1), ut, p, api.Deg(0)).Valid() {
			t.Fatalf("CreateUnit at %v failed", p)
		}
	}

	L := lua.NewState()
	defer L.Close()
	if err := Register(L, g); err != nil {
		t.Fatalf("Register: %v", err)
	}

	luaCount := func(t *testing.T, expr string) int {
		t.Helper()
		if err := L.DoString("__n = #(" + expr + ")"); err != nil {
			t.Fatalf("lua %q: %v", expr, err)
		}
		return int(lua.LVAsNumber(L.GetGlobal("__n")))
	}

	// AllUnits: Go vs Lua count parity (SoT = the sim's live-unit set).
	goAll := len(g.AllUnits(nil))
	luaAll := luaCount(t, "Game_AllUnits()")
	if goAll != luaAll || goAll != 5 {
		t.Fatalf("AllUnits: go=%d lua=%d (want 5)", goAll, luaAll)
	}

	// UnitsInRange(origin, 100): the three clustered units, not the two far ones.
	goNear := len(g.UnitsInRange(api.Vec2{}, 100, nil))
	luaNear := luaCount(t, "Game_UnitsInRange({x=0,y=0}, 100)")
	if goNear != luaNear || goNear != 3 {
		t.Fatalf("UnitsInRange: go=%d lua=%d (want 3)", goNear, luaNear)
	}

	// UnitsIn(rect around origin): same three.
	goIn := len(g.UnitsIn(api.Rect{MinX: -100, MinY: -100, MaxX: 100, MaxY: 100}, nil))
	luaIn := luaCount(t, "Game_UnitsIn({minx=-100,miny=-100,maxx=100,maxy=100})")
	if goIn != luaIn {
		t.Fatalf("UnitsIn: go=%d lua=%d", goIn, luaIn)
	}

	// Players parity.
	goP := len(g.Players(nil))
	luaP := luaCount(t, "Game_Players()")
	if goP != luaP {
		t.Fatalf("Players: go=%d lua=%d", goP, luaP)
	}

	// Every element the Lua query returns is a live handle (no zero/garbage).
	if err := L.DoString(`for _,u in ipairs(Game_AllUnits()) do assert(Valid(u), "AllUnits returned an invalid unit") end`); err != nil {
		t.Fatalf("AllUnits element validity: %v", err)
	}

	t.Logf("FSV #267 catalog queries: AllUnits go=%d lua=%d | InRange(100) go=%d lua=%d | UnitsIn go=%d lua=%d | Players go=%d lua=%d — all match",
		goAll, luaAll, goNear, luaNear, goIn, luaIn, goP, luaP)
}

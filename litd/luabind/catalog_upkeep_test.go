package luabind

// Game_SetUpkeep (#267): configures the food-tiered income tax read back by the
// Player upkeep getters bound this session. SoT = the player's upkeep rate state
// via the Go api, set from Lua and cross-checked.

import (
	"testing"

	lua "github.com/yuin/gopher-lua"
)

func approxEq(a, b float64) bool { d := a - b; return d < 1e-9 && d > -1e-9 }

func TestGameSetUpkeepBindingFSV(t *testing.T) {
	g, _ := confGame(t, 43)
	if err := g.DefineEconomy(2); err != nil {
		t.Fatalf("DefineEconomy: %v", err)
	}
	p1 := g.Player(1)
	L := lua.NewState()
	defer L.Close()
	if err := Register(L, g); err != nil {
		t.Fatalf("Register: %v", err)
	}
	pud := L.NewUserData()
	pud.Value = p1
	L.SetGlobal("p1", pud)

	// BEFORE: no upkeep configured → zero rates.
	if p1.GoldUpkeepRate() != 0 || p1.LumberUpkeepRate() != 0 {
		t.Fatalf("pre-upkeep rates nonzero: gold=%v lumber=%v", p1.GoldUpkeepRate(), p1.LumberUpkeepRate())
	}

	// Set a single always-on tier (Food:0) taxing gold 0.5, lumber 0.25 from Lua.
	if err := L.DoString(`_ok = Game_SetUpkeep({ {food = 0, rate = {0.5, 0.25}} })`); err != nil {
		t.Fatalf("Game_SetUpkeep: %v", err)
	}
	if !lua.LVAsBool(L.GetGlobal("_ok")) {
		t.Fatal("Game_SetUpkeep returned false")
	}
	// SoT via Go: the rates the Lua config produced.
	if !approxEq(p1.GoldUpkeepRate(), 0.5) || !approxEq(p1.LumberUpkeepRate(), 0.25) {
		t.Fatalf("after SetUpkeep: gold=%v lumber=%v, want 0.5/0.25", p1.GoldUpkeepRate(), p1.LumberUpkeepRate())
	}
	// And the Lua getters (bound this session) agree.
	if err := L.DoString(`_g = Player_GoldUpkeepRate(p1); _l = Player_LumberUpkeepRate(p1)`); err != nil {
		t.Fatalf("read rates: %v", err)
	}
	if !approxEq(float64(lua.LVAsNumber(L.GetGlobal("_g"))), 0.5) || !approxEq(float64(lua.LVAsNumber(L.GetGlobal("_l"))), 0.25) {
		t.Fatalf("Lua rate getters: gold=%v lumber=%v, want 0.5/0.25",
			float64(lua.LVAsNumber(L.GetGlobal("_g"))), float64(lua.LVAsNumber(L.GetGlobal("_l"))))
	}
	t.Logf("FSV #267 SetUpkeep: Lua config → gold rate 0.5 / lumber 0.25 (Go SoT + Lua getters agree)")

	// Clear with an empty array → rates back to zero.
	if err := L.DoString(`Game_SetUpkeep({})`); err != nil {
		t.Fatalf("clear upkeep: %v", err)
	}
	if p1.GoldUpkeepRate() != 0 {
		t.Fatalf("upkeep not cleared by empty array: gold=%v", p1.GoldUpkeepRate())
	}
	t.Logf("FSV #267 SetUpkeep: empty array clears upkeep (gold rate back to 0)")
}

package luabind

// FSV for the Lua trigger bindings (#463, ADR #451). SoT = (a) the action's
// own side-effect (a Lua-side counter the action mutates) observed after a
// real sim event, and (b) the sim StateHash (R-FSV-2) for the Lua-vs-Go
// parity edge. Happy path (event → condition gate → action) + the four
// mandated edges, each printing before/after state.

import (
	"testing"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	lua "github.com/yuin/gopher-lua"
)

// spawnP1Foo creates one P1 "hfoo" unit and injects it into L as global U
// so a Lua trigger can register on it.
func spawnP1Foo(t *testing.T, g *api.Game, L *lua.LState) api.Unit {
	t.Helper()
	u := g.CreateUnit(g.Player(1), g.UnitType("hfoo"), api.Vec2{X: 0, Y: 0}, api.Deg(0))
	if L != nil {
		L.SetGlobal("U", pushHandle(L, u))
	}
	return u
}

// TestLuaTriggerFiresFSV — happy path: CreateTrigger + register a unit-death
// event + a condition that passes + an action; killing the unit runs the
// action exactly once. SoT = the Lua `fired` counter the action mutates.
func TestLuaTriggerFiresFSV(t *testing.T) {
	g, L := eventGame(t)
	defer L.Close()
	u := spawnP1Foo(t, g, L)

	if err := L.DoString(`
		fired = 0
		function ownerIsP1(e) return Player_Slot(Unit_Owner(Event_Unit(e))) == 1 end
		function printAct(e) fired = fired + 1 end
		t = CreateTrigger()
		TriggerRegisterUnitEvent(t, U, "death")
		TriggerAddCondition(t, ownerIsP1)
		TriggerAddAction(t, printAct)`); err != nil {
		t.Fatalf("build trigger: %v", err)
	}
	t.Logf("BEFORE kill: Lua fired=%v", luaNum(t, L, "fired"))
	if luaNum(t, L, "fired") != 0 {
		t.Fatal("action ran before any event")
	}

	u.Kill()
	g.Advance(1)

	got := luaNum(t, L, "fired")
	t.Logf("AFTER kill+Advance: Lua fired=%v (want 1); sim StateHash=%#x", got, g.StateHash())
	if got != 1 {
		t.Fatalf("P1 unit death did not run the action: fired=%v want 1", got)
	}
}

// TestLuaTriggerConditionGateFSV — edge 1: a condition returning false skips
// the action. Two units; the trigger fires on either death but its condition
// only passes while gateOpen is true.
func TestLuaTriggerConditionGateFSV(t *testing.T) {
	g, L := eventGame(t)
	defer L.Close()
	u1 := g.CreateUnit(g.Player(1), g.UnitType("hfoo"), api.Vec2{X: 0, Y: 0}, api.Deg(0))
	u2 := g.CreateUnit(g.Player(1), g.UnitType("hfoo"), api.Vec2{X: 32, Y: 0}, api.Deg(0))
	L.SetGlobal("U1", pushHandle(L, u1))
	L.SetGlobal("U2", pushHandle(L, u2))

	if err := L.DoString(`
		fired = 0
		gateOpen = false
		function gate(e) return gateOpen end
		function act(e) fired = fired + 1 end
		t = CreateTrigger()
		TriggerRegisterUnitEvent(t, U1, "death")
		TriggerRegisterUnitEvent(t, U2, "death")
		TriggerAddCondition(t, gate)
		TriggerAddAction(t, act)`); err != nil {
		t.Fatalf("build: %v", err)
	}

	// gate closed → kill U1 → action skipped.
	t.Logf("BEFORE (gate closed): fired=%v", luaNum(t, L, "fired"))
	u1.Kill()
	g.Advance(1)
	mid := luaNum(t, L, "fired")
	t.Logf("AFTER U1 death (gate false): fired=%v (want 0 — condition gated)", mid)
	if mid != 0 {
		t.Fatalf("condition false but action ran: fired=%v", mid)
	}

	// open the gate → kill U2 → action runs.
	if err := L.DoString(`gateOpen = true`); err != nil {
		t.Fatal(err)
	}
	u2.Kill()
	g.Advance(1)
	end := luaNum(t, L, "fired")
	t.Logf("AFTER U2 death (gate true): fired=%v (want 1)", end)
	if end != 1 {
		t.Fatalf("condition true but action did not run: fired=%v", end)
	}
}

// TestLuaTriggerDisableFSV — edge 2: a disabled trigger does not fire.
func TestLuaTriggerDisableFSV(t *testing.T) {
	g, L := eventGame(t)
	defer L.Close()
	u := spawnP1Foo(t, g, L)

	if err := L.DoString(`
		fired = 0
		function act(e) fired = fired + 1 end
		t = CreateTrigger()
		TriggerRegisterUnitEvent(t, U, "death")
		TriggerAddAction(t, act)
		DisableTrigger(t)`); err != nil {
		t.Fatalf("build: %v", err)
	}
	t.Logf("BEFORE (disabled): fired=%v", luaNum(t, L, "fired"))

	u.Kill()
	g.Advance(1)

	got := luaNum(t, L, "fired")
	t.Logf("AFTER kill (trigger disabled): fired=%v (want 0)", got)
	if got != 0 {
		t.Fatalf("disabled trigger fired: fired=%v", got)
	}
}

// TestLuaTriggerExecuteBypassesConditionFSV — edge 3: TriggerExecute runs the
// actions directly, bypassing events AND a false condition.
func TestLuaTriggerExecuteBypassesConditionFSV(t *testing.T) {
	g, L := eventGame(t)
	defer L.Close()

	if err := L.DoString(`
		fired = 0
		function never(e) return false end
		function act(e) fired = fired + 1 end
		t = CreateTrigger()
		TriggerAddCondition(t, never)
		TriggerAddAction(t, act)
		evalBefore = TriggerEvaluate(t) and 1 or 0   -- condition is false → 0
		TriggerExecute(t)                            -- runs action anyway`); err != nil {
		t.Fatalf("build+execute: %v", err)
	}
	g.Advance(1) // let the action-runner continuation (if any) settle

	eval := luaNum(t, L, "evalBefore")
	got := luaNum(t, L, "fired")
	t.Logf("TriggerEvaluate=%v (want 0/false); after TriggerExecute fired=%v (want 1 — condition bypassed)", eval, got)
	if eval != 0 {
		t.Fatal("TriggerEvaluate returned true for a false condition")
	}
	if got != 1 {
		t.Fatalf("TriggerExecute did not run the action past the false condition: fired=%v", got)
	}
}

// TestLuaTriggerVsGoParityFSV — edge 4: a trigger built through the Lua
// bindings produces a byte-identical sim trigger slab to the same trigger
// built through the Go api. SoT = g.StateHash() (covers the trigger slab +
// handler registry + boolexpr arena). Both games share the same Register'd
// baseline, so any difference is the build path.
func TestLuaTriggerVsGoParityFSV(t *testing.T) {
	// Lua-built.
	gLua, L := eventGame(t)
	defer L.Close()
	spawnP1Foo(t, gLua, L)
	if err := L.DoString(`
		function cond(e) return true end
		function act(e) end
		t = CreateTrigger()
		TriggerRegisterUnitEvent(t, U, "death")
		TriggerAddCondition(t, cond)
		TriggerAddAction(t, act)`); err != nil {
		t.Fatalf("lua build: %v", err)
	}

	// Go-built — identical structure on an identically-seeded game.
	gGo, LGo := eventGame(t)
	defer LGo.Close()
	uGo := gGo.CreateUnit(gGo.Player(1), gGo.UnitType("hfoo"), api.Vec2{X: 0, Y: 0}, api.Deg(0))
	tr := gGo.NewTrigger()
	tr.On(api.EventUnitDeath, api.ForUnit(uGo))
	tr.WhenEvent(func(api.Event) bool { return true })
	tr.Do(func(api.Event) {})

	hLua, hGo := gLua.StateHash(), gGo.StateHash()
	t.Logf("Lua-built StateHash=%#x | Go-built StateHash=%#x", hLua, hGo)
	if hLua != hGo {
		t.Fatalf("Lua trigger build diverged from the Go build: %#x != %#x", hLua, hGo)
	}
}

package luabind

// FSV for the Lua lifecycle-event binders (#470): OnAbilityCast/OnAttack/
// OnBuffApplied globals + Event_Ability. SoT = a Lua handler's observed
// side-effect (it counts and inspects the event) driven by a REAL buff apply
// through the public api, plus the binder-registration wiring.

import (
	"testing"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	lua "github.com/yuin/gopher-lua"
)

func lifecycleGame(t *testing.T) (*api.Game, *lua.LState, api.Unit) {
	t.Helper()
	g, err := api.NewGame(api.GameOptions{MaxUnits: 16, Seed: 9})
	if err != nil {
		t.Fatalf("NewGame: %v", err)
	}
	if err := g.DefineUnits([]data.Unit{
		{ID: "hfoo", Life: 100, MoveSpeedPerTick: 8 * fixed.One, TurnRatePerTick: 65535, CollisionSize: 16},
	}); err != nil {
		t.Fatalf("DefineUnits: %v", err)
	}
	if err := g.DefineBuffTypes([]data.BuffType{
		{ID: "slow", DurationTicks: 40, Stacking: data.StackCount, MaxStacks: 3},
	}); err != nil {
		t.Fatalf("DefineBuffTypes: %v", err)
	}
	u := g.CreateUnit(g.Player(1), g.UnitType("hfoo"), api.Vec2{X: 100, Y: 100}, api.Deg(0))
	if !u.Valid() {
		t.Fatal("unit invalid")
	}
	L := lua.NewState()
	if err := Register(L, g); err != nil {
		t.Fatalf("Register: %v", err)
	}
	uud := L.NewUserData()
	uud.Value = u
	L.SetGlobal("u", uud)
	return g, L, u
}

// TestLuaOnBuffAppliedFiresFSV — a real Unit_ApplyBuff fires a Lua
// OnBuffApplied handler; the handler reads e.Unit() (the buffed unit) and
// Event_Ability(e) (0 on a non-ability event, the safe-degrade).
func TestLuaOnBuffAppliedFiresFSV(t *testing.T) {
	g, L, u := lifecycleGame(t)
	defer L.Close()
	slow := g.BuffType("slow")

	if err := L.DoString(`applied = 0; unitAlive = false; abOnBuff = -1
		OnBuffApplied(function(e)
			applied = applied + 1
			unitAlive = Unit_Alive(Event_Unit(e))
			abOnBuff = Event_Ability(e)
		end)`); err != nil {
		t.Fatalf("OnBuffApplied register: %v", err)
	}
	if got := luaNum(t, L, "applied"); got != 0 {
		t.Fatalf("applied should start at 0, got %v", got)
	}

	// Trigger: apply a buff for real, then advance so the event flushes.
	if err := L.DoString(`Unit_ApplyBuff(u, Game_BuffType("slow"))`); err != nil {
		t.Fatalf("Unit_ApplyBuff: %v", err)
	}
	g.Advance(1)

	if got := luaNum(t, L, "applied"); got != 1 {
		t.Fatalf("after apply+Advance: applied=%v, want 1 (handler did not fire)", got)
	}
	if !lua.LVAsBool(L.GetGlobal("unitAlive")) {
		t.Fatal("Event_Unit(e) is not a live unit")
	}
	if got := luaNum(t, L, "abOnBuff"); got != 0 {
		t.Fatalf("Event_Ability on a buff event = %v, want 0 (safe-degrade)", got)
	}
	// Go-side SoT: the buff really landed on u (the only unit), so the handler's
	// Event_Unit is necessarily u.
	if !u.HasBuff(slow) {
		t.Fatal("the buff is not on the unit in the store")
	}
	t.Logf("FSV: real buff apply fired Lua OnBuffApplied; Event_Unit is the live buffed unit; Event_Ability degraded to 0")
}

// TestLuaLifecycleBindersRegistered — the three convenience globals exist and
// register a real (non-nil) subscription handle.
func TestLuaLifecycleBindersRegistered(t *testing.T) {
	_, L, _ := lifecycleGame(t)
	defer L.Close()

	for _, name := range []string{"OnAbilityCast", "OnAttack", "OnBuffApplied"} {
		if L.GetGlobal(name).Type() != lua.LTFunction {
			t.Fatalf("global %s is not a function (binder not registered)", name)
		}
		if err := L.DoString(`_sub = ` + name + `(function() end)`); err != nil {
			t.Fatalf("%s register: %v", name, err)
		}
		if L.GetGlobal("_sub").Type() == lua.LTNil {
			t.Fatalf("%s returned nil, want a Subscription handle", name)
		}
	}
	t.Logf("FSV: OnAbilityCast/OnAttack/OnBuffApplied registered and return handles")
}

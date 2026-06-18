package luabind

// Buff apply + handle lifecycle (#267): Unit_ApplyBuff returns a Buff handle the
// script can query (Type/Stacks/Present/RemainingSeconds) and Remove. SoT = the
// unit's buff state via the Go api (Unit.HasBuff / Unit.BuffCount) and the buff
// instance, cross-checked against the Lua handle getters across expiry.

import (
	"testing"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	lua "github.com/yuin/gopher-lua"
)

func TestUnitApplyBuffLifecycleFSV(t *testing.T) {
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
		{ID: "slow", DurationTicks: 40, Stacking: data.StackCount, MaxStacks: 3}, // 2.0s
	}); err != nil {
		t.Fatalf("DefineBuffTypes: %v", err)
	}
	u := g.CreateUnit(g.Player(1), g.UnitType("hfoo"), api.Vec2{X: 100, Y: 100}, api.Deg(0))
	if !u.Valid() {
		t.Fatal("unit invalid")
	}
	bt := g.BuffType("slow")

	L := lua.NewState()
	defer L.Close()
	if err := Register(L, g); err != nil {
		t.Fatalf("Register: %v", err)
	}
	uud := L.NewUserData()
	uud.Value = u
	L.SetGlobal("u", uud)

	// BEFORE: no buff.
	if u.HasBuff(bt) || u.BuffCount() != 0 {
		t.Fatalf("BEFORE: HasBuff=%v count=%d, want false/0", u.HasBuff(bt), u.BuffCount())
	}

	// Apply via Lua, holding the returned handle as a global.
	if err := L.DoString(`b = Unit_ApplyBuff(u, Game_BuffType("slow"))`); err != nil {
		t.Fatalf("Unit_ApplyBuff: %v", err)
	}
	// SoT (Go): the buff is really on the unit.
	if !u.HasBuff(bt) || u.BuffCount() != 1 {
		t.Fatalf("AFTER apply: HasBuff=%v count=%d, want true/1", u.HasBuff(bt), u.BuffCount())
	}
	// Lua handle getters agree.
	readF := func(expr string) float64 {
		if err := L.DoString(`_v = ` + expr); err != nil {
			t.Fatalf("%s: %v", expr, err)
		}
		return float64(lua.LVAsNumber(L.GetGlobal("_v")))
	}
	readB := func(expr string) bool {
		if err := L.DoString(`_v = ` + expr); err != nil {
			t.Fatalf("%s: %v", expr, err)
		}
		return lua.LVAsBool(L.GetGlobal("_v"))
	}
	if !readB(`Buff_Present(b)`) {
		t.Fatal("Buff_Present false right after apply")
	}
	if s := readF(`Buff_Stacks(b)`); s != 1 {
		t.Fatalf("Buff_Stacks = %v, want 1", s)
	}
	if rs := readF(`Buff_RemainingSeconds(b)`); rs <= 1.5 || rs > 2.0 {
		t.Fatalf("Buff_RemainingSeconds = %v just after apply, want ~2.0", rs)
	}
	// Buff_Type(b) identifies the same type as Game_BuffType("slow") (pooled ==).
	if !readB(`Buff_Type(b) == Game_BuffType("slow")`) {
		t.Fatal("Buff_Type(b) != Game_BuffType(\"slow\")")
	}
	t.Logf("FSV #267 buff apply: HasBuff(Go)=true count=1; Lua Present=true stacks=1 remaining≈2.0 type matches")

	// Advance 1s — duration ticks down, still present.
	g.Advance(20)
	if rs := readF(`Buff_RemainingSeconds(b)`); rs <= 0.5 || rs >= 1.5 {
		t.Fatalf("Buff_RemainingSeconds after 1s = %v, want ~1.0", rs)
	}
	if !readB(`Buff_Present(b)`) {
		t.Fatal("Buff_Present false at 1s (should still be live)")
	}

	// Advance past expiry (total 50 > 40) — buff gone everywhere.
	g.Advance(30)
	if u.HasBuff(bt) || u.BuffCount() != 0 {
		t.Fatalf("after expiry: Go HasBuff=%v count=%d, want false/0", u.HasBuff(bt), u.BuffCount())
	}
	if readB(`Buff_Present(b)`) {
		t.Fatal("Buff_Present true after expiry")
	}
	if s := readF(`Buff_Stacks(b)`); s != 0 {
		t.Fatalf("Buff_Stacks after expiry = %v, want 0", s)
	}
	t.Logf("FSV #267 buff expiry: at 1s remaining≈1.0 present; past 2.0s → Go HasBuff=false, Lua Present=false stacks=0")

	// Edge: Buff_Remove dispels a freshly-applied buff immediately.
	if err := L.DoString(`b2 = Unit_ApplyBuff(u, Game_BuffType("slow"))`); err != nil {
		t.Fatalf("re-apply: %v", err)
	}
	if !u.HasBuff(bt) {
		t.Fatal("re-applied buff not present (Go)")
	}
	if err := L.DoString(`Buff_Remove(b2)`); err != nil {
		t.Fatalf("Buff_Remove: %v", err)
	}
	if u.HasBuff(bt) || u.BuffCount() != 0 {
		t.Fatalf("after Buff_Remove: Go HasBuff=%v count=%d, want false/0", u.HasBuff(bt), u.BuffCount())
	}
	t.Logf("FSV #267 buff remove: re-applied then Buff_Remove → Go HasBuff=false count=0")
}

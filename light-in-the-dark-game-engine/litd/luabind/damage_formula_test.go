package luabind

// FSV for the Lua programmable-combat surface (#475): ReplaceDamageStage +
// the full read/write DamageEvent + SetArmorReduction, driven through a REAL
// attack. SoT = the victim's life after the hit + the fields the Lua stage
// observed + Game.StateHash().

import (
	"testing"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	lua "github.com/yuin/gopher-lua"
)

// combatLuaGame: a game with two attack types (normal, holy), one armor type,
// a 2×1 matrix (normal 100%, holy 200%), an attacker, and a high-life victim.
func combatLuaGame(t *testing.T) (*api.Game, *lua.LState, api.Unit, api.Unit) {
	t.Helper()
	g, err := api.NewGame(api.GameOptions{MaxUnits: 16, Seed: 3})
	if err != nil {
		t.Fatalf("NewGame: %v", err)
	}
	if err := g.DefineUnits([]data.Unit{
		{ID: "hfoo", Life: 1000, MoveSpeedPerTick: 8 * fixed.One, TurnRatePerTick: 65535, CollisionSize: 16},
	}); err != nil {
		t.Fatalf("DefineUnits: %v", err)
	}
	if err := g.DefineDamageTypes([]string{"normal", "holy"}, []string{"unarmored"}); err != nil {
		t.Fatalf("DefineDamageTypes: %v", err)
	}
	if err := g.DefineCombat([][]int{{1000}, {2000}}); err != nil {
		t.Fatalf("DefineCombat: %v", err)
	}
	attacker := g.CreateUnit(g.Player(1), g.UnitType("hfoo"), api.Vec2{X: 0, Y: 0}, api.Deg(0))
	victim := g.CreateUnit(g.Player(2), g.UnitType("hfoo"), api.Vec2{X: 200, Y: 0}, api.Deg(0))
	if !attacker.Valid() || !victim.Valid() {
		t.Fatal("unit spawn failed")
	}
	L := lua.NewState()
	if err := Register(L, g); err != nil {
		t.Fatalf("Register: %v", err)
	}
	return g, L, attacker, victim
}

// TestLuaReplaceDamageStageSetZeroFSV — a Lua stage sets the amount to 0 → the
// real attack deals no damage.
func TestLuaReplaceDamageStageSetZeroFSV(t *testing.T) {
	g, L, attacker, victim := combatLuaGame(t)
	defer L.Close()
	if err := L.DoString(`ReplaceDamageStage("script-modifier", function(e)
		DamageEvent_SetAmount(e, 0)
	end)`); err != nil {
		t.Fatalf("ReplaceDamageStage: %v", err)
	}
	before := victim.Life()
	attacker.Damage(victim, 100, api.WithAttackType(0))
	g.Advance(1)
	after := victim.Life()
	t.Logf("FSV #475 Lua set-zero: before=%.0f after=%.0f", before, after)
	if after != before {
		t.Fatalf("victim took damage (%.0f→%.0f) after Lua SetAmount(0)", before, after)
	}
}

// TestLuaSetAttackTypeMidPipelineFSV — a Lua stage reads the attack type,
// switches it to holy, and re-applies the coefficient: 50 raw → 100 (holy 200%).
func TestLuaSetAttackTypeMidPipelineFSV(t *testing.T) {
	g, L, attacker, victim := combatLuaGame(t)
	defer L.Close()
	if err := L.DoString(`sawType = ""; okSwitch = false
		ReplaceDamageStage("coeff-lookup", function(e)
			sawType = DamageEvent_AttackType(e)
			okSwitch = DamageEvent_SetAttackType(e, "holy")
			DamageEvent_ApplyCoefficient(e)
		end)`); err != nil {
		t.Fatalf("ReplaceDamageStage: %v", err)
	}
	before := victim.Life()
	attacker.Damage(victim, 50, api.WithAttackType(0)) // queued as normal
	g.Advance(1)
	delta := before - victim.Life()
	t.Logf("FSV #475 Lua holy: sawType=%q okSwitch=%v delta=%.0f want=100",
		lua.LVAsString(L.GetGlobal("sawType")), lua.LVAsBool(L.GetGlobal("okSwitch")), delta)
	if lua.LVAsString(L.GetGlobal("sawType")) != "normal" {
		t.Fatalf("stage read attack type %q, want normal", lua.LVAsString(L.GetGlobal("sawType")))
	}
	if !lua.LVAsBool(L.GetGlobal("okSwitch")) {
		t.Fatal("SetAttackType(holy) returned false")
	}
	if delta != 100 {
		t.Fatalf("holy delta = %.0f, want 100 (50·2000/1000)", delta)
	}
}

// TestLuaDamageEventInvalidWriteFSV — an unknown attack-type name returns false
// (fail-closed) and leaves the hit as normal.
func TestLuaDamageEventInvalidWriteFSV(t *testing.T) {
	g, L, attacker, victim := combatLuaGame(t)
	defer L.Close()
	if err := L.DoString(`badOK = true; rawSeen = -1
		ReplaceDamageStage("coeff-lookup", function(e)
			rawSeen = DamageEvent_RawAmount(e)
			badOK = DamageEvent_SetAttackType(e, "does-not-exist")
			DamageEvent_ApplyCoefficient(e)
		end)`); err != nil {
		t.Fatalf("ReplaceDamageStage: %v", err)
	}
	before := victim.Life()
	attacker.Damage(victim, 60, api.WithAttackType(0))
	g.Advance(1)
	delta := before - victim.Life()
	t.Logf("FSV #475 Lua invalid: badOK=%v rawSeen=%v delta=%.0f",
		lua.LVAsBool(L.GetGlobal("badOK")), lua.LVAsNumber(L.GetGlobal("rawSeen")), delta)
	if lua.LVAsBool(L.GetGlobal("badOK")) {
		t.Fatal("SetAttackType(unknown) returned true — must fail closed")
	}
	if float64(lua.LVAsNumber(L.GetGlobal("rawSeen"))) != 60 {
		t.Fatalf("RawAmount read = %v, want 60", lua.LVAsNumber(L.GetGlobal("rawSeen")))
	}
	if delta != 60 {
		t.Fatalf("delta = %.0f, want 60 (normal coeff, attack type unchanged)", delta)
	}
}

// TestLuaProgrammableCombatHashAndArmorFSV — a Lua override changes
// Game.StateHash (identity bound) and SetArmorReduction is callable from Lua.
func TestLuaProgrammableCombatHashAndArmorFSV(t *testing.T) {
	g, L, _, _ := combatLuaGame(t)
	defer L.Close()
	base := g.StateHash()
	if err := L.DoString(`ReplaceDamageStage("armor-reduction", function(e) end)`); err != nil {
		t.Fatalf("ReplaceDamageStage: %v", err)
	}
	over := g.StateHash()
	t.Logf("FSV #475 Lua hash: base=%#016x override=%#016x", base, over)
	if over == base {
		t.Fatal("Lua stage override did not change the state hash")
	}
	if err := L.DoString(`SetArmorReduction(0.10)`); err != nil {
		t.Fatalf("SetArmorReduction: %v", err)
	}
	t.Log("FSV #475 Lua: override hashes; SetArmorReduction callable")
}

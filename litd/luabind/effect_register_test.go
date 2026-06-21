package luabind

// FSV for the #477 Lua surface: RegisterEffect / RunEffect. SoT = the source
// unit's life after a script-registered "lifesteal" effect runs through a real
// invocation, plus fail-closed registration.

import (
	"testing"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	lua "github.com/yuin/gopher-lua"
)

func effectLuaGame(t *testing.T) (*api.Game, *lua.LState, api.Unit, api.Unit) {
	t.Helper()
	g, err := api.NewGame(api.GameOptions{MaxUnits: 16, Seed: 11})
	if err != nil {
		t.Fatalf("NewGame: %v", err)
	}
	if err := g.DefineUnits([]data.Unit{
		{ID: "hfoo", Life: 1000, MoveSpeedPerTick: 8 * fixed.One, TurnRatePerTick: 65535, CollisionSize: 16},
	}); err != nil {
		t.Fatalf("DefineUnits: %v", err)
	}
	src := g.CreateUnit(g.Player(1), g.UnitType("hfoo"), api.Vec2{X: 0, Y: 0}, api.Deg(0))
	tgt := g.CreateUnit(g.Player(2), g.UnitType("hfoo"), api.Vec2{X: 50, Y: 0}, api.Deg(0))
	if !src.Valid() || !tgt.Valid() {
		t.Fatal("spawn failed")
	}
	src.SetLife(500) // below max, room to heal
	L := lua.NewState()
	if err := Register(L, g); err != nil {
		t.Fatalf("Register: %v", err)
	}
	set := func(name string, v any) {
		ud := L.NewUserData()
		ud.Value = v
		L.SetGlobal(name, ud)
	}
	set("src", src)
	set("tgt", tgt)
	return g, L, src, tgt
}

// TestLuaRegisterAndRunEffectFSV — a script registers "lifesteal" (heals its
// source) and invokes it via RunEffect; the source heals by the scripted amount.
func TestLuaRegisterAndRunEffectFSV(t *testing.T) {
	_, L, src, _ := effectLuaGame(t)
	defer L.Close()
	if err := L.DoString(`
		RegisterEffect("lifesteal", function(s, t)
			Unit_SetLife(s, Unit_Life(s) + 200)
		end)
		registered = EffectRegistered("lifesteal")
		ran = RunEffect("lifesteal", src, tgt)
	`); err != nil {
		t.Fatalf("DoString: %v", err)
	}
	if L.GetGlobal("registered") != lua.LTrue || L.GetGlobal("ran") != lua.LTrue {
		t.Fatalf("registered=%v ran=%v, want true/true", L.GetGlobal("registered"), L.GetGlobal("ran"))
	}
	t.Logf("FSV #477 lua: lifesteal source life now %.0f", src.Life())
	if src.Life() != 700 {
		t.Fatalf("source life = %.0f, want 700 (500 + 200 scripted lifesteal)", src.Life())
	}
}

// TestLuaRunEffectUnknownFSV — RunEffect on an unregistered name is a false
// no-op, not an error.
func TestLuaRunEffectUnknownFSV(t *testing.T) {
	_, L, _, _ := effectLuaGame(t)
	defer L.Close()
	if err := L.DoString(`ran = RunEffect("nonesuch", src, tgt)`); err != nil {
		t.Fatalf("DoString: %v", err)
	}
	if L.GetGlobal("ran") != lua.LFalse {
		t.Fatalf("RunEffect(unknown) = %v, want false", L.GetGlobal("ran"))
	}
	t.Log("FSV #477 lua: RunEffect on an unregistered name returns false (no-op)")
}

// TestLuaRegisterEffectFailClosedFSV — a duplicate registration raises (the
// setup verb fails closed and the script sees it).
func TestLuaRegisterEffectFailClosedFSV(t *testing.T) {
	_, L, _, _ := effectLuaGame(t)
	defer L.Close()
	err := L.DoString(`
		RegisterEffect("dup", function(s, t) end)
		RegisterEffect("dup", function(s, t) end)
	`)
	if err == nil {
		t.Fatal("duplicate RegisterEffect did not raise")
	}
	t.Logf("FSV #477 lua fail-closed: duplicate registration raised: %v", err)
}

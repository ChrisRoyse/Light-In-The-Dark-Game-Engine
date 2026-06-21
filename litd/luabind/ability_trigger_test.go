package luabind

// FSV for the #478 Lua surface: BindTriggerName. SoT = the sim's name→trigger
// resolution after a script binds a trigger to a name.

import (
	"testing"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	lua "github.com/yuin/gopher-lua"
)

// TestLuaBindTriggerNameFSV — a script creates a trigger and binds it to a name;
// the sim resolves the name to that trigger.
func TestLuaBindTriggerNameFSV(t *testing.T) {
	g, err := api.NewGame(api.GameOptions{MaxUnits: 8, Seed: 3})
	if err != nil {
		t.Fatalf("NewGame: %v", err)
	}
	L := lua.NewState()
	defer L.Close()
	if err := Register(L, g); err != nil {
		t.Fatalf("Register: %v", err)
	}
	// create the trigger first; the binding (not the bare trigger) is the SoT we
	// watch in the hash.
	if err := L.DoString(`tr = CreateTrigger()`); err != nil {
		t.Fatalf("DoString create: %v", err)
	}
	before := g.StateHash()
	if err := L.DoString(`ok = BindTriggerName("spell", tr)`); err != nil {
		t.Fatalf("DoString bind: %v", err)
	}
	after := g.StateHash() // differs from `before` only by the new binding
	if err := L.DoString(`dup = BindTriggerName("spell", tr)`); err != nil {
		t.Fatalf("DoString dup: %v", err)
	}
	t.Logf("FSV #478 lua: ok=%v dup=%v, hash %#016x→%#016x", L.GetGlobal("ok"), L.GetGlobal("dup"), before, after)
	if L.GetGlobal("ok") != lua.LTrue {
		t.Fatalf("BindTriggerName = %v, want true", L.GetGlobal("ok"))
	}
	if L.GetGlobal("dup") != lua.LFalse {
		t.Fatalf("duplicate BindTriggerName = %v, want false", L.GetGlobal("dup"))
	}
	if after == before {
		t.Fatal("binding did not change the state hash — the name→trigger pair must hash")
	}
}

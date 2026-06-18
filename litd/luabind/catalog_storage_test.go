package luabind

// Storage_GetInt binding (#267): completes the Lua round-trip for saved ints
// (Storage_SetInt was already generated). SoT = the Storage's actual stored
// value, read back both through the Lua binding AND the Go api after a Lua write.

import (
	"testing"

	lua "github.com/yuin/gopher-lua"
)

func TestStorageGetIntBindingFSV(t *testing.T) {
	g, _ := confGame(t, 9)
	L := lua.NewState()
	defer L.Close()
	if err := Register(L, g); err != nil {
		t.Fatalf("Register: %v", err)
	}

	// Lua writes via the generated Storage_SetInt, reads via the new GetInt.
	if err := L.DoString(`
		local s = Game_Storage()
		Storage_SetInt(s, "score", "p1", 42)
		_v, _ok = Storage_GetInt(s, "score", "p1")
		_mv, _mok = Storage_GetInt(s, "score", "missing")
	`); err != nil {
		t.Fatalf("lua: %v", err)
	}
	if v := int(lua.LVAsNumber(L.GetGlobal("_v"))); v != 42 {
		t.Fatalf("Lua GetInt value = %d, want 42", v)
	}
	if ok := lua.LVAsBool(L.GetGlobal("_ok")); !ok {
		t.Fatal("Lua GetInt found-flag = false for a written key")
	}
	// Missing key: zero value + false found-flag (not a silent 0-is-present bug).
	if v := int(lua.LVAsNumber(L.GetGlobal("_mv"))); v != 0 {
		t.Fatalf("Lua GetInt(missing) value = %d, want 0", v)
	}
	if ok := lua.LVAsBool(L.GetGlobal("_mok")); ok {
		t.Fatal("Lua GetInt(missing) found-flag = true, want false")
	}

	// Cross-check against the Go api: the Lua SetInt wrote real Storage state.
	if gv, gok := g.Storage().GetInt("score", "p1"); gv != 42 || !gok {
		t.Fatalf("Go-side Storage.GetInt = (%d,%v), want (42,true) — Lua write did not persist", gv, gok)
	}
	t.Logf("FSV #267 Storage_GetInt: Lua wrote 42, Lua read (42,true) + missing (0,false), Go confirms (42,true)")
}

package luabind

// Storage typed accessors (#267): Real/String/Bool round-trips that complete the
// save-data surface alongside the already-bound Int. SoT = the Storage's actual
// stored value, read back through BOTH the Lua binding and the Go api after a Lua
// write (cross-language), plus missing-key behavior.

import (
	"testing"

	lua "github.com/yuin/gopher-lua"
)

func TestStorageTypedAccessorsFSV(t *testing.T) {
	g, _ := confGame(t, 23)
	L := lua.NewState()
	defer L.Close()
	if err := Register(L, g); err != nil {
		t.Fatalf("Register: %v", err)
	}

	if err := L.DoString(`
		local s = Game_Storage()
		Storage_SetReal(s,   "r", "pi",   3.5)
		Storage_SetString(s, "t", "name", "vigil")
		Storage_SetBool(s,   "b", "flag", true)
		_rv, _rok = Storage_GetReal(s,   "r", "pi")
		_sv, _sok = Storage_GetString(s, "t", "name")
		_bv, _bok = Storage_GetBool(s,   "b", "flag")
		-- missing keys: zero value + false found-flag
		_mrv, _mrok = Storage_GetReal(s,   "r", "absent")
		_msv, _msok = Storage_GetString(s, "t", "absent")
		_mbv, _mbok = Storage_GetBool(s,   "b", "absent")
	`); err != nil {
		t.Fatalf("lua: %v", err)
	}

	// Lua-side reads.
	if v := float64(lua.LVAsNumber(L.GetGlobal("_rv"))); v != 3.5 || !lua.LVAsBool(L.GetGlobal("_rok")) {
		t.Fatalf("GetReal Lua = (%v,%v), want (3.5,true)", v, lua.LVAsBool(L.GetGlobal("_rok")))
	}
	if v := lua.LVAsString(L.GetGlobal("_sv")); v != "vigil" || !lua.LVAsBool(L.GetGlobal("_sok")) {
		t.Fatalf("GetString Lua = (%q,%v), want (vigil,true)", v, lua.LVAsBool(L.GetGlobal("_sok")))
	}
	if v := lua.LVAsBool(L.GetGlobal("_bv")); !v || !lua.LVAsBool(L.GetGlobal("_bok")) {
		t.Fatalf("GetBool Lua = (%v,%v), want (true,true)", v, lua.LVAsBool(L.GetGlobal("_bok")))
	}
	// Missing keys → zero + false.
	if lua.LVAsBool(L.GetGlobal("_mrok")) || lua.LVAsBool(L.GetGlobal("_msok")) || lua.LVAsBool(L.GetGlobal("_mbok")) {
		t.Fatal("a missing key reported found=true")
	}
	if float64(lua.LVAsNumber(L.GetGlobal("_mrv"))) != 0 || lua.LVAsString(L.GetGlobal("_msv")) != "" || lua.LVAsBool(L.GetGlobal("_mbv")) {
		t.Fatal("a missing key returned a non-zero value")
	}

	// Cross-check the Go api SoT: the Lua writes persisted real Storage state.
	if rv, rok := g.Storage().GetReal("r", "pi"); rv != 3.5 || !rok {
		t.Fatalf("Go SoT GetReal = (%v,%v), want (3.5,true)", rv, rok)
	}
	if sv, sok := g.Storage().GetString("t", "name"); sv != "vigil" || !sok {
		t.Fatalf("Go SoT GetString = (%q,%v), want (vigil,true)", sv, sok)
	}
	if bv, bok := g.Storage().GetBool("b", "flag"); !bv || !bok {
		t.Fatalf("Go SoT GetBool = (%v,%v), want (true,true)", bv, bok)
	}
	t.Logf("FSV #267 typed Storage: Real 3.5 / String vigil / Bool true round-trip via Lua, Go SoT confirms; missing keys (zero,false)")
}

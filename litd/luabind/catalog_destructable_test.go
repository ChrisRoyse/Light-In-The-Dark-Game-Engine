package luabind

// Game_CreateDestructable binding (#267): the generated dispatch defers it for
// its options-struct argument. SoT = the created destructable's sim state, read
// back through the already-bound Destructable_Life/SetLife verbs.

import (
	"testing"

	lua "github.com/yuin/gopher-lua"
)

func TestCreateDestructableBindingFSV(t *testing.T) {
	g, _ := confGame(t, 11)
	L := lua.NewState()
	defer L.Close()
	if err := Register(L, g); err != nil {
		t.Fatalf("Register: %v", err)
	}

	// Create from a named-field options table; then round-trip its life through
	// the bound Destructable verbs (no assumption about the create-time default —
	// SetLife(250) then Life() must read 250).
	if err := L.DoString(`
		local d = Game_CreateDestructable({type = 1, x = 100, y = 200, life = 500})
		_valid = Valid(d)
		Destructable_SetLife(d, 250)
		_life = Destructable_Life(d)
		-- a second, distinct destructable
		local d2 = Game_CreateDestructable({type = 1, x = 300, y = 400})
		_valid2 = Valid(d2)
		_same = (d == d2)
	`); err != nil {
		t.Fatalf("lua: %v", err)
	}
	if !lua.LVAsBool(L.GetGlobal("_valid")) {
		t.Fatal("created destructable is not Valid — options marshal/create failed")
	}
	if life := int(lua.LVAsNumber(L.GetGlobal("_life"))); life != 250 {
		t.Fatalf("Destructable life round-trip = %d, want 250", life)
	}
	if !lua.LVAsBool(L.GetGlobal("_valid2")) {
		t.Fatal("second destructable invalid")
	}
	if lua.LVAsBool(L.GetGlobal("_same")) {
		t.Fatal("two CreateDestructable calls returned the same handle")
	}

	// Fail-closed: omitting the required `type` raises.
	if err := L.DoString(`Game_CreateDestructable({x = 1, y = 2})`); err == nil {
		t.Fatal("CreateDestructable without `type` must raise")
	}
	t.Logf("FSV #267 CreateDestructable: two distinct valid destructables; life SetLife(250)->Life()=250; missing type raised")
}

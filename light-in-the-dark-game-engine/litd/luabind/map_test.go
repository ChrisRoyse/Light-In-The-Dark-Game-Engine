package luabind

// RegisterMap FSV (#410 Lua-exposure half): the host loads the real First Flame
// map (#174) and exposes it; a Lua world reads beacon/start placements as world
// coordinates. SoT = the parsed mapdata.Map (Go) cross-checked against what the
// Lua verbs return, with the documented cell→world conversion (cell*32+16).

import (
	"os"
	"testing"

	mapdata "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/asset/mapdata"
	lua "github.com/yuin/gopher-lua"
)

func TestRegisterMapBeaconsStartsFSV(t *testing.T) {
	m, err := mapdata.Load(os.DirFS("../.."), "data/maps/firstflame")
	if err != nil {
		t.Fatalf("Load(firstflame): %v", err)
	}
	L := lua.NewState()
	defer L.Close()
	RegisterMap(L, m)

	// --- Beacons: Lua view matches the Go SoT, in world coords. ---
	if err := L.DoString(`
		bs = Game_MapBeacons()
		_n = #bs
	`); err != nil {
		t.Fatalf("Game_MapBeacons: %v", err)
	}
	if n := int(lua.LVAsNumber(L.GetGlobal("_n"))); n != len(m.Beacons()) || n != 3 {
		t.Fatalf("Lua beacon count = %d, want %d (=3)", n, len(m.Beacons()))
	}
	for i, b := range m.Beacons() { // Go SoT, sorted by id
		wantX := float64(b.X*32 + 16)
		wantY := float64(b.Y*32 + 16)
		L.DoString(`_e = bs[` + itoa(i+1) + `]`)
		e := L.GetGlobal("_e").(*lua.LTable)
		gotID := int(lua.LVAsNumber(e.RawGetString("id")))
		gotX := float64(lua.LVAsNumber(e.RawGetString("x")))
		gotY := float64(lua.LVAsNumber(e.RawGetString("y")))
		gotOwner := int(lua.LVAsNumber(e.RawGetString("owner")))
		if gotID != int(b.ID) || gotX != wantX || gotY != wantY || gotOwner != b.Owner {
			t.Fatalf("beacon %d: Lua{id=%d,x=%v,y=%v,owner=%d} vs map{id=%d,cell(%d,%d)→world(%v,%v),owner=%d}",
				i+1, gotID, gotX, gotY, gotOwner, b.ID, b.X, b.Y, wantX, wantY, b.Owner)
		}
	}
	t.Logf("FSV #410 beacons: Lua Game_MapBeacons == map's 3 beacons in world coords (e.g. center cell(128,128)→(4112,4112), owner=-1)")

	// --- Starts: same cross-check. ---
	if err := L.DoString(`ss = Game_MapStarts(); _m = #ss`); err != nil {
		t.Fatalf("Game_MapStarts: %v", err)
	}
	if n := int(lua.LVAsNumber(L.GetGlobal("_m"))); n != 2 {
		t.Fatalf("Lua start count = %d, want 2", n)
	}
	for i, s := range m.Starts() {
		L.DoString(`_e = ss[` + itoa(i+1) + `]`)
		e := L.GetGlobal("_e").(*lua.LTable)
		gotP := int(lua.LVAsNumber(e.RawGetString("player")))
		gotX := float64(lua.LVAsNumber(e.RawGetString("x")))
		gotY := float64(lua.LVAsNumber(e.RawGetString("y")))
		if gotP != int(s.Player) || gotX != float64(s.X*32+16) || gotY != float64(s.Y*32+16) {
			t.Fatalf("start %d: Lua{p=%d,x=%v,y=%v} vs map{p=%d,cell(%d,%d)}", i+1, gotP, gotX, gotY, s.Player, s.X, s.Y)
		}
	}
	t.Logf("FSV #410 starts: Lua Game_MapStarts == map's 2 starts in world coords (P0 cell(40,128)→(1296,4112), P1→(6928,4112))")

	// --- Edge: no map loaded → fail-closed (loud error, not empty result). ---
	L2 := lua.NewState()
	defer L2.Close()
	RegisterMap(L2, nil)
	if err := L2.DoString(`Game_MapBeacons()`); err == nil {
		t.Fatal("Game_MapBeacons with nil map did not error (must fail closed)")
	}
	t.Logf("FSV #410 fail-closed: Game_MapBeacons with no map raises loudly")
}

// itoa avoids fmt in the hot DoString path; small non-negative ints only.
func itoa(n int) string {
	if n == 0 {
		return "0"
	}
	var b [20]byte
	i := len(b)
	for n > 0 {
		i--
		b[i] = byte('0' + n%10)
		n /= 10
	}
	return string(b[i:])
}

package luabind

// #410 FSV (production wire): luabind.Register — the entrypoint cmd/litd uses
// before LoadWorld — auto-exposes the game's map to Lua worlds, with NO explicit
// RegisterMap call. Proves a loaded world reads the map's placements rather than
// hardcoding them. SoT = Lua Game_MapBeacons()/Game_MapStarts() counts vs the map.

import (
	"os"
	"testing"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	mapdata "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/asset/mapdata"
	lua "github.com/yuin/gopher-lua"
)

func TestRegisterAutoWiresMapFSV(t *testing.T) {
	m, err := mapdata.Load(os.DirFS("../.."), "data/maps/firstflame")
	if err != nil {
		t.Fatalf("Load(firstflame): %v", err)
	}
	g, err := api.NewGame(api.GameOptions{Seed: 1, Map: m})
	if err != nil {
		t.Fatalf("NewGame: %v", err)
	}
	L := lua.NewState()
	defer L.Close()
	if err := Register(L, g); err != nil { // production entrypoint — no direct RegisterMap
		t.Fatalf("Register: %v", err)
	}
	if err := L.DoString(`
		bs = Game_MapBeacons(); _n = #bs
		ss = Game_MapStarts();  _ns = #ss
		_b1x = bs[1].x; _b1y = bs[1].y
	`); err != nil {
		t.Fatalf("Lua map verbs: %v", err)
	}
	if n := int(lua.LVAsNumber(L.GetGlobal("_n"))); n != 3 {
		t.Fatalf("Game_MapBeacons count = %d, want 3", n)
	}
	if ns := int(lua.LVAsNumber(L.GetGlobal("_ns"))); ns != 2 {
		t.Fatalf("Game_MapStarts count = %d, want 2", ns)
	}
	// Center beacon cell (128,128) → world (4112,4112).
	if x := float64(lua.LVAsNumber(L.GetGlobal("_b1x"))); x != 4112 {
		t.Fatalf("beacon 1 x = %v, want 4112", x)
	}
	if y := float64(lua.LVAsNumber(L.GetGlobal("_b1y"))); y != 4112 {
		t.Fatalf("beacon 1 y = %v, want 4112", y)
	}
	t.Log("FSV production wire: Register auto-exposes Game_MapBeacons (3) + Game_MapStarts (2); beacon 1 at world (4112,4112)")
}

package luabind

// End-to-end vertical-slice FSV (#410 + #169 + #174): the real First Flame map is
// loaded and exposed via RegisterMap, then worlds/firstflame-beacon captures the
// map's CENTRAL beacon at its authored cell (not a hardcoded point). Proves the
// full loop map-data → RegisterMap → Lua → mechanic. SoT = beacon state published
// to Storage (incl. the map-sourced coords) + the sim fog grid via the Go api.

import (
	"path/filepath"
	"testing"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	mapdata "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/asset/mapdata"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	lua "github.com/yuin/gopher-lua"
	"os"
)

func TestFirstFlameBeaconEndToEndFSV(t *testing.T) {
	// Load the real committed map and find its central beacon's world coords.
	m, err := mapdata.Load(os.DirFS("../.."), "data/maps/firstflame")
	if err != nil {
		t.Fatalf("Load(firstflame): %v", err)
	}
	var central mapdata.Beacon
	for _, b := range m.Beacons() {
		if b.ID == 1 {
			central = b
		}
	}
	beaconWorld := api.Vec2{X: float64(central.X*32 + 16), Y: float64(central.Y*32 + 16)}

	g, err := api.NewGame(api.GameOptions{MaxUnits: 16, Seed: 5})
	if err != nil {
		t.Fatalf("NewGame: %v", err)
	}
	if err := g.DefineUnits([]data.Unit{
		{ID: "hfoo", Life: 100, MoveSpeedPerTick: 8 * fixed.One, TurnRatePerTick: 65535, CollisionSize: 16},
	}); err != nil {
		t.Fatalf("DefineUnits: %v", err)
	}
	L := lua.NewState()
	defer L.Close()
	if err := Register(L, g); err != nil {
		t.Fatalf("Register: %v", err)
	}
	RegisterMap(L, m) // host exposes the loaded map to the world

	reg := NewChunkRegistry()
	defer reg.Close()

	// Place a player-1 unit AT the map's central beacon, then load the world.
	if !g.CreateUnit(g.Player(1), g.UnitType("hfoo"), beaconWorld, api.Deg(0)).Valid() {
		t.Fatal("capturing unit invalid")
	}
	if _, err := LoadWorld(L, reg, filepath.Join("..", "..", "worlds", "firstflame-beacon")); err != nil {
		t.Fatalf("LoadWorld(firstflame-beacon): %v", err)
	}

	// The world must publish the MAP's beacon coords (proving it read them, not a literal).
	bx, _ := g.Storage().GetInt("beacon", "x")
	by, _ := g.Storage().GetInt("beacon", "y")
	if bx != int(beaconWorld.X) || by != int(beaconWorld.Y) {
		t.Fatalf("world beacon coords (%d,%d) != map central beacon (%v,%v)", bx, by, beaconWorld.X, beaconWorld.Y)
	}

	// Capture: advance past the 2s threshold; SoT = beacon lit for player 1 + vision.
	g.Advance(70)
	lit, _ := g.Storage().GetInt("beacon", "lit")
	owner, _ := g.Storage().GetInt("beacon", "owner")
	if lit != 1 || owner != 1 {
		t.Fatalf("map beacon not captured: lit=%d owner=%d, want 1/1", lit, owner)
	}
	if fs := g.FogStateAt(g.Player(1), beaconWorld); fs != api.FogVisible {
		t.Fatalf("captured map beacon did not reveal: FogStateAt=%d, want Visible(%d)", int(fs), int(api.FogVisible))
	}
	t.Logf("FSV #410+#169+#174 end-to-end: captured the MAP's central beacon at world (%v,%v) [cell (%d,%d)] → lit=1 owner=1, vision stamped",
		beaconWorld.X, beaconWorld.Y, central.X, central.Y)
}

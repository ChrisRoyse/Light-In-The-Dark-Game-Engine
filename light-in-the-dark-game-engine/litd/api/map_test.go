package litd

// #410 FSV (Go side): loading the real First Flame map into a Game seeds the
// players' start locations and exposes the map's placements in world coordinates.
// SoT = the parsed mapdata.Map vs the Game's StartLocation/Player.StartLocation/
// MapStarts/MapBeacons. Cells convert at 32 units +16 center (engine convention).

import (
	"os"
	"testing"

	mapdata "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/asset/mapdata"
)

func loadFirstFlame(t *testing.T) *mapdata.Map {
	t.Helper()
	m, err := mapdata.Load(os.DirFS("../.."), "data/maps/firstflame")
	if err != nil {
		t.Fatalf("Load(firstflame): %v", err)
	}
	return m
}

func cell(x, y int) Vec2 { return Vec2{X: float64(x*32 + 16), Y: float64(y*32 + 16)} }

func TestGameSeedsStartsFromMapFSV(t *testing.T) {
	m := loadFirstFlame(t)
	g, err := NewGame(GameOptions{Seed: 1, Map: m})
	if err != nil {
		t.Fatalf("NewGame: %v", err)
	}

	// Go SoT: the parsed map's starts, converted to world centers.
	want := map[uint8]Vec2{}
	for _, s := range m.Starts() {
		want[s.Player] = cell(s.X, s.Y)
	}
	if len(want) != 2 {
		t.Fatalf("firstflame has %d starts, expected 2", len(want))
	}

	// StartLocation table (indexed) AND the player's sim start point both seeded.
	for p, wv := range want {
		if got := g.StartLocation(int(p)); got != wv {
			t.Fatalf("StartLocation(%d) = %v, want %v", p, got, wv)
		}
		if got := g.Player(int(p)).StartLocation(); got != wv {
			t.Fatalf("Player(%d).StartLocation() = %v, want %v (sim not seeded)", p, got, wv)
		}
	}
	// Concrete check against the known firstflame cells (40,128)/(216,128).
	if g.StartLocation(0) != (Vec2{1296, 4112}) || g.StartLocation(1) != (Vec2{6928, 4112}) {
		t.Fatalf("start coords = %v / %v, want {1296 4112} / {6928 4112}", g.StartLocation(0), g.StartLocation(1))
	}
	t.Logf("FSV starts: player 0 → %v, player 1 → %v (seeded into both the start table and the sim)", g.StartLocation(0), g.StartLocation(1))
}

func TestGameMapBeaconsAndStartsFSV(t *testing.T) {
	m := loadFirstFlame(t)
	g, _ := NewGame(GameOptions{Seed: 1, Map: m})

	// MapStarts mirrors the parsed starts.
	gs := g.MapStarts()
	if len(gs) != len(m.Starts()) {
		t.Fatalf("MapStarts len %d, want %d", len(gs), len(m.Starts()))
	}
	for i, s := range m.Starts() {
		if gs[i].Player != s.Player || gs[i].Pos != cell(s.X, s.Y) {
			t.Fatalf("MapStarts[%d] = %+v, want player %d at %v", i, gs[i], s.Player, cell(s.X, s.Y))
		}
	}

	// MapBeacons mirrors the parsed beacons (3 neutral control points).
	gb := g.MapBeacons()
	if len(gb) != 3 {
		t.Fatalf("MapBeacons len %d, want 3", len(gb))
	}
	for i, b := range m.Beacons() {
		if gb[i].ID != b.ID || gb[i].Pos != cell(b.X, b.Y) || gb[i].Owner != b.Owner {
			t.Fatalf("MapBeacons[%d] = %+v, want id %d at %v owner %d", i, gb[i], b.ID, cell(b.X, b.Y), b.Owner)
		}
	}
	// Concrete: center beacon at cell (128,128) → (4112,4112), neutral.
	if gb[0].Pos != (Vec2{4112, 4112}) || gb[0].Owner != mapdata.BeaconNeutral {
		t.Fatalf("beacon 1 = %+v, want {4112 4112} neutral(-1)", gb[0])
	}
	t.Logf("FSV beacons: %d control points; #1 at %v owner %d (neutral)", len(gb), gb[0].Pos, gb[0].Owner)
}

func TestGameNoMapEmptyPlacementsFSV(t *testing.T) {
	g, _ := NewGame(GameOptions{Seed: 1}) // no Map
	if g.MapData() != nil {
		t.Fatal("MapData() non-nil without a map")
	}
	if g.MapStarts() != nil || g.MapBeacons() != nil {
		t.Fatal("MapStarts/MapBeacons should be nil without a map")
	}
	if g.StartLocation(0) != (Vec2{}) {
		t.Fatalf("StartLocation(0) = %v without a map, want zero", g.StartLocation(0))
	}
	t.Log("FSV mapless: MapData nil, placements empty, start locations zero")
}

package mapdata

// First Flame skirmish map (#174) data-layer FSV. SoT = the parsed Map structure
// read back from data/maps/firstflame via Load — dimensions, start locations, and
// beacon placements must match what terrain.toml authored, and the load must be
// deterministic (stable fingerprint). This is the loadable map DATA the First
// Flame mechanics (#169/#170/#171/#172/#200/#201 prototypes) run on; the 3D
// terrain look + screenshots are the render/asset half of #174.

import (
	"os"
	"testing"
)

func TestLoadFirstFlameMapFSV(t *testing.T) {
	root := os.DirFS("../../..")
	m, err := Load(root, "data/maps/firstflame")
	if err != nil {
		t.Fatalf("Load(firstflame): %v", err)
	}

	// Dimensions.
	if m.Width != 64 || m.Height != 64 {
		t.Fatalf("dims = %dx%d, want 64x64", m.Width, m.Height)
	}
	if m.PathingWidth != 256 || m.PathingHeight != 256 {
		t.Fatalf("pathing dims = %dx%d, want 256x256", m.PathingWidth, m.PathingHeight)
	}
	if m.Biome != "ashen-veil" {
		t.Fatalf("biome = %q, want ashen-veil", m.Biome)
	}

	// Start locations: two players, symmetric, on buildable ground.
	starts := m.Starts()
	if len(starts) != 2 {
		t.Fatalf("starts = %d, want 2", len(starts))
	}
	wantStart := map[uint8][2]int{0: {40, 128}, 1: {216, 128}}
	for _, s := range starts {
		w, ok := wantStart[s.Player]
		if !ok {
			t.Fatalf("unexpected start player %d", s.Player)
		}
		if s.X != w[0] || s.Y != w[1] {
			t.Fatalf("start player %d at (%d,%d), want (%d,%d)", s.Player, s.X, s.Y, w[0], w[1])
		}
		// Each start must actually be buildable ground (the loader enforces this,
		// re-assert as SoT).
		flags, ok := m.PathingAt(s.X, s.Y)
		if !ok || flags&PathBuildable == 0 || flags&PathWater != 0 {
			t.Fatalf("start player %d cell (%d,%d) not buildable ground (flags=%d ok=%v)", s.Player, s.X, s.Y, flags, ok)
		}
	}

	// Beacons: three neutral control points at the authored cells.
	beacons := m.Beacons()
	if len(beacons) != 3 {
		t.Fatalf("beacons = %d, want 3", len(beacons))
	}
	wantBeacon := map[uint32][2]int{1: {128, 128}, 2: {88, 88}, 3: {168, 168}}
	for _, b := range beacons {
		w, ok := wantBeacon[b.ID]
		if !ok {
			t.Fatalf("unexpected beacon id %d", b.ID)
		}
		if b.X != w[0] || b.Y != w[1] {
			t.Fatalf("beacon %d at (%d,%d), want (%d,%d)", b.ID, b.X, b.Y, w[0], w[1])
		}
		if b.Owner != BeaconNeutral {
			t.Fatalf("beacon %d owner = %d, want neutral (%d)", b.ID, b.Owner, BeaconNeutral)
		}
	}

	// Deterministic load: same bytes → same fingerprint.
	again, err := Load(root, "data/maps/firstflame")
	if err != nil {
		t.Fatalf("Load(firstflame) #2: %v", err)
	}
	if m.Fingerprint != again.Fingerprint {
		t.Fatalf("non-deterministic fingerprint: %#x vs %#x", m.Fingerprint, again.Fingerprint)
	}
	t.Logf("FSV #174 map: 64x64 ashen-veil, 2 symmetric starts (P0@40,128 P1@216,128), 3 neutral beacons (center+2 flanks), fingerprint=%#x stable", m.Fingerprint)
}

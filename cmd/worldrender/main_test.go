package main

// GL-free FSV for the worldrender harness logic (#490). The render loop itself
// needs a GL context (exercised manually via the binary, screenshots read), but
// the parts most likely to regress — beat parsing and the sim->world auto-fit
// transform — are pure and load a REAL world through worldhost, so they verify
// against the live sim's actual unit positions (SoT), not a mock.

import (
	"path/filepath"
	"testing"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/worldhost"
)

func TestParseBeatsSortsAndFilters(t *testing.T) {
	got := parseBeats(" 160, 20 ,80,, 20 ")
	want := []int{20, 20, 80, 160} // sorted ascending; blanks dropped; dups kept
	if len(got) != len(want) {
		t.Fatalf("parseBeats len = %d (%v), want %d (%v)", len(got), got, len(want), want)
	}
	for i := range want {
		if got[i] != want[i] {
			t.Fatalf("parseBeats[%d] = %d, want %d (full %v)", i, got[i], want[i], got)
		}
	}
	t.Logf("FSV parseBeats: %v", got)
}

// TestAutoFitTransformMapsUnitsFSV loads the First Flame slice and proves the
// auto-fit transform centers on the unit cloud and maps the leftmost/rightmost
// sim units to opposite sides of world origin — the exact layout the beat-20
// screenshot shows (2 P1 boxes left of center, 1 P2 box right).
func TestAutoFitTransformMapsUnitsFSV(t *testing.T) {
	world := filepath.Join("..", "..", "worlds", "firstflame-slice")
	host, err := worldhost.Load(world, 1, 50_000_000)
	if err != nil {
		t.Fatalf("worldhost.Load: %v", err)
	}
	defer host.Close()
	h := &harness{g: host.Game}

	// SoT BEFORE: read the live sim unit positions the transform will fit.
	us := h.g.UnitsInRange(api.Vec2{}, 1e9, nil)
	if len(us) < 2 {
		t.Fatalf("slice should spawn >=2 units at load, got %d", len(us))
	}
	var minX, maxX float64 = us[0].Position().X, us[0].Position().X
	for _, u := range us {
		minX = min(minX, u.Position().X)
		maxX = max(maxX, u.Position().X)
	}
	t.Logf("FSV BEFORE: %d units, simX in [%.0f,%.0f]", len(us), minX, maxX)

	h.computeFit()
	t.Logf("FSV transform: center=(%.1f,%.1f) scale=%.5f", h.cx, h.cy, h.scale)

	// Center must be the X-midpoint of the cloud.
	if wantCx := (minX + maxX) / 2; h.cx != wantCx {
		t.Fatalf("fit center X = %.1f, want %.1f", h.cx, wantCx)
	}
	// Scale floors the half-span at minSimHalf, so a tight cloud isn't zoomed in.
	if want := fitHalf / minSimHalf; h.scale != want {
		t.Fatalf("fit scale = %.5f, want %.5f (floored)", h.scale, want)
	}
	// AFTER: leftmost sim unit maps left of origin, rightmost right of origin,
	// and everything lands within the ground half-plane.
	for _, u := range us {
		x, _ := h.simToWorld(u.Position())
		if x < -groundSize/2 || x > groundSize/2 {
			t.Fatalf("unit simX=%.0f maps to world x=%.2f, off the ground plane", u.Position().X, x)
		}
	}
	leftX, _ := h.simToWorld(api.Vec2{X: minX, Y: h.cy})
	rightX, _ := h.simToWorld(api.Vec2{X: maxX, Y: h.cy})
	if !(leftX < 0 && rightX > 0) {
		t.Fatalf("leftmost world x=%.2f (want <0), rightmost x=%.2f (want >0)", leftX, rightX)
	}
	t.Logf("FSV AFTER: leftmost->%.2f, rightmost->%.2f (straddle origin, on-plane)", leftX, rightX)
}

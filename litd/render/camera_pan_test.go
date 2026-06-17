package render

import (
	"testing"

	"github.com/g3n/engine/math32"
)

const panEps = 1e-4

func TestEdgePanStraightFSV(t *testing.T) {
	p := NewPanController(PanConfig{EdgeBandPx: 8, PanSpeed: 2})
	const w, h = 800, 600
	const zoom, dt = 10, 0.5
	want := float32(2 * zoom * dt) // 10 world units
	cases := []struct {
		name           string
		cx, cy         int
		wantDX, wantDZ float32
	}{
		{"left", 2, 300, -want, 0},
		{"right", w - 1, 300, want, 0},
		{"top", 400, 1, 0, -want},
		{"bottom", 400, h - 1, 0, want},
		{"center", 400, 300, 0, 0},
	}
	for _, c := range cases {
		dx, dz := p.EdgePanDelta(c.cx, c.cy, w, h, zoom, dt, false)
		t.Logf("FSV edge %-7s cursor=(%d,%d) delta=(%.3f,%.3f) want=(%.3f,%.3f)", c.name, c.cx, c.cy, dx, dz, c.wantDX, c.wantDZ)
		if math32.Abs(dx-c.wantDX) > panEps || math32.Abs(dz-c.wantDZ) > panEps {
			t.Fatalf("%s: delta=(%.3f,%.3f) want (%.3f,%.3f)", c.name, dx, dz, c.wantDX, c.wantDZ)
		}
	}
}

// TestEdgePanCornerNormalizedFSV — a corner pans at the SAME speed as a straight
// edge (edge case 4): |delta| equal, not √2 larger.
func TestEdgePanCornerNormalizedFSV(t *testing.T) {
	p := NewPanController(PanConfig{EdgeBandPx: 8, PanSpeed: 3})
	const w, h = 1000, 800
	const zoom, dt = 4, 1.0
	straightDX, _ := p.EdgePanDelta(2, 400, w, h, zoom, dt, false) // left edge
	straightMag := math32.Abs(straightDX)
	cdx, cdz := p.EdgePanDelta(2, 2, w, h, zoom, dt, false) // top-left corner
	cornerMag := math32.Sqrt(cdx*cdx + cdz*cdz)
	t.Logf("FSV corner mag=%.4f straight mag=%.4f (delta=%.4f,%.4f)", cornerMag, straightMag, cdx, cdz)
	if math32.Abs(cornerMag-straightMag) > panEps {
		t.Fatalf("corner speed %.4f != straight speed %.4f", cornerMag, straightMag)
	}
	if cdx >= 0 || cdz >= 0 {
		t.Fatalf("top-left corner must pan −X,−Z, got (%.3f,%.3f)", cdx, cdz)
	}
}

func TestEdgePanSuppressedFSV(t *testing.T) {
	p := NewPanController(DefaultPanConfig())
	// Cursor squarely in the left band, but suppressed (active marquee).
	dx, dz := p.EdgePanDelta(1, 300, 800, 600, 10, 0.5, true)
	t.Logf("FSV suppressed delta=(%.3f,%.3f) want (0,0)", dx, dz)
	if dx != 0 || dz != 0 {
		t.Fatalf("suppressed edge-pan must be zero, got (%.3f,%.3f)", dx, dz)
	}
}

func TestArrowPanMatchesEdgeFSV(t *testing.T) {
	p := NewPanController(PanConfig{PanSpeed: 5})
	const zoom, dt = 6, 0.25
	ex, ez := p.EdgePanDelta(1, 300, 800, 600, zoom, dt, false) // left edge
	ax, az := p.ArrowPanDelta(true, false, false, false, zoom, dt)
	t.Logf("FSV arrow-left=(%.3f,%.3f) edge-left=(%.3f,%.3f)", ax, az, ex, ez)
	if math32.Abs(ax-ex) > panEps || math32.Abs(az-ez) > panEps {
		t.Fatalf("arrow-left %v,%v != edge-left %v,%v", ax, az, ex, ez)
	}
}

// TestMiddleDragKeepsPointUnderCursorFSV — the 1:1 grab invariant (edge case 2).
// Models the locked camera's affine screen→ground map worldUnderCursor =
// anchor.XZ + ndc*scale. After grabbing a point and moving the cursor, applying
// the drag delta must put the grabbed point exactly back under the cursor.
func TestMiddleDragKeepsPointUnderCursorFSV(t *testing.T) {
	const scale = 250 // world units per ndc unit (zoom-derived, fixed here)
	anchor := math32.Vector3{X: 1000, Z: -500}
	worldUnderCursor := func(a math32.Vector3, ndcX, ndcZ float32) math32.Vector3 {
		return math32.Vector3{X: a.X + ndcX*scale, Z: a.Z + ndcZ*scale}
	}
	p := NewPanController(DefaultPanConfig())

	// Button-down at cursor ndc (0.3,-0.2): pick the grabbed world point.
	grab := worldUnderCursor(anchor, 0.3, -0.2)
	p.BeginDrag(grab)
	if !p.Dragging() {
		t.Fatal("BeginDrag should set dragging")
	}

	// Cursor moves to ndc (-0.4, 0.5). Fresh pick at the CURRENT anchor:
	nowNDCx, nowNDCz := float32(-0.4), float32(0.5)
	now := worldUnderCursor(anchor, nowNDCx, nowNDCz)
	dx, dz := p.DragAnchorDelta(now)
	anchor.X += dx
	anchor.Z += dz

	// After applying, the grabbed point must sit under the new cursor position.
	check := worldUnderCursor(anchor, nowNDCx, nowNDCz)
	errX := math32.Abs(check.X - grab.X)
	errZ := math32.Abs(check.Z - grab.Z)
	t.Logf("FSV drag grab=(%.2f,%.2f) under-cursor-after=(%.2f,%.2f) err=(%.4f,%.4f)", grab.X, grab.Z, check.X, check.Z, errX, errZ)
	if errX > 1 || errZ > 1 {
		t.Fatalf("grabbed point drifted: err=(%.4f,%.4f), want < 1 world unit", errX, errZ)
	}
	p.EndDrag()
	if p.Dragging() {
		t.Fatal("EndDrag should clear dragging")
	}
	if dx, dz := p.DragAnchorDelta(now); dx != 0 || dz != 0 {
		t.Fatalf("DragAnchorDelta after EndDrag must be zero, got (%.3f,%.3f)", dx, dz)
	}
}

// TestPanClampToBoundsFSV — edge case 1: anchor cannot scroll into the void;
// pushing past the boundary leaves it pinned at the clamp rect.
func TestPanClampToBoundsFSV(t *testing.T) {
	p := NewPanController(PanConfig{Margin: 100})
	p.SetMapBounds(-1000, -2000, 1000, 2000) // clamp rect = ±1100 X, ±2100 Z
	in := p.ClampAnchor(math32.Vector3{X: 500, Z: 500})
	if in.X != 500 || in.Z != 500 {
		t.Fatalf("in-bounds anchor altered: %v", in)
	}
	out := p.ClampAnchor(math32.Vector3{X: 99999, Z: -99999})
	t.Logf("FSV clamp far anchor -> (%.1f,%.1f) want (1100,-2100)", out.X, out.Z)
	if out.X != 1100 || out.Z != -2100 {
		t.Fatalf("clamp wrong: %v want (1100,-2100)", out)
	}
	// Pushing further past the boundary is a no-op (before==after).
	again := p.ClampAnchor(out)
	if again != out {
		t.Fatalf("re-clamp at boundary changed anchor: %v -> %v", out, again)
	}
}

// TestPanRealCameraAccumulationFSV — apply edge-pan deltas to a real RTSCamera
// anchor over N frames and clamp to a real 128² map's bounds. SoT: the anchor
// delta equals k*zoom*N*dt until it pins at the clamp rect.
func TestPanRealCameraAccumulationFSV(t *testing.T) {
	cam := NewRTSCamera(DefaultRTSCameraConfig(16.0 / 9.0))
	const tiles, cell = 128, 128
	const half = tiles * cell / 2 // 8192
	p := NewPanController(PanConfig{PanSpeed: 4, Margin: 0})
	p.SetMapBounds(-half, -half, half, half)

	anchor := math32.Vector3{}
	const zoom, dt = 50, 0.1
	const frames = 30
	per := float32(4 * zoom * dt) // 20 units/frame, +X (right edge)
	for i := 0; i < frames; i++ {
		dx, dz := p.EdgePanDelta(1599, 540, 1600, 1080, zoom, dt, false) // right edge
		anchor.X += dx
		anchor.Z += dz
		anchor = p.ClampAnchor(anchor)
		cam.SetAnchor(anchor)
	}
	wantUnclamped := per * frames // 600, within ±8192 → not clamped
	t.Logf("FSV real-cam pan anchor.X=%.2f want=%.2f (clamp ±%d)", anchor.X, wantUnclamped, half)
	if math32.Abs(anchor.X-wantUnclamped) > 1e-2 {
		t.Fatalf("accumulated pan X=%.2f want %.2f", anchor.X, wantUnclamped)
	}
	if anchor.Z != 0 {
		t.Fatalf("right-edge pan moved Z: %.2f", anchor.Z)
	}

	// Now pan hard into +X until it pins at the clamp rect.
	for i := 0; i < 100000; i++ {
		dx, _ := p.EdgePanDelta(1599, 540, 1600, 1080, zoom, dt, false)
		anchor.X += dx
		anchor = p.ClampAnchor(anchor)
	}
	t.Logf("FSV real-cam pan pinned anchor.X=%.2f want=%d", anchor.X, half)
	if anchor.X != float32(half) {
		t.Fatalf("pan did not pin at +X bound: %.2f want %d", anchor.X, half)
	}
}

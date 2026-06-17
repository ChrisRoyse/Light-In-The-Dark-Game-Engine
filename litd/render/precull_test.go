package render

import "testing"

// squareFootprint builds a footprint that is a square of half-extent h centred
// at the origin on the XZ plane (Y unused by the culler).
func squareFootprint(h float32) RTSCameraFootprint {
	return RTSCameraFootprint{
		Projection: "orthographic",
		Corners: [4]Vec3Snapshot{
			{X: -h, Z: -h}, {X: h, Z: -h}, {X: h, Z: h}, {X: -h, Z: h},
		},
		MinX: -h, MaxX: h, MinZ: -h, MaxZ: h, Area: 4 * h * h, OK: true,
	}
}

func TestPrecullInsideOutsideFSV(t *testing.T) {
	var c GroundFootprintCuller
	c.SetFootprint(squareFootprint(50), 0)
	cases := []struct {
		x, z float32
		want bool
	}{
		{0, 0, true},        // centre
		{49, 49, true},      // inside corner
		{51, 0, false},      // just outside, no margin
		{1000, 1000, false}, // far away
		{-50, -50, true},    // on a corner (boundary inclusive)
	}
	for _, tc := range cases {
		got := c.Contains(tc.x, tc.z)
		t.Logf("FSV contains(%.0f,%.0f)=%v want=%v", tc.x, tc.z, got, tc.want)
		if got != tc.want {
			t.Fatalf("contains(%.0f,%.0f)=%v want %v", tc.x, tc.z, got, tc.want)
		}
	}
}

func TestPrecullMarginFSV(t *testing.T) {
	var c GroundFootprintCuller
	c.SetFootprint(squareFootprint(50), 10) // 10-unit margin
	// 5 units outside the right edge → within margin → kept.
	if !c.Contains(55, 0) {
		t.Fatalf("point 5 outside with margin 10 must be kept")
	}
	// 15 units outside → beyond margin → culled.
	if c.Contains(65, 0) {
		t.Fatalf("point 15 outside with margin 10 must be culled")
	}
	// Exactly on the inflated boundary (60) → kept (dist == -margin).
	if !c.Contains(60, 0) {
		t.Fatalf("point exactly at margin boundary must be kept")
	}
	t.Logf("FSV margin=10: 55→keep, 60→keep(boundary), 65→cull")
}

func TestPrecullCullFractionFSV(t *testing.T) {
	// Real camera footprint over a 128×128-tile map (128 world units/tile).
	cam := NewRTSCamera(DefaultRTSCameraConfig(16.0 / 9.0)) // default anchor = origin
	fp, ok := cam.GroundFootprint()
	if !ok {
		t.Fatal("no ground footprint")
	}
	var c GroundFootprintCuller
	c.SetFootprint(fp, 256) // ~2 tiles of vertical margin
	const tiles = 128
	const cell = 128
	const half = tiles * cell / 2
	xs := make([]float32, 0, tiles*tiles)
	zs := make([]float32, 0, tiles*tiles)
	for r := 0; r < tiles; r++ {
		for col := 0; col < tiles; col++ {
			xs = append(xs, float32(col*cell-half)+cell/2)
			zs = append(zs, float32(r*cell-half)+cell/2)
		}
	}
	vis, cull := c.Cull(xs, zs)
	total := tiles * tiles
	frac := float64(len(vis)) / float64(total)
	t.Logf("FSV cull-fraction visible=%d culled=%d total=%d (%.2f%% visible)", len(vis), len(cull), total, frac*100)
	if len(vis)+len(cull) != total {
		t.Fatalf("partition lost entities: %d+%d != %d", len(vis), len(cull), total)
	}
	// A locked RTS camera over a 128² map sees a small slice — well under 10%.
	if frac > 0.10 {
		t.Fatalf("visible fraction %.2f%% too high — footprint cull not tight", frac*100)
	}
	if len(vis) == 0 {
		t.Fatalf("camera centred on map should see some cells")
	}
}

func TestPrecullZeroAllocFSV(t *testing.T) {
	var c GroundFootprintCuller
	c.SetFootprint(squareFootprint(2000), 100)
	const n = 1000
	xs := make([]float32, n)
	zs := make([]float32, n)
	for i := 0; i < n; i++ {
		xs[i] = float32(i*8 - 4000)
		zs[i] = float32((i%50)*100 - 2500)
	}
	c.Reserve(n)
	c.Cull(xs, zs) // warm
	allocs := testing.AllocsPerRun(200, func() {
		c.Cull(xs, zs)
	})
	vis, cull := c.Cull(xs, zs)
	t.Logf("FSV 1000-entity cull allocs/op=%v visible=%d culled=%d", allocs, len(vis), len(cull))
	if allocs != 0 {
		t.Fatalf("pre-cull allocates %v/op, want 0", allocs)
	}
}

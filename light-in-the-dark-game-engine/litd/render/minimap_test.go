package render

import "testing"

// 128² tile map of 128 units/cell → world rect [-8192, 8192]².
const mmMin, mmMax = -8192, 8192

func newTestMinimap() *Minimap { return NewMinimap(mmMin, mmMin, mmMax, mmMax) }

// TestMinimapMappingRoundTripFSV — pixel→world→pixel is exact; world→pixel hits
// the expected pixel for known points (X+X=Y).
func TestMinimapMappingRoundTripFSV(t *testing.T) {
	m := newTestMinimap()
	// Known points: NW corner → (0,0); center → (128,128); SE → (255,255).
	cases := []struct {
		x, z           float32
		wantPX, wantPY int
	}{
		{mmMin, mmMin, 0, 0},
		{0, 0, 128, 128},
		{mmMax - 1, mmMax - 1, 255, 255},
	}
	for _, c := range cases {
		px, py := m.WorldToPixel(c.x, c.z)
		t.Logf("FSV world(%.0f,%.0f)->px(%d,%d) want(%d,%d)", c.x, c.z, px, py, c.wantPX, c.wantPY)
		if px != c.wantPX || py != c.wantPY {
			t.Fatalf("world(%.0f,%.0f)->(%d,%d) want (%d,%d)", c.x, c.z, px, py, c.wantPX, c.wantPY)
		}
	}
	// pixel→world→pixel round-trips exactly for every... sample a grid.
	for py := 0; py < 256; py += 17 {
		for px := 0; px < 256; px += 17 {
			wx, wz := m.PixelToWorld(px, py)
			rpx, rpy := m.WorldToPixel(wx, wz)
			if rpx != px || rpy != py {
				t.Fatalf("round-trip px(%d,%d)->world(%.2f,%.2f)->px(%d,%d)", px, py, wx, wz, rpx, rpy)
			}
		}
	}
	t.Logf("FSV pixel→world→pixel round-trip exact over sampled grid")
}

// TestMinimapBlipPlotFSV — a blip writes exactly its sizePx block at the mapped
// pixel in the given color; non-visible blips write nothing (fog respect).
func TestMinimapBlipPlotFSV(t *testing.T) {
	m := newTestMinimap()
	m.Clear()
	red := RGBA{1, 0, 0, 1}
	// A 2×2 unit blip at world center → pixels (127..128, 127..128) (half=1).
	m.PlotBlip(0, 0, 2, red, true)
	lit := 0
	for py := 0; py < 256; py++ {
		for px := 0; px < 256; px++ {
			if m.At(px, py).A > 0 {
				lit++
			}
		}
	}
	t.Logf("FSV 2x2 blip lit pixels=%d (want 4)", lit)
	if lit != 4 {
		t.Fatalf("2x2 blip lit %d pixels, want 4", lit)
	}
	if got := m.At(127, 127); got != red {
		t.Fatalf("blip color at (127,127)=%+v want red", got)
	}

	// Non-visible blip writes nothing.
	m.Clear()
	m.PlotBlip(0, 0, 2, red, false)
	for _, b := range m.buf {
		if b != 0 {
			t.Fatal("non-visible blip wrote to the buffer (fog not respected)")
		}
	}
	t.Logf("FSV non-visible blip leaves buffer empty")
}

// TestMinimapBlipEdgeClampFSV — a blip at the map corner clamps its block to
// the buffer, no out-of-bounds write/panic.
func TestMinimapBlipEdgeClampFSV(t *testing.T) {
	m := newTestMinimap()
	m.Clear()
	// 4×4 building blip at the very NW corner: only the in-bounds quadrant lands.
	m.PlotBlip(mmMin, mmMin, 4, RGBA{0, 0, 1, 1}, true)
	lit := 0
	for py := 0; py < 256; py++ {
		for px := 0; px < 256; px++ {
			if m.At(px, py).A > 0 {
				lit++
			}
		}
	}
	// center pixel (0,0), half=2 → block px,py in [-2..1]; in-bounds [0..1]² = 4.
	t.Logf("FSV 4x4 corner blip lit=%d (clamped, want 4)", lit)
	if lit != 4 {
		t.Fatalf("corner blip lit %d, want 4 (clamped)", lit)
	}
}

func TestMinimapClearFSV(t *testing.T) {
	m := newTestMinimap()
	m.PlotBlip(0, 0, 8, RGBA{1, 1, 1, 1}, true)
	m.Clear()
	for i, b := range m.buf {
		if b != 0 {
			t.Fatalf("Clear left byte %d = %d", i, b)
		}
	}
	t.Logf("FSV Clear zeroes the buffer")
}

// TestMinimapFrustumLoopFSV — the viewport reduces to 5 verts with the loop
// closed (vert[4]==vert[0]) and corners at the mapped pixels.
func TestMinimapFrustumLoopFSV(t *testing.T) {
	m := newTestMinimap()
	fp := RTSCameraFootprint{
		Corners: [4]Vec3Snapshot{
			{X: -4096, Z: -4096}, {X: 4096, Z: -4096}, {X: 4096, Z: 4096}, {X: -4096, Z: 4096},
		},
		OK: true,
	}
	loop := m.Frustum(fp)
	for i := 0; i < 4; i++ {
		wpx, wpy := m.WorldToPixel(fp.Corners[i].X, fp.Corners[i].Z)
		if loop[i].X != float32(wpx) || loop[i].Y != float32(wpy) {
			t.Fatalf("frustum vert %d = %v, want pixel (%d,%d)", i, loop[i], wpx, wpy)
		}
	}
	if loop[4] != loop[0] {
		t.Fatalf("frustum loop not closed: v4=%v v0=%v", loop[4], loop[0])
	}
	t.Logf("FSV frustum loop closed, corners mapped: v0=%v v2=%v", loop[0], loop[2])
}

func TestMinimapZeroAllocFSV(t *testing.T) {
	m := newTestMinimap()
	blips := 500
	allocs := testing.AllocsPerRun(100, func() {
		m.Clear()
		for i := 0; i < blips; i++ {
			fx := float32((i%64)*256 - 8192)
			fz := float32((i/64)*256 - 8192)
			m.PlotBlip(fx, fz, 2, RGBA{0, 1, 0, 1}, true)
		}
	})
	t.Logf("FSV clear+%d blips allocs/op=%v", blips, allocs)
	if allocs != 0 {
		t.Fatalf("minimap raster allocates %v/op, want 0", allocs)
	}
}

// TestMinimapTexturePersistenceFSV — texture created once, buffer reused.
func TestMinimapTexturePersistenceFSV(t *testing.T) {
	m := newTestMinimap()
	t0 := m.EnsureTexture()
	t1 := m.EnsureTexture()
	if t0 != t1 {
		t.Fatal("minimap texture recreated")
	}
	bufPtr := &m.buf[0]
	for i := 0; i < 3; i++ {
		m.Clear()
		m.PlotBlip(0, 0, 3, RGBA{1, 1, 1, 1}, true)
		m.Upload()
	}
	if &m.buf[0] != bufPtr {
		t.Fatal("minimap buffer reallocated")
	}
	if m.Uploads() != 3 {
		t.Fatalf("uploads=%d want 3", m.Uploads())
	}
	t.Logf("FSV texture single-instance, buffer stable, uploads=%d", m.Uploads())
}

package render

import "testing"

// fakeFogGrid is a synthetic FogGridSource with known per-player cell states.
// It is test INPUT, not a mock that hides failure: the verdict is always the
// resulting FogTexture buffer bytes, read back and asserted against the known
// luminance the input demands (the X+X=Y discipline).
type fakeFogGrid struct {
	size  int32
	state [fogMaxPlayers][]uint8
}

func newFakeFogGrid() *fakeFogGrid {
	g := &fakeFogGrid{size: FogTexSize}
	for p := range g.state {
		g.state[p] = make([]uint8, int(g.size*g.size))
	}
	return g
}

func (g *fakeFogGrid) set(player uint8, x, y int32, st uint8) {
	g.state[player][y*g.size+x] = st
}

func (g *fakeFogGrid) FogStateAt(player uint8, x, y int32) uint8 {
	if int(player) >= fogMaxPlayers || x < 0 || y < 0 || x >= g.size || y >= g.size {
		return fogStateHidden
	}
	return g.state[player][y*g.size+x]
}

// TestFogLuminanceMappingFSV — known grid state at known cells must produce the
// exact luminance bytes in the buffer. blend=1 (instant) isolates the mapping.
func TestFogLuminanceMappingFSV(t *testing.T) {
	g := newFakeFogGrid()
	g.set(0, 10, 10, fogStateHidden)
	g.set(0, 20, 20, fogStateExplored)
	g.set(0, 30, 30, fogStateVisible)
	f := NewFogTexture(1)
	f.Update(g, 1<<0)
	cases := []struct {
		x, y int
		want uint8
	}{
		{10, 10, FogHiddenLum},   // 0
		{20, 20, FogExploredLum}, // 102
		{30, 30, FogVisibleLum},  // 255
		{0, 0, FogHiddenLum},     // untouched cell stays hidden
	}
	for _, c := range cases {
		got := f.At(c.x, c.y)
		t.Logf("FSV cell(%d,%d) state→lum got=%d want=%d", c.x, c.y, got, c.want)
		if got != c.want {
			t.Fatalf("cell(%d,%d) lum=%d want %d", c.x, c.y, got, c.want)
		}
	}
}

// TestFogAllyUnionFSV — a cell visible to an ally but hidden to the local
// player must read visible when the mask includes the ally, hidden when it
// does not. Proves the per-cell max union.
func TestFogAllyUnionFSV(t *testing.T) {
	g := newFakeFogGrid()
	const cx, cy = 40, 50
	g.set(0, cx, cy, fogStateVisible) // ally (player 0) sees it
	g.set(1, cx, cy, fogStateHidden)  // local player 1 does not
	f := NewFogTexture(1)

	f.Update(g, 1<<1) // local only → hidden
	loneLocal := f.At(cx, cy)
	t.Logf("FSV union local-only cell(%d,%d)=%d want=%d", cx, cy, loneLocal, FogHiddenLum)
	if loneLocal != FogHiddenLum {
		t.Fatalf("local-only union=%d want %d", loneLocal, FogHiddenLum)
	}

	f.Update(g, (1<<0)|(1<<1)) // local + ally → visible
	unioned := f.At(cx, cy)
	t.Logf("FSV union local+ally cell(%d,%d)=%d want=%d", cx, cy, unioned, FogVisibleLum)
	if unioned != FogVisibleLum {
		t.Fatalf("local+ally union=%d want %d", unioned, FogVisibleLum)
	}
}

// TestFogTemporalBlendFSV — with blend<1 the buffer must fade monotonically
// toward the target and converge exactly (no rounding stall, no overshoot),
// both rising (reveal) and falling (cell goes explored).
func TestFogTemporalBlendFSV(t *testing.T) {
	g := newFakeFogGrid()
	const cx, cy = 5, 5
	g.set(0, cx, cy, fogStateVisible)
	f := NewFogTexture(0.5)

	// Reveal: 0 → 255, strictly increasing until converged.
	prev := f.At(cx, cy)
	var rise int
	for i := 0; i < 64; i++ {
		f.Update(g, 1<<0)
		cur := f.At(cx, cy)
		if cur < prev {
			t.Fatalf("reveal not monotonic at step %d: %d < %d", i, cur, prev)
		}
		prev = cur
		rise++
		if cur == FogVisibleLum {
			break
		}
	}
	t.Logf("FSV temporal reveal converged to %d in %d steps", prev, rise)
	if prev != FogVisibleLum {
		t.Fatalf("reveal did not converge: %d want %d", prev, FogVisibleLum)
	}

	// Fade: flip cell to explored, 255 → 102, strictly decreasing until converged.
	g.set(0, cx, cy, fogStateExplored)
	prev = f.At(cx, cy)
	var fall int
	for i := 0; i < 64; i++ {
		f.Update(g, 1<<0)
		cur := f.At(cx, cy)
		if cur > prev {
			t.Fatalf("fade not monotonic at step %d: %d > %d", i, cur, prev)
		}
		prev = cur
		fall++
		if cur == FogExploredLum {
			break
		}
	}
	t.Logf("FSV temporal fade converged to %d in %d steps", prev, fall)
	if prev != FogExploredLum {
		t.Fatalf("fade did not converge: %d want %d", prev, FogExploredLum)
	}
}

// TestFogStabilityZeroAllocFSV — once converged, re-running Update on the same
// grid must leave every byte unchanged (5 Hz cadence: intermediate frames are
// stable) and allocate nothing.
func TestFogStabilityZeroAllocFSV(t *testing.T) {
	g := newFakeFogGrid()
	for i := int32(0); i < FogTexSize; i++ {
		g.set(0, i, i, fogStateVisible) // diagonal visible
		g.set(0, i, (i+1)%FogTexSize, fogStateExplored)
	}
	f := NewFogTexture(1)
	f.Update(g, 1<<0) // warm + converge (blend=1 → instant)

	before := make([]byte, len(f.buf))
	copy(before, f.buf)
	f.Update(g, 1<<0)
	mismatch := 0
	for i := range f.buf {
		if f.buf[i] != before[i] {
			mismatch++
		}
	}
	t.Logf("FSV stability re-update mismatches=%d (want 0)", mismatch)
	if mismatch != 0 {
		t.Fatalf("converged buffer changed on stable grid: %d cells", mismatch)
	}

	allocs := testing.AllocsPerRun(200, func() { f.Update(g, 1<<0) })
	t.Logf("FSV Update allocs/op=%v (want 0)", allocs)
	if allocs != 0 {
		t.Fatalf("fog Update allocates %v/op, want 0", allocs)
	}
}

// TestFogEmptyMaskAndBoundsFSV — empty mask yields an all-hidden buffer;
// out-of-range cell reads return hidden (no panic, no bleed).
func TestFogEmptyMaskAndBoundsFSV(t *testing.T) {
	g := newFakeFogGrid()
	g.set(0, 60, 60, fogStateVisible)
	f := NewFogTexture(1)

	f.Update(g, 0) // no players → everything hidden
	nonzero := 0
	for _, b := range f.buf {
		if b != FogHiddenLum {
			nonzero++
		}
	}
	t.Logf("FSV empty-mask nonzero cells=%d (want 0)", nonzero)
	if nonzero != 0 {
		t.Fatalf("empty mask produced %d lit cells", nonzero)
	}

	// Out-of-range source reads.
	for _, c := range [][2]int32{{-1, 0}, {0, -1}, {FogTexSize, 0}, {0, FogTexSize}} {
		if st := g.FogStateAt(0, c[0], c[1]); st != fogStateHidden {
			t.Fatalf("oob (%d,%d) state=%d want hidden", c[0], c[1], st)
		}
	}
	t.Logf("FSV out-of-range reads all hidden")
}

// TestFogVisibleToMaskFSV — VisibleToMask reports visible only for the visible
// state, only for players in the mask.
func TestFogVisibleToMaskFSV(t *testing.T) {
	g := newFakeFogGrid()
	g.set(2, 7, 8, fogStateVisible)
	g.set(3, 9, 9, fogStateExplored)
	cases := []struct {
		mask uint16
		x, y int32
		want bool
	}{
		{1 << 2, 7, 8, true},  // player 2 visible, in mask
		{1 << 1, 7, 8, false}, // player 2 visible, NOT in mask
		{1 << 3, 9, 9, false}, // explored is not visible
		{0xFFFF, 0, 0, false}, // empty cell
	}
	for _, c := range cases {
		got := VisibleToMask(g, c.mask, c.x, c.y)
		t.Logf("FSV VisibleToMask mask=%04x cell(%d,%d)=%v want=%v", c.mask, c.x, c.y, got, c.want)
		if got != c.want {
			t.Fatalf("VisibleToMask mask=%04x cell(%d,%d)=%v want %v", c.mask, c.x, c.y, got, c.want)
		}
	}
}

// TestFogDrawSkipFSV — the sync-pass draw decision: own always drawn; enemy
// drawn only when its cell is visible AND it is detectable.
func TestFogDrawSkipFSV(t *testing.T) {
	cases := []struct {
		name                       string
		isOwn, cellVisible, detect bool
		want                       bool
	}{
		{"own in fog", true, false, false, true},         // own units never fogged
		{"enemy hidden cell", false, false, true, false}, // outside vision → skip
		{"enemy visible detectable", false, true, true, true},
		{"enemy visible invisible", false, true, false, false}, // invisible, no true-sight
		{"enemy visible true-sighted", false, true, true, true},
	}
	for _, c := range cases {
		got := ShouldDrawEntity(c.isOwn, c.cellVisible, c.detect)
		t.Logf("FSV draw %-26s own=%v vis=%v detect=%v -> %v want=%v", c.name, c.isOwn, c.cellVisible, c.detect, got, c.want)
		if got != c.want {
			t.Fatalf("%s: ShouldDrawEntity=%v want %v", c.name, got, c.want)
		}
	}
}

// TestFogTexturePersistenceFSV — the texture is created once (same pointer),
// Upload counts each push, and the backing buffer slice is never reallocated
// (so the GPU keeps reading the same memory the CPU blends into).
func TestFogTexturePersistenceFSV(t *testing.T) {
	f := NewFogTexture(1)
	t0 := f.EnsureTexture()
	t1 := f.EnsureTexture()
	if t0 != t1 {
		t.Fatalf("texture recreated: %p != %p", t0, t1)
	}
	bufPtr := &f.buf[0]
	g := newFakeFogGrid()
	g.set(0, 1, 1, fogStateVisible)
	for i := 0; i < 5; i++ {
		f.Update(g, 1<<0)
		f.Upload()
	}
	if &f.buf[0] != bufPtr {
		t.Fatalf("fog buffer reallocated under Update/Upload")
	}
	t.Logf("FSV texture single-instance=%v uploads=%d buffer-stable=%v", t0 == t1, f.Uploads(), &f.buf[0] == bufPtr)
	if f.Uploads() != 5 {
		t.Fatalf("uploads=%d want 5", f.Uploads())
	}
	if f.Size() != FogTexSize {
		t.Fatalf("size=%d want %d", f.Size(), FogTexSize)
	}
}

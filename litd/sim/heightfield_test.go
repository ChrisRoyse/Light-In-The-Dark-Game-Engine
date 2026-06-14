package sim

// #371 heightfield FSV. SoT = the value TerrainHeight returns after
// BindHeightfield stores known samples, plus the post-save hash. Uses a
// known 2×2 / 3×3 grid so every bilinear result is hand-computable
// (X+X=Y): the four corners read back exactly, the centre is the mean.

import (
	"bytes"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/statehash"
)

func fi(n int32) fixed.F64 { return fixed.FromInt(n) }

// TestTerrainHeightUnboundFSV — edge: no heightfield bound → flat 0
// everywhere (the genuine height of flat terrain, not a placeholder).
func TestTerrainHeightUnboundFSV(t *testing.T) {
	w := NewWorld(Caps{})
	for _, p := range []fixed.Vec2{{X: 0, Y: 0}, {X: fi(500), Y: fi(900)}, {X: fi(-100), Y: fi(20)}} {
		got := w.TerrainHeight(p.X, p.Y)
		t.Logf("FSV unbound: height(%d,%d)=%d (want 0)", p.X.Floor(), p.Y.Floor(), int64(got))
		if got != 0 {
			t.Fatalf("unbound height = %d, want 0", int64(got))
		}
	}
}

// TestTerrainHeightBilinearFSV — a 2×2 grid over [0,100]² with corner
// heights 0/10/20/40 (row-major: (0,0)=0 (100,0)=10 (0,100)=20 (100,100)=40,
// cell=100). Corners read back exactly; mid-edges and centre are the exact
// bilinear blend.
func TestTerrainHeightBilinearFSV(t *testing.T) {
	w := NewWorld(Caps{})
	// samples row-major: r0 = y=0 row [c0=x0, c1=x100]; r1 = y=100 row.
	samples := []fixed.F64{fi(0), fi(10), fi(20), fi(40)}
	if !w.BindHeightfield(2, 2, 0, 0, fi(100), samples) {
		t.Fatal("BindHeightfield refused")
	}
	type tc struct {
		x, y int32
		want fixed.F64
	}
	cases := []tc{
		{0, 0, fi(0)},     // corner SoT
		{100, 0, fi(10)},  // corner
		{0, 100, fi(20)},  // corner
		{100, 100, fi(40)}, // corner
		{50, 0, fixed.F64(5) << 32},               // edge: (0+10)/2 = 5
		{0, 50, fi(10)},                            // edge: (0+20)/2 = 10
		{50, 50, fi(17) + fixed.One/2},             // centre: (0+10+20+40)/4 = 17.5
		{200, 200, fi(40)},                         // clamp past far corner
		{-50, -50, fi(0)},                          // clamp before near corner
	}
	for _, c := range cases {
		got := w.TerrainHeight(fi(c.x), fi(c.y))
		t.Logf("FSV bilinear: height(%d,%d)=%d.%03d want=%d.%03d",
			c.x, c.y, got.Floor(), milli(got), c.want.Floor(), milli(c.want))
		if got != c.want {
			t.Fatalf("height(%d,%d) = %d, want %d (raw)", c.x, c.y, int64(got), int64(c.want))
		}
	}
}

// milli renders the fractional part of a fixed.F64 in thousandths.
func milli(v fixed.F64) int64 {
	frac := v - fixed.F64(v.Floor())<<32
	if frac < 0 {
		frac = -frac
	}
	return frac.Mul(fixed.FromInt(1000)).Floor()
}

// TestBindHeightfieldRejectsFSV — edges: degenerate dims, bad cell size,
// and a sample-count mismatch are all refused (fail-closed), leaving the
// world flat.
func TestBindHeightfieldRejectsFSV(t *testing.T) {
	w := NewWorld(Caps{})
	bad := []struct {
		name             string
		cols, rows       int32
		cell             fixed.F64
		samples          []fixed.F64
	}{
		{"zero cols", 0, 2, fi(100), nil},
		{"zero cell", 2, 2, 0, []fixed.F64{0, 0, 0, 0}},
		{"count mismatch", 2, 2, fi(100), []fixed.F64{0, 0, 0}},
	}
	for _, b := range bad {
		if w.BindHeightfield(b.cols, b.rows, 0, 0, b.cell, b.samples) {
			t.Fatalf("%s: BindHeightfield accepted bad input", b.name)
		}
		t.Logf("FSV reject %-14s -> refused; still flat (height=%d)", b.name, int64(w.TerrainHeight(0, 0)))
		if w.TerrainHeight(fi(50), fi(50)) != 0 {
			t.Fatalf("%s: world not flat after refusal", b.name)
		}
	}
}

// TestHeightfieldSaveRoundTripFSV — the heightfield survives save(v22)→load
// and the full-World hash matches. SoT: TerrainHeight on the reloaded world
// + the state hash.
func TestHeightfieldSaveRoundTripFSV(t *testing.T) {
	w := NewWorld(Caps{})
	samples := []fixed.F64{fi(1), fi(2), fi(3), fi(4), fi(5), fi(6), fi(7), fi(8), fi(9)}
	if !w.BindHeightfield(3, 3, fi(10), fi(20), fi(50), samples) {
		t.Fatal("bind refused")
	}
	reg := NewHashRegistry()
	var before statehash.Snapshot
	w.HashState(reg, &before)

	var buf bytes.Buffer
	const fp = 0x1234
	if err := w.SaveState(&buf, fp); err != nil {
		t.Fatalf("save: %v", err)
	}
	w2 := NewWorld(Caps{})
	if err := w2.LoadState(bytes.NewReader(buf.Bytes()), fp); err != nil {
		t.Fatalf("load: %v", err)
	}
	// origin (10,20), cell 50: sample (0,0) is at world (10,20)=height 1.
	got := w2.TerrainHeight(fi(10), fi(20))
	t.Logf("FSV reload: height(10,20)=%d (want 1)", int64(got)>>32)
	if got != fi(1) {
		t.Fatalf("reloaded corner height = %d, want 1", int64(got)>>32)
	}
	var after statehash.Snapshot
	w2.HashState(reg, &after)
	t.Logf("FSV hash: orig=%016x reload=%016x", before.Top, after.Top)
	if before.Top != after.Top {
		t.Fatalf("post-load hash mismatch: %016x vs %016x", before.Top, after.Top)
	}
}

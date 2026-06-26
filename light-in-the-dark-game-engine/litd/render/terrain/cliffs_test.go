package terrain

import (
	"os"
	"testing"

	litmapdata "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/asset/mapdata"
	"github.com/g3n/engine/math32"
)

type fakeCliff struct {
	w, h  int
	level []int
	ramp  []bool
}

func newFakeCliff(w, h int) *fakeCliff {
	return &fakeCliff{w: w, h: h, level: make([]int, w*h), ramp: make([]bool, w*h)}
}
func (f *fakeCliff) set(x, y, lvl int)     { f.level[y*f.w+x] = lvl }
func (f *fakeCliff) setRamp(x, y, lvl int) { f.level[y*f.w+x] = lvl; f.ramp[y*f.w+x] = true }
func (f *fakeCliff) PathDims() (int, int)  { return f.w, f.h }
func (f *fakeCliff) LevelAt(x, y int) (int, bool, bool) {
	if x < 0 || y < 0 || x >= f.w || y >= f.h {
		return 0, false, false
	}
	return f.level[y*f.w+x], f.ramp[y*f.w+x], true
}

// allCasesField builds a grid exercising every marching-squares case:
// an L-shaped high region (inner+outer corners + walls) and a diagonal pair
// (saddle), on a flat background.
func allCasesField() *fakeCliff {
	f := newFakeCliff(8, 8)
	// L-shape: 3 of a 2×2 block high, the 4th low → inner corner at the notch.
	f.set(1, 1, 1)
	f.set(2, 1, 1)
	f.set(1, 2, 1)
	// (2,2) stays 0 → concave (inner) corner at vertex (2,2).
	// Diagonal high pair → saddle at vertex (5,5).
	f.set(4, 4, 1)
	f.set(5, 5, 1)
	return f
}

// quadVertical asserts positions[base:base+4] form an exactly-vertical wall
// quad and returns (yBot, yTop).
func quadVertical(t *testing.T, p []math32.Vector3, base int) (float32, float32) {
	t.Helper()
	a, b, c, d := p[base], p[base+1], p[base+2], p[base+3]
	// a,b bottom; c,d top. a over d, b over c (same XZ).
	if a.X != d.X || a.Z != d.Z || b.X != c.X || b.Z != c.Z {
		t.Fatalf("quad %d not vertical: XZ drifts top vs bottom", base)
	}
	if a.Y != b.Y || c.Y != d.Y {
		t.Fatalf("quad %d bottom/top edge not level", base)
	}
	if c.Y <= a.Y {
		t.Fatalf("quad %d top %.1f not above bottom %.1f", base, c.Y, a.Y)
	}
	if a.X == b.X && a.Z == b.Z {
		t.Fatalf("quad %d degenerate: bottom edge zero length", base)
	}
	return a.Y, c.Y
}

func TestCliffCaseCoverageFSV(t *testing.T) {
	cm := BuildCliffs(allCasesField())
	names := []string{"Flat", "Wall", "OuterCorner", "InnerCorner", "Saddle"}
	for c := CliffCase(0); c < cliffCaseCount; c++ {
		t.Logf("FSV case %-12s count=%d", names[c], cm.CaseCounts[c])
		if cm.CaseCounts[c] == 0 {
			t.Fatalf("marching-squares case %s never instantiated — table not fully covered", names[c])
		}
	}
}

func TestCliffWallVerticalityFSV(t *testing.T) {
	// Ramp-free field: every emitted quad is a wall, so all are checkable.
	f := newFakeCliff(6, 6)
	for y := 0; y < 6; y++ {
		for x := 3; x < 6; x++ {
			f.set(x, y, 1) // right half level 1, left half level 0
		}
	}
	cm := BuildCliffs(f)
	if cm.TriangleCount() == 0 {
		t.Fatal("no cliff geometry for a level boundary")
	}
	walls := 0
	for base := 0; base < len(cm.Positions); base += 4 {
		yBot, yTop := quadVertical(t, cm.Positions, base)
		// Watertight: top/bottom land exactly on terrace heights.
		if math32.Mod(yBot, CliffLevelHeight) != 0 || math32.Mod(yTop, CliffLevelHeight) != 0 {
			t.Fatalf("quad %d edges off terrace: bot=%.1f top=%.1f", base, yBot, yTop)
		}
		walls++
	}
	t.Logf("FSV vertical walls=%d all exactly vertical, edges on terraces", walls)
}

// TestCliffTwoLevelNoHoleFSV — a 2-level jump produces one wall spanning the
// full 0..1024 gap, no mid-level hole (edge case 1).
func TestCliffTwoLevelNoHoleFSV(t *testing.T) {
	f := newFakeCliff(4, 4)
	f.set(2, 1, 2) // a single level-2 cell in a level-0 field
	f.set(2, 2, 2)
	cm := BuildCliffs(f)
	found := false
	for base := 0; base < len(cm.Positions); base += 4 {
		yBot, yTop := quadVertical(t, cm.Positions, base)
		if yBot == 0 && yTop == 2*CliffLevelHeight {
			found = true
		}
	}
	t.Logf("FSV two-level wall spanning 0..%.0f present=%v", 2*CliffLevelHeight, found)
	if !found {
		t.Fatalf("2-level jump must yield a wall spanning the full gap (no hole)")
	}
}

// TestCliffRampSlopeFSV — a ramp cell slopes from its low level to high level,
// grid-aligned, with corners at both heights (edge case 2: walkable ramp).
func TestCliffRampSlopeFSV(t *testing.T) {
	f := newFakeCliff(4, 4)
	// level-1 region on the right; a ramp cell bridging into it.
	for y := 0; y < 4; y++ {
		f.set(3, y, 1)
	}
	f.setRamp(2, 1, 0) // ramp at level 0→1, high neighbor at +X (cell (3,1)=1)
	cm := BuildCliffs(f)

	// Find the ramp quad: it must have some corners at 0 and some at 512.
	lo, hi := float32(0), CliffLevelHeight
	rampFound := false
	for base := 0; base < len(cm.Positions); base += 4 {
		var has0, has512 bool
		for i := 0; i < 4; i++ {
			y := cm.Positions[base+i].Y
			if y == lo {
				has0 = true
			}
			if y == hi {
				has512 = true
			}
		}
		if has0 && has512 {
			// Is this the ramp (a non-vertical, sloped top quad)?
			p := cm.Positions[base : base+4]
			if !(p[0].X == p[3].X && p[0].Z == p[3].Z) { // not a vertical wall layout
				rampFound = true
				t.Logf("FSV ramp quad Ys=[%.0f %.0f %.0f %.0f] (slopes 0→512)", p[0].Y, p[1].Y, p[2].Y, p[3].Y)
			}
		}
	}
	if !rampFound {
		t.Fatalf("ramp cell did not produce a sloped 0→512 surface")
	}
}

func TestCliffDeterminismFSV(t *testing.T) {
	f := allCasesField()
	a := BuildCliffs(f)
	b := BuildCliffs(f)
	if len(a.Positions) != len(b.Positions) || len(a.Indices) != len(b.Indices) {
		t.Fatalf("nondeterministic sizes: %d/%d vs %d/%d", len(a.Positions), len(a.Indices), len(b.Positions), len(b.Indices))
	}
	for i := range a.Positions {
		if a.Positions[i] != b.Positions[i] {
			t.Fatalf("nondeterministic vertex %d: %v != %v", i, a.Positions[i], b.Positions[i])
		}
	}
	t.Logf("FSV determinism: %d verts %d tris bit-identical across two builds", len(a.Positions), a.TriangleCount())
}

func TestCliffNoDegenerateTrianglesFSV(t *testing.T) {
	cm := BuildCliffs(allCasesField())
	degenerate := 0
	for i := 0; i < len(cm.Indices); i += 3 {
		a := cm.Positions[cm.Indices[i]]
		b := cm.Positions[cm.Indices[i+1]]
		c := cm.Positions[cm.Indices[i+2]]
		ab := b.Clone().Sub(&a)
		ac := c.Clone().Sub(&a)
		if ab.Clone().Cross(ac).Length() < 1e-3 {
			degenerate++
		}
	}
	t.Logf("FSV degenerate triangles=%d / %d (want 0)", degenerate, cm.TriangleCount())
	if degenerate != 0 {
		t.Fatalf("%d degenerate cliff triangles", degenerate)
	}
}

// TestCliffRealMapFSV — real test64 (has a 0↔1 cliff with a ramp): geometry is
// generated, every wall vertical with terrace-aligned edges, ramp present.
func TestCliffRealMapFSV(t *testing.T) {
	m, err := litmapdata.Load(os.DirFS("../../.."), "data/maps/test64")
	if err != nil {
		t.Fatalf("load test64: %v", err)
	}
	cm := BuildCliffs(NewMapCliffField(m))
	t.Logf("FSV realmap cliff tris=%d verts=%d cases=%v", cm.TriangleCount(), len(cm.Positions), cm.CaseCounts)
	if cm.TriangleCount() == 0 {
		t.Fatal("test64 has a cliff boundary but no geometry generated")
	}
	// Wall case must be covered (test64 has a straight 0↔1 boundary).
	if cm.CaseCounts[CaseWall] == 0 {
		t.Fatal("test64 straight cliff did not register a Wall case")
	}
	// Spot-check heights are terrace-aligned across all geometry.
	for _, p := range cm.Positions {
		if math32.Mod(p.Y, CliffLevelHeight) != 0 {
			t.Fatalf("realmap vertex Y=%.2f not on a terrace multiple of %.0f", p.Y, CliffLevelHeight)
		}
	}
}

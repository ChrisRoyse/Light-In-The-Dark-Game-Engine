package path

import (
	"strings"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/statehash"
)

// dump renders a window of the grid as ASCII: ramps print 'R',
// occupied-static '#', plain cells print their cliff level digit;
// unwalkable (no Walkable flag) cells print 'x'.
func dump(g *Grid, r Rect) string {
	var b strings.Builder
	for y := r.Y; y < r.Y+r.H; y++ {
		for x := r.X; x < r.X+r.W; x++ {
			switch {
			case g.FlagsAt(x, y)&OccupiedStatic != 0:
				b.WriteByte('#')
			case g.FlagsAt(x, y)&Walkable == 0:
				b.WriteByte('x')
			case g.IsRamp(x, y):
				b.WriteByte('R')
			default:
				b.WriteByte('0' + g.CliffLevel(x, y))
			}
		}
		b.WriteByte('\n')
	}
	return b.String()
}

func walkableGrid() *Grid {
	g := NewGrid()
	for y := int32(0); y < GridSize; y++ {
		for x := int32(0); x < GridSize; x++ {
			g.SetFlags(x, y, Walkable|Buildable)
		}
	}
	return g
}

// The §7 memory contract: 256 KB flags + 256 KB cliff, allocated once.
func TestGridAllocationSizes(t *testing.T) {
	g := NewGrid()
	t.Logf("flags layer: %d bytes; cliff layer: %d bytes; total: %d bytes",
		len(g.flags), len(g.cliff), g.PreallocatedBytes())
	if len(g.flags) != 262144 || len(g.cliff) != 262144 {
		t.Fatalf("layer sizes wrong: flags=%d cliff=%d (want 262144 each)", len(g.flags), len(g.cliff))
	}
}

// Edge 1: stepping level 1 → level 2 without a ramp is illegal; the
// same step through a ramp cell is legal.
func TestGridCliffStepNeedsRamp(t *testing.T) {
	g := walkableGrid()
	// column x=0 level 1, column x=2 level 2, x=1 plain level 1 at
	// y=0..1 (a bare cliff face) but a ramp 1/2 at y=2
	for y := int32(0); y < 3; y++ {
		g.SetCliffLevel(0, y, 1)
		g.SetCliffLevel(1, y, 1)
		g.SetCliffLevel(2, y, 2)
	}
	g.SetRamp(1, 2, 1)

	t.Logf("3x3 neighborhood (levels, R=ramp 1/2):\n%s", dump(g, Rect{0, 0, 3, 3}))
	type verdict struct {
		name      string
		ax, ay    int32
		bx, by    int32
		wantLegal bool
	}
	cases := []verdict{
		{"same level 1->1", 0, 0, 1, 0, true},
		{"cliff face 1->2 no ramp", 1, 0, 2, 0, false},
		{"cliff face 1->2 no ramp (row 1)", 1, 1, 2, 1, false},
		{"onto ramp from level 1", 0, 2, 1, 2, true},
		{"ramp -> level 2", 1, 2, 2, 2, true},
		{"level 2 -> ramp", 2, 2, 1, 2, true},
	}
	for _, c := range cases {
		got := g.AdjacencyLegal(c.ax, c.ay, c.bx, c.by)
		t.Logf("verdict: (%d,%d)->(%d,%d) %-32s legal=%v (want %v)", c.ax, c.ay, c.bx, c.by, c.name, got, c.wantLegal)
		if got != c.wantLegal {
			t.Errorf("%s: legal=%v want %v", c.name, got, c.wantLegal)
		}
	}
}

// Edge 2: a ramp joins exactly L and L+1 — level 0 → level 2 through
// a single 0/1 ramp is still illegal at the ramp→2 step.
func TestGridRampJoinsAdjacentLevelsOnly(t *testing.T) {
	g := walkableGrid()
	g.SetCliffLevel(0, 0, 0)
	g.SetRamp(1, 0, 0) // joins 0 and 1
	g.SetCliffLevel(2, 0, 2)

	t.Logf("row (0=level0, R=ramp 0/1, 2=level2):\n%s", dump(g, Rect{0, 0, 3, 1}))
	if !g.AdjacencyLegal(0, 0, 1, 0) {
		t.Errorf("level0 -> ramp(0/1) must be legal")
	}
	legal := g.AdjacencyLegal(1, 0, 2, 0)
	t.Logf("verdict: ramp(0/1) -> level2 legal=%v (want false: ramp tops out at 1)", legal)
	if legal {
		t.Errorf("ramp(0/1) -> level2 must be illegal")
	}
	// and a 1/2 ramp in the same spot makes it legal — the rule is
	// about the levels the ramp carries, not ramp-ness itself
	g.SetRamp(1, 0, 1)
	if !g.AdjacencyLegal(1, 0, 2, 0) {
		t.Errorf("ramp(1/2) -> level2 must be legal")
	}
	if g.AdjacencyLegal(0, 0, 1, 0) {
		t.Errorf("level0 -> ramp(1/2) must be illegal")
	}
}

// Edge 3: a destructable (tree block) stamps OccupiedStatic at bake;
// its death clears the stamp and the cells flip back to walkable.
func TestGridDestructableStampClears(t *testing.T) {
	g := walkableGrid()
	trees := Rect{2, 1, 3, 2}
	g.StampStatic(trees)

	before := dump(g, Rect{0, 0, 7, 5})
	t.Logf("before destructable death (# = OccupiedStatic):\n%s", before)
	if g.CellWalkable(3, 2) || g.StepLegal(1, 1, 2, 1) {
		t.Fatalf("stamped cells must not be occupiable")
	}

	g.ClearStatic(trees) // tree destroyed — WC3 tree-cutting
	after := dump(g, Rect{0, 0, 7, 5})
	t.Logf("after destructable death:\n%s", after)
	if !g.CellWalkable(3, 2) || !g.StepLegal(1, 1, 2, 1) {
		t.Fatalf("cleared cells must be occupiable again")
	}
	if strings.Contains(after, "#") {
		t.Fatalf("stamp residue left after clear:\n%s", after)
	}
}

// Edge 4: within-level "rolling terrain" does not exist in the grid —
// only the discrete cliff level is stored, so two same-level cells
// are always adjacency-legal no matter what the heightmap renders.
func TestGridWithinLevelHeightIrrelevant(t *testing.T) {
	g := walkableGrid()
	for x := int32(0); x < 4; x++ {
		g.SetCliffLevel(x, 0, 3)
	}
	t.Logf("rolling hill row, all cliff level 3:\n%s", dump(g, Rect{0, 0, 4, 1}))
	for x := int32(0); x < 3; x++ {
		if !g.AdjacencyLegal(x, 0, x+1, 0) {
			t.Errorf("same-level step (%d,0)->(%d,0) must be legal", x, x+1)
		}
	}
	t.Logf("verdict: all 3 same-level steps legal — smooth height is cosmetic, level is gameplay")
}

// Occupancy semantics: dynamic reservation blocks like static, and
// flag clears are surgical (other bits survive).
func TestGridOccupancyAndFlagSurgery(t *testing.T) {
	g := walkableGrid()
	g.OrFlags(5, 5, OccupiedDynamic)
	if g.CellWalkable(5, 5) {
		t.Fatalf("OccupiedDynamic cell must not be occupiable")
	}
	g.ClearFlags(5, 5, OccupiedDynamic)
	if !g.CellWalkable(5, 5) || g.FlagsAt(5, 5)&Buildable == 0 {
		t.Fatalf("clear must restore walkability and preserve Buildable")
	}
}

// Hashed state: flags and cliffs both feed the hash; a single ramp
// bit or occupancy stamp changes it (and reverting restores it).
func TestGridHashCoversFlagsAndCliffs(t *testing.T) {
	sum := func(g *Grid) uint64 {
		h := statehash.New()
		g.HashInto(h)
		return h.Sum64()
	}
	g := walkableGrid()
	base := sum(g)

	g.StampStatic(Rect{10, 10, 1, 1})
	stamped := sum(g)
	g.ClearStatic(Rect{10, 10, 1, 1})
	cleared := sum(g)

	g.SetRamp(20, 20, 0)
	ramped := sum(g)
	g.SetCliffLevel(20, 20, 0)
	unramped := sum(g)

	t.Logf("hash base=%016x stamped=%016x cleared=%016x ramped=%016x unramped=%016x",
		base, stamped, cleared, ramped, unramped)
	if stamped == base || ramped == base {
		t.Fatalf("flag/cliff mutations must change the hash")
	}
	if cleared != base || unramped != base {
		t.Fatalf("reverted mutations must restore the hash")
	}
}

// Fail closed: out-of-bounds access and oversized levels panic.
func TestGridFailClosed(t *testing.T) {
	g := NewGrid()
	for name, f := range map[string]func(){
		"oob-read":       func() { g.FlagsAt(-1, 0) },
		"oob-write":      func() { g.SetFlags(0, GridSize, Walkable) },
		"oob-rect":       func() { g.StampStatic(Rect{GridSize - 1, 0, 2, 1}) },
		"negative-rect":  func() { g.StampStatic(Rect{0, 0, -1, 1}) },
		"level-overflow": func() { g.SetCliffLevel(0, 0, MaxCliffLevel+1) },
		"ramp-overflow":  func() { g.SetRamp(0, 0, MaxCliffLevel+1) },
	} {
		func() {
			defer func() {
				if recover() == nil {
					t.Errorf("%s did not panic", name)
				}
			}()
			f()
		}()
	}
}

func BenchmarkGridStepLegal(b *testing.B) {
	g := NewGrid()
	for y := int32(0); y < GridSize; y++ {
		for x := int32(0); x < GridSize; x++ {
			g.SetFlags(x, y, Walkable)
		}
	}
	b.ReportAllocs()
	ok := 0
	for i := 0; i < b.N; i++ {
		x := int32(i & 255)
		if g.StepLegal(x, 100, x+1, 100) {
			ok++
		}
	}
	_ = ok
}

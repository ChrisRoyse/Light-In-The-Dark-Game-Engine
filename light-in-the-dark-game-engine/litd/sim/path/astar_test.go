package path

import (
	"strings"
	"testing"
)

// walkableLayer builds a grid with Walkable set on a rect and one
// ground layer (radius 0) over it.
func walkableLayer(open Rect) (*Grid, *Layer) {
	g := NewGrid()
	open.forEach(func(x, y int32) { g.OrFlags(x, y, Walkable) })
	d := NewDilatedSet(g, []LayerKey{{Required: Walkable, Blocked: OccupiedStatic | OccupiedDynamic}})
	d.RecomputeAll()
	return g, d.Layer(0)
}

func cellXY(c int32) (int32, int32) { return c % GridSize, c / GridSize }

func pathString(cells []int32) string {
	var b strings.Builder
	for i, c := range cells {
		x, y := cellXY(c)
		if i > 0 {
			b.WriteByte(' ')
		}
		b.WriteString("(")
		b.WriteString(itoa(x))
		b.WriteString(",")
		b.WriteString(itoa(y))
		b.WriteString(")")
	}
	return b.String()
}

func itoa(v int32) string {
	if v == 0 {
		return "0"
	}
	neg := v < 0
	if neg {
		v = -v
	}
	var buf [12]byte
	i := len(buf)
	for v > 0 {
		i--
		buf[i] = byte('0' + v%10)
		v /= 10
	}
	if neg {
		i--
		buf[i] = '-'
	}
	return string(buf[i:])
}

// Edge 1: symmetric open map, many equal-f paths — 100 runs must
// produce the identical path and identical expansion count.
func TestAStarDeterministicAcrossRuns(t *testing.T) {
	g, l := walkableLayer(Rect{X: 0, Y: 0, W: 32, H: 32})
	s := NewSearcher(g)
	var first []int32
	var firstExp int32
	counts := map[int32]int{}
	for run := 0; run < 100; run++ {
		out, ok := s.Search(l, 2, 2, 29, 13, nil)
		if !ok {
			t.Fatalf("run %d found no path", run)
		}
		counts[s.Expansions()]++
		if run == 0 {
			first = append([]int32(nil), out...)
			firstExp = s.Expansions()
			continue
		}
		if len(out) != len(first) {
			t.Fatalf("run %d path length %d != %d", run, len(out), len(first))
		}
		for i := range out {
			if out[i] != first[i] {
				t.Fatalf("run %d diverges at waypoint %d", run, i)
			}
		}
		if s.Expansions() != firstExp {
			t.Fatalf("run %d expansions %d != %d", run, s.Expansions(), firstExp)
		}
	}
	t.Logf("(2,2)->(29,13) on symmetric 32x32: path len=%d, expansion counts over 100 runs: %v (single key = identical)",
		len(first), counts)
	t.Logf("path: %s", pathString(first))
	// 27 east, 11 north: octile optimum = 11 diagonals + 16 cardinals
	if len(first) != 27 {
		t.Fatalf("optimal octile path is max(dx,dy)=27 steps, got %d", len(first))
	}
}

// Edge 2: diagonal past a blocked orthogonal is refused. 3×3 map,
// (1,0) blocked: NE from (0,0) must not cut the corner to (1,1).
func TestAStarNoCornerCut(t *testing.T) {
	g := NewGrid()
	for y := int32(0); y < 3; y++ {
		for x := int32(0); x < 3; x++ {
			g.OrFlags(x, y, Walkable)
		}
	}
	g.ClearFlags(1, 0, Walkable) // the blocked orthogonal
	d := NewDilatedSet(g, []LayerKey{{Required: Walkable}})
	d.RecomputeAll()
	l := d.Layer(0)
	s := NewSearcher(g)
	out, ok := s.Search(l, 0, 0, 1, 1, nil)
	if !ok {
		t.Fatal("no path found")
	}
	rows := []string{}
	for y := int32(2); y >= 0; y-- {
		row := ""
		for x := int32(0); x < 3; x++ {
			switch {
			case x == 0 && y == 0:
				row += "S"
			case x == 1 && y == 1:
				row += "G"
			case !l.CenterClear(x, y):
				row += "#"
			default:
				row += "."
			}
		}
		rows = append(rows, row)
	}
	t.Logf("map (y up):\n  %s\n  %s\n  %s", rows[0], rows[1], rows[2])
	t.Logf("path from S(0,0) to G(1,1): %s (expansions=%d)", pathString(out), s.Expansions())
	fx, fy := cellXY(out[0])
	if fx == 1 && fy == 1 {
		t.Fatalf("corner cut: stepped diagonally past blocked (1,0)")
	}
	if !(fx == 0 && fy == 1) {
		t.Fatalf("first step must be N to (0,1), got (%d,%d)", fx, fy)
	}
	if len(out) != 2 {
		t.Fatalf("detour path must be 2 steps, got %d: %s", len(out), pathString(out))
	}
}

// Edge 3: goal == start → empty path, zero expansions.
func TestAStarGoalIsStart(t *testing.T) {
	g, l := walkableLayer(Rect{X: 0, Y: 0, W: 8, H: 8})
	s := NewSearcher(g)
	out, ok := s.Search(l, 3, 3, 3, 3, nil)
	t.Logf("start==goal (3,3): ok=%v len(path)=%d expansions=%d", ok, len(out), s.Expansions())
	if !ok || len(out) != 0 || s.Expansions() != 0 {
		t.Fatalf("identity search wrong: ok=%v len=%d exp=%d", ok, len(out), s.Expansions())
	}
}

// Edge 4: back-to-back searches share scratch without clearing — the
// epoch stamp isolates them. The second search's result equals a
// fresh searcher's, and stale first-search g-values remain in the
// array under an old epoch.
func TestAStarEpochScratchIsolation(t *testing.T) {
	g, l := walkableLayer(Rect{X: 0, Y: 0, W: 64, H: 64})
	shared := NewSearcher(g)
	out1, ok1 := shared.Search(l, 1, 1, 20, 20, nil)
	exp1 := shared.Expansions()
	probe := idx(10, 10) // on the first path's diagonal
	staleG, staleStamp := shared.gCost[probe], shared.stamp[probe]

	out2, ok2 := shared.Search(l, 40, 40, 50, 41, nil)
	exp2 := shared.Expansions()

	fresh := NewSearcher(g)
	outF, okF := fresh.Search(l, 40, 40, 50, 41, nil)
	expF := fresh.Expansions()

	t.Logf("search1 (1,1)->(20,20): ok=%v len=%d exp=%d; probe cell (10,10): g=%d stamp(epoch)=%d", ok1, len(out1), exp1, staleG, staleStamp)
	t.Logf("search2 on SAME scratch (40,40)->(50,41): ok=%v len=%d exp=%d; searcher epoch now=%d", ok2, len(out2), exp2, shared.epoch)
	t.Logf("fresh searcher same query:                ok=%v len=%d exp=%d", okF, len(outF), expF)
	t.Logf("probe cell after search2: g=%d stamp=%d (stale epoch %d != current %d -> ignored, never cleared)",
		shared.gCost[probe], shared.stamp[probe], shared.stamp[probe], shared.epoch)
	if !ok1 || !ok2 || !okF {
		t.Fatal("searches failed")
	}
	if exp2 != expF || len(out2) != len(outF) {
		t.Fatalf("shared scratch diverged from fresh: exp %d vs %d, len %d vs %d", exp2, expF, len(out2), len(outF))
	}
	for i := range out2 {
		if out2[i] != outF[i] {
			t.Fatalf("waypoint %d differs between shared and fresh", i)
		}
	}
	if shared.stamp[probe] == shared.epoch {
		t.Fatalf("probe cell must hold a stale epoch (search2 never touched it)")
	}
}

// Known-cost audit (X+X=Y): 4 cells east costs 40; 3 cells diagonal
// costs 42; an L of 2 east + 2 NE costs 2×10+2×14=48.
func TestAStarIntegerCosts(t *testing.T) {
	g, l := walkableLayer(Rect{X: 0, Y: 0, W: 16, H: 16})
	s := NewSearcher(g)
	cases := []struct {
		tx, ty int32
		want   int32
		name   string
	}{
		{4, 0, 40, "4 east = 4x10"},
		{3, 3, 42, "3 diagonal = 3x14"},
		{4, 2, 48, "2 diag + 2 card = 2x14+2x10"},
	}
	for _, c := range cases {
		_, ok := s.Search(l, 0, 0, c.tx, c.ty, nil)
		goalG := s.gCost[idx(c.tx, c.ty)]
		t.Logf("%s: g(goal)=%d want %d (ok=%v)", c.name, goalG, c.want, ok)
		if !ok || goalG != c.want {
			t.Fatalf("%s: got %d want %d", c.name, goalG, c.want)
		}
	}
}

// Unreachable goal: walled-in target drains the open list and fails
// deterministically with a stable expansion count.
func TestAStarUnreachable(t *testing.T) {
	g := NewGrid()
	for y := int32(0); y < 8; y++ {
		for x := int32(0); x < 8; x++ {
			g.OrFlags(x, y, Walkable)
		}
	}
	// wall off (6,6): clear Walkable on its 8 neighbors
	for _, d := range neighborOrder {
		g.ClearFlags(6+d.dx, 6+d.dy, Walkable)
	}
	d := NewDilatedSet(g, []LayerKey{{Required: Walkable}})
	d.RecomputeAll()
	l := d.Layer(0)
	s := NewSearcher(g)
	out, ok := s.Search(l, 0, 0, 6, 6, nil)
	exp1 := s.Expansions()
	_, ok2 := s.Search(l, 0, 0, 6, 6, nil)
	exp2 := s.Expansions()
	t.Logf("walled goal: ok=%v len=%d expansions run1=%d run2=%d (identical)", ok, len(out), exp1, exp2)
	if ok || ok2 || len(out) != 0 || exp1 != exp2 {
		t.Fatalf("unreachable must fail deterministically: ok=%v exp %d/%d", ok, exp1, exp2)
	}
}

func BenchmarkAStar(b *testing.B) {
	g, l := walkableLayer(Rect{X: 0, Y: 0, W: 128, H: 128})
	s := NewSearcher(g)
	out := make([]int32, 0, 512)
	b.ReportAllocs()
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		out = out[:0]
		var ok bool
		out, ok = s.Search(l, 1, 1, 126, 100, out)
		if !ok {
			b.Fatal("no path")
		}
	}
}

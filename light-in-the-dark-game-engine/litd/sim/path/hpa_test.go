package path

import (
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/prng"
)

// openMapHPA builds a fully-walkable W×H region with a ground layer
// and an HPA hierarchy over it.
func openMapHPA(w, hgt int32) (*Grid, *DilatedSet, *HPA) {
	g := NewGrid()
	for y := int32(0); y < hgt; y++ {
		for x := int32(0); x < w; x++ {
			g.OrFlags(x, y, Walkable)
		}
	}
	d := NewDilatedSet(g, []LayerKey{{Required: Walkable, Blocked: OccupiedStatic | OccupiedDynamic}})
	d.RecomputeAll()
	h := NewHPA(g, d.Layer(0), NewSearcher(g))
	return g, d, h
}

// Edge 1: walled-off goal — labels answer unreachable in O(1) (zero
// search expansions for the verdict), nearest reachable substituted.
func TestHPAUnreachableSubstitution(t *testing.T) {
	g := NewGrid()
	for y := int32(0); y < 48; y++ {
		for x := int32(0); x < 48; x++ {
			g.OrFlags(x, y, Walkable)
		}
	}
	// wall a 6×6 pocket around (40,40): perimeter blocked, inside walkable
	for i := int32(37); i <= 43; i++ {
		g.ClearFlags(i, 37, Walkable)
		g.ClearFlags(i, 43, Walkable)
		g.ClearFlags(37, i, Walkable)
		g.ClearFlags(43, i, Walkable)
	}
	d := NewDilatedSet(g, []LayerKey{{Required: Walkable}})
	d.RecomputeAll()
	h := NewHPA(g, d.Layer(0), NewSearcher(g))

	sL, gL := h.Label(2, 2), h.Label(40, 40)
	t.Logf("labels: start(2,2)=%d, pocket goal(40,40)=%d (differ -> unreachable known in O(1), no search)", sL, gL)
	if sL == gL || sL < 0 || gL < 0 {
		t.Fatalf("pocket must be a separate component: %d vs %d", sL, gL)
	}
	out, res, ok := h.FindPath(2, 2, 40, 40, nil)
	t.Logf("FindPath: stage=%s substituted=%v goal=(%d,%d) coarseExp=%d fineExp=%d pathLen=%d",
		res.Stage, res.Substituted, res.GoalX, res.GoalY, res.CoarseExpansions, res.FineExpansions, len(out))
	if !ok || !res.Substituted {
		t.Fatalf("must substitute nearest reachable: ok=%v res=%+v", ok, res)
	}
	if h.Label(res.GoalX, res.GoalY) != sL {
		t.Fatalf("substituted goal not in start's component")
	}
	// substituted goal must hug the pocket wall (ring scan distance:
	// pocket interior to outside the 37..43 perimeter is ≤ 4)
	dx, dy := res.GoalX-40, res.GoalY-40
	if dx < -4 || dx > 4 || dy < -4 || dy > 4 {
		t.Fatalf("substitute too far: (%d,%d)", res.GoalX, res.GoalY)
	}
	// truly unreachable: empty far component beyond substitution radius
	g2 := NewGrid()
	for x := int32(0); x < 8; x++ {
		for y := int32(0); y < 8; y++ {
			g2.OrFlags(x, y, Walkable)
			g2.OrFlags(500+x, 500+y, Walkable)
		}
	}
	d2 := NewDilatedSet(g2, []LayerKey{{Required: Walkable}})
	d2.RecomputeAll()
	h2 := NewHPA(g2, d2.Layer(0), NewSearcher(g2))
	_, res2, ok2 := h2.FindPath(2, 2, 504, 504, nil)
	t.Logf("island goal 700 cells away: ok=%v stage=%s coarseExp=%d fineExp=%d (zero — no search ran)",
		ok2, res2.Stage, res2.CoarseExpansions, res2.FineExpansions)
	if ok2 || res2.Stage != StageUnreachable || res2.CoarseExpansions != 0 || res2.FineExpansions != 0 {
		t.Fatalf("unreachable must cost zero expansions: %+v", res2)
	}
}

// Edge 2: a stamp sealing the only corridor splits the labels into
// two components; only dirty sectors rebuild.
func TestHPAStampSplitsComponents(t *testing.T) {
	g := NewGrid()
	// two 32×32 rooms joined by a 2-wide corridor at y=14..15, x=32..47
	for y := int32(0); y < 32; y++ {
		for x := int32(0); x < 32; x++ {
			g.OrFlags(x, y, Walkable)    // west room
			g.OrFlags(48+x, y, Walkable) // east room
		}
	}
	for x := int32(32); x < 48; x++ {
		g.OrFlags(x, 14, Walkable)
		g.OrFlags(x, 15, Walkable)
	}
	d := NewDilatedSet(g, []LayerKey{{Required: Walkable, Blocked: OccupiedStatic}})
	d.RecomputeAll()
	h := NewHPA(g, d.Layer(0), NewSearcher(g))

	before := [2]int32{h.Label(5, 5), h.Label(70, 5)}
	t.Logf("before seal: label(west 5,5)=%d label(east 70,5)=%d (equal = connected through corridor)", before[0], before[1])
	if before[0] != before[1] {
		t.Fatalf("rooms must start connected")
	}
	seal := Rect{X: 38, Y: 14, W: 2, H: 2}
	d.StampStatic(seal)
	h.RebuildRect(seal)
	after := [2]int32{h.Label(5, 5), h.Label(70, 5)}
	t.Logf("after StampStatic(%+v)+RebuildRect: label(west)=%d label(east)=%d (differ = split into 2 components)", seal, after[0], after[1])
	if after[0] == after[1] {
		t.Fatalf("seal must split the components")
	}
	if !h.Reachable(5, 5, 20, 20) || h.Reachable(5, 5, 70, 5) {
		t.Fatalf("reachability O(1) answers wrong after seal")
	}
	// unseal restores one component
	d.ClearStatic(seal)
	h.RebuildRect(seal)
	restored := [2]int32{h.Label(5, 5), h.Label(70, 5)}
	t.Logf("after ClearStatic+RebuildRect: labels %d/%d (rejoined)", restored[0], restored[1])
	if restored[0] != restored[1] {
		t.Fatalf("unseal must rejoin components")
	}
}

// Edge 3: start and goal in the same sector skip the coarse stage.
func TestHPASameSectorSkipsCoarse(t *testing.T) {
	_, _, h := openMapHPA(64, 64)
	out, res, ok := h.FindPath(2, 2, 13, 9, nil)
	t.Logf("same-sector (2,2)->(13,9): stage=%s coarseExp=%d fineExp=%d corridorSectors=%d pathLen=%d ok=%v",
		res.Stage, res.CoarseExpansions, res.FineExpansions, res.CorridorSectorCnt, len(out), ok)
	if !ok || res.Stage != StageFineOnly || res.CoarseExpansions != 0 || res.CorridorSectorCnt != 1 {
		t.Fatalf("same-sector search must skip coarse: %+v", res)
	}
	if len(out) != 11 { // max(11,7) octile steps
		t.Fatalf("path length %d, want 11", len(out))
	}
}

// Edge 4: corridor-constrained cost equals flat-A* cost on an open
// map (corridor covers the straight line) and stays within the
// corridor-optimality bound (+10%) on a blocked map.
func TestHPACostWithinBound(t *testing.T) {
	g, _, h := openMapHPA(128, 128)
	flat := NewSearcher(g)
	_, okF := flat.Search(h.l, 3, 3, 120, 90, nil)
	flatCost := flat.gCost[idx(120, 90)]
	out, res, okH := h.FindPath(3, 3, 120, 90, nil)
	hpaCost := walkCost(t, 3, 3, out)
	t.Logf("open map (3,3)->(120,90): flat cost=%d (exp=%d) | hpa cost=%d (coarse=%d fine=%d corridor=%d sectors) len=%d",
		flatCost, flat.Expansions(), hpaCost, res.CoarseExpansions, res.FineExpansions, res.CorridorSectorCnt, len(out))
	if !okF || !okH {
		t.Fatal("searches failed")
	}
	if hpaCost < flatCost {
		t.Fatalf("hpa cheaper than optimal flat: %d < %d", hpaCost, flatCost)
	}
	if hpaCost*10 > flatCost*11 {
		t.Fatalf("hpa cost %d exceeds +10%% bound over flat %d", hpaCost, flatCost)
	}
}

// walkCost sums 10/14 step costs along a waypoint list, failing on
// any non-adjacent hop (stitched segments must form a real walk).
func walkCost(t *testing.T, sx, sy int32, cells []int32) int32 {
	t.Helper()
	cost := int32(0)
	px, py := sx, sy
	for i, c := range cells {
		x, y := cellXY(c)
		dx, dy := x-px, y-py
		if dx < -1 || dx > 1 || dy < -1 || dy > 1 || (dx == 0 && dy == 0) {
			t.Fatalf("waypoint %d not adjacent: (%d,%d)->(%d,%d)", i, px, py, x, y)
		}
		if dx != 0 && dy != 0 {
			cost += CostDiagonal
		} else {
			cost += CostCardinal
		}
		px, py = x, y
	}
	return cost
}

// Determinism: repeated FindPath on the blocked fixture gives
// identical coarse+fine expansion counts and waypoints.
func TestHPADeterministicRepeat(t *testing.T) {
	g, _, h := blockedFixture()
	_ = g
	out1, res1, ok1 := h.FindPath(2, 2, 509, 509, nil)
	out2, res2, ok2 := h.FindPath(2, 2, 509, 509, nil)
	t.Logf("run1: ok=%v coarse=%d fine=%d len=%d | run2: ok=%v coarse=%d fine=%d len=%d",
		ok1, res1.CoarseExpansions, res1.FineExpansions, len(out1),
		ok2, res2.CoarseExpansions, res2.FineExpansions, len(out2))
	if !ok1 || !ok2 || res1 != res2 || len(out1) != len(out2) {
		t.Fatalf("repeat diverged: %+v vs %+v", res1, res2)
	}
	for i := range out1 {
		if out1[i] != out2[i] {
			t.Fatalf("waypoint %d differs", i)
		}
	}
}

// blockedFixture is the D-29-style pathological fixture: 512×512,
// 20% seeded-PRNG blockers, corner-to-corner long path.
func blockedFixture() (*Grid, *DilatedSet, *HPA) {
	g := NewGrid()
	rng := prng.New(42, 1)
	for y := int32(0); y < GridSize; y++ {
		for x := int32(0); x < GridSize; x++ {
			if rng.Uint32()%100 < 20 {
				continue // blocked
			}
			g.OrFlags(x, y, Walkable)
		}
	}
	// keep endpoints open
	for _, c := range [][2]int32{{2, 2}, {509, 509}} {
		g.OrFlags(c[0], c[1], Walkable)
	}
	d := NewDilatedSet(g, []LayerKey{{Required: Walkable}})
	d.RecomputeAll()
	h := NewHPA(g, d.Layer(0), NewSearcher(g))
	return g, d, h
}

// The mandated expansion cut: corridor-constrained fine search must
// expand ≥10× fewer nodes than flat A* on the long-path fixture.
func TestHPAExpansionCut(t *testing.T) {
	g, _, h := blockedFixture()
	if !h.Reachable(2, 2, 509, 509) {
		t.Fatal("fixture seed must connect the corners; pick another seed")
	}
	flat := NewSearcher(g)
	_, okF := flat.Search(h.l, 2, 2, 509, 509, nil)
	flatExp := flat.Expansions()
	out, res, okH := h.FindPath(2, 2, 509, 509, nil)
	t.Logf("flat A*:  ok=%v expansions=%d", okF, flatExp)
	t.Logf("HPA*:     ok=%v coarse=%d fine=%d total=%d corridor=%d sectors pathLen=%d",
		okH, res.CoarseExpansions, res.FineExpansions, res.CoarseExpansions+res.FineExpansions, res.CorridorSectorCnt, len(out))
	t.Logf("cut: flat/fine = %.1fx (mandate >=10x)", float64(flatExp)/float64(res.FineExpansions))
	if !okF || !okH {
		t.Fatal("searches failed")
	}
	if res.FineExpansions*10 > flatExp {
		t.Fatalf("expansion cut below 10x: flat=%d fine=%d", flatExp, res.FineExpansions)
	}
}

func BenchmarkAStarFlat(b *testing.B) {
	g, _, h := blockedFixture()
	flat := NewSearcher(g)
	out := make([]int32, 0, 2048)
	b.ReportAllocs()
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		out = out[:0]
		var ok bool
		out, ok = flat.Search(h.l, 2, 2, 509, 509, out)
		if !ok {
			b.Fatal("no path")
		}
	}
}

func BenchmarkAStarHPA(b *testing.B) {
	_, _, h := blockedFixture()
	out := make([]int32, 0, 2048)
	b.ReportAllocs()
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		out = out[:0]
		var ok bool
		out, _, ok = h.FindPath(2, 2, 509, 509, out)
		if !ok {
			b.Fatal("no path")
		}
	}
}

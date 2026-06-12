package path

import (
	"bytes"
	"strings"
	"testing"
)

var dirGlyphs = [9]rune{'.', '↑', '↗', '→', '↘', '↓', '↙', '←', '↖'}

// fieldDump renders a rect of a field as a glyph map (y printed top
// = north, so rows descend).
func fieldDump(f *FlowSet, slot int, r Rect) string {
	var b strings.Builder
	for y := r.Y + r.H - 1; y >= r.Y; y-- {
		b.WriteString("  ")
		for x := r.X; x < r.X+r.W; x++ {
			if !f.l.CenterClear(x, y) {
				b.WriteRune('#')
				continue
			}
			if idx(x, y) == f.slots[slot].goal {
				b.WriteRune('G')
				continue
			}
			b.WriteRune(dirGlyphs[f.Dir(slot, x, y)])
		}
		b.WriteByte('\n')
	}
	return b.String()
}

func flowFixture(open Rect) (*Grid, *DilatedSet, *FlowSet) {
	g := NewGrid()
	open.forEach(func(x, y int32) { g.OrFlags(x, y, Walkable) })
	d := NewDilatedSet(g, []LayerKey{{Required: Walkable, Blocked: OccupiedStatic}})
	d.RecomputeAll()
	return g, d, NewFlowSet(g, d.Layer(0))
}

// Field correctness + glyph dump: every walkable cell flows toward
// the goal; following directions from any cell reaches it.
func TestFlowFieldDirections(t *testing.T) {
	_, _, f := flowFixture(Rect{X: 0, Y: 0, W: 12, H: 12})
	slot, ok := f.Acquire(6, 6)
	if !ok {
		t.Fatal("acquire failed")
	}
	t.Logf("12x12 field toward (6,6):\n%s", fieldDump(f, slot, Rect{X: 0, Y: 0, W: 12, H: 12}))
	// walk from every corner; must reach the goal within 24 steps
	for _, c := range [][2]int32{{0, 0}, {11, 0}, {0, 11}, {11, 11}} {
		x, y := c[0], c[1]
		steps := 0
		for !(x == 6 && y == 6) {
			dx, dy := Step(f.Dir(slot, x, y))
			if dx == 0 && dy == 0 {
				t.Fatalf("dead direction at (%d,%d)", x, y)
			}
			x, y = x+dx, y+dy
			steps++
			if steps > 24 {
				t.Fatalf("walk from (%d,%d) did not converge", c[0], c[1])
			}
		}
		t.Logf("walk from (%d,%d) reached goal in %d steps", c[0], c[1], steps)
	}
}

// Determinism: the same goal generated twice (fresh slot each time)
// is byte-identical.
func TestFlowFieldByteIdentical(t *testing.T) {
	_, _, f := flowFixture(Rect{X: 0, Y: 0, W: 96, H: 96})
	slot1, _ := f.Acquire(80, 40)
	snap1 := make([]byte, len(f.slots[slot1].dirs))
	copy(snap1, f.slots[slot1].dirs)
	f.Release(slot1)
	// churn the scratch with a different goal, then regenerate
	mid, _ := f.Acquire(10, 10)
	f.Release(mid)
	slot2, _ := f.Acquire(80, 40)
	same := bytes.Equal(snap1, f.slots[slot2].dirs)
	diff := 0
	for i := range snap1 {
		if snap1[i] != f.slots[slot2].dirs[i] {
			diff++
		}
	}
	t.Logf("two generations of goal (80,40): byte-identical=%v (%d differing bytes of %d)", same, diff, len(snap1))
	if !same {
		t.Fatalf("field generation must be byte-deterministic: %d diffs", diff)
	}
}

// Edge 1: backend selection at the 39/40 boundary.
func TestFlowFieldBackendSelection(t *testing.T) {
	g, d, f := flowFixture(Rect{X: 0, Y: 0, W: 64, H: 64})
	_ = g
	h := NewHPA(f.g, d.Layer(0), NewSearcher(f.g))
	q := NewQueue(h, NewPathStore(64, runWaypointCap))
	p := NewProvider(NewSharer([]*Queue{q}), f)
	for _, n := range []int{1, 12, 39, 40, 41, 500} {
		b := p.SelectBackend(n)
		t.Logf("backend-selection: group=%3d -> %s", n, b)
		if (n >= DefaultFlowThreshold) != (b == BackendFlow) {
			t.Fatalf("group %d selected %s (threshold %d)", n, b, p.Threshold)
		}
	}
}

// Edge 2: a 5th distinct goal with 4 live slots recycles the least-
// recently-used slot deterministically.
func TestFlowFieldSlotRecycle(t *testing.T) {
	_, _, f := flowFixture(Rect{X: 0, Y: 0, W: 64, H: 64})
	goals := [][2]int32{{10, 10}, {20, 20}, {30, 30}, {40, 40}}
	for _, gpt := range goals {
		f.Acquire(gpt[0], gpt[1])
	}
	logSlots := func(when string) {
		for i := 0; i < FlowSlots; i++ {
			goal, live, use := f.SlotState(i)
			t.Logf("%s slot %d: goal=%d (%d,%d) live=%v lastUse=%d", when, i, goal, goal%GridSize, goal/GridSize, live, use)
		}
	}
	logSlots("before:")
	// touch slot 0's goal so slot 1 becomes the LRU
	f.Acquire(10, 10)
	slot, ok := f.Acquire(50, 50) // 5th distinct goal
	logSlots("after: ")
	t.Logf("5th goal (50,50) recycled slot %d (LRU was slot 1, goal (20,20))", slot)
	if !ok || slot != 1 {
		t.Fatalf("LRU recycle must pick slot 1: got %d", slot)
	}
	if g0, _, _ := f.SlotState(0); g0 != idx(10, 10) {
		t.Fatalf("slot 0 (recently touched) must survive")
	}
}

// Edge 3: a stamp inside a live field flips directions around the
// obstacle after re-integration.
func TestFlowFieldRestampReintegrates(t *testing.T) {
	_, d, f := flowFixture(Rect{X: 0, Y: 0, W: 16, H: 16})
	slot, _ := f.Acquire(13, 8)
	window := Rect{X: 4, Y: 5, W: 9, H: 7}
	before := fieldDump(f, slot, window)
	probe := f.Dir(slot, 6, 8) // cell west of the future wall, on the straight line
	// wall between (8,5)..(9,11): blocks the straight path to (13,8)
	wall := Rect{X: 8, Y: 5, W: 2, H: 7}
	d.StampStatic(wall)
	f.InvalidateAll()
	after := fieldDump(f, slot, window)
	probeAfter := f.Dir(slot, 6, 8)
	t.Logf("field toward (13,8) BEFORE stamp:\n%s", before)
	t.Logf("field toward (13,8) AFTER %+v stamped:\n%s", wall, after)
	t.Logf("probe (6,8): dir %c before -> %c after (must flip off the straight east line)",
		dirGlyphs[probe], dirGlyphs[probeAfter])
	if before == after {
		t.Fatal("re-integration must change the field")
	}
	if probe != 3 { // '→' east, straight at the goal
		t.Fatalf("pre-stamp probe must point east: %c", dirGlyphs[probe])
	}
	if probeAfter == 3 {
		t.Fatalf("post-stamp probe still points into the wall")
	}
}

// Edge 4: unreachable cells hold DirNone — the unit-side contract is
// fall-back to nearest-reachable.
func TestFlowFieldUnreachableNullDirection(t *testing.T) {
	g, _, _ := flowFixture(Rect{X: 0, Y: 0, W: 24, H: 24})
	// carve a sealed pocket: interior walkable, ring blocked
	for i := int32(16); i <= 22; i++ {
		g.ClearFlags(i, 16, Walkable)
		g.ClearFlags(i, 22, Walkable)
		g.ClearFlags(16, i, Walkable)
		g.ClearFlags(22, i, Walkable)
	}
	d := NewDilatedSet(g, []LayerKey{{Required: Walkable, Blocked: OccupiedStatic}})
	d.RecomputeAll()
	f := NewFlowSet(g, d.Layer(0))
	slot, _ := f.Acquire(4, 4)
	t.Logf("field toward (4,4), pocket at 17..21:\n%s", fieldDump(f, slot, Rect{X: 14, Y: 14, W: 10, H: 10}))
	for y := int32(17); y <= 21; y++ {
		for x := int32(17); x <= 21; x++ {
			if f.Dir(slot, x, y) != DirNone {
				t.Fatalf("pocket cell (%d,%d) must hold DirNone, got %d", x, y, f.Dir(slot, x, y))
			}
		}
	}
	if dx, dy := Step(DirNone); dx != 0 || dy != 0 {
		t.Fatalf("DirNone must step (0,0)")
	}
	t.Logf("all 25 pocket cells DirNone; Step(DirNone)=(0,0) — caller falls back to nearest-reachable")
}

// Full 512×512 open grid: the absolute worst case (262k cells).
func BenchmarkFlowFieldIntegrate(b *testing.B) {
	_, _, f := flowFixture(Rect{X: 0, Y: 0, W: GridSize, H: GridSize})
	slot, ok := f.Acquire(256, 256)
	if !ok {
		b.Fatal("acquire failed")
	}
	b.ReportAllocs()
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		f.integrate(&f.slots[slot])
	}
}

// The §7 reference map: 128×128 playable cells.
func BenchmarkFlowFieldIntegrateReferenceMap(b *testing.B) {
	_, _, f := flowFixture(Rect{X: 0, Y: 0, W: 128, H: 128})
	slot, ok := f.Acquire(64, 64)
	if !ok {
		b.Fatal("acquire failed")
	}
	b.ReportAllocs()
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		f.integrate(&f.slots[slot])
	}
}

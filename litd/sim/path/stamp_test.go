package path

import (
	"fmt"
	"testing"
)

func newStampFixture() (*Grid, *Stamper) {
	g := walkableGrid()
	d := NewDilatedSet(g, []LayerKey{groundKey(1)})
	return g, &Stamper{D: d, Paths: NewPathStore(8, 32)}
}

// Edge 1: placement overlapping an existing stamp is rejected and the
// grid is byte-identical before/after the attempt.
func TestStampOverlapRejectedGridUnchanged(t *testing.T) {
	g, s := newStampFixture()
	if !s.PlaceBuilding(Rect{3, 3, 3, 3}, 0, 0) {
		t.Fatalf("first placement must succeed")
	}
	before := dump(g, Rect{0, 0, 9, 9})
	t.Logf("before overlap attempt:\n%s", before)

	if s.PlaceBuilding(Rect{5, 5, 3, 3}, 0, 0) {
		t.Fatalf("overlapping placement must be rejected")
	}
	after := dump(g, Rect{0, 0, 9, 9})
	t.Logf("after rejected overlap attempt:\n%s", after)
	if before != after {
		t.Fatalf("rejected placement mutated the grid:\nbefore:\n%s\nafter:\n%s", before, after)
	}
}

// Edge 2: a stamp invalidates exactly the cached paths whose bbox
// intersects it; others stay live.
func TestStampInvalidatesIntersectingPaths(t *testing.T) {
	_, s := newStampFixture()

	crossing, buf, _ := s.Paths.Acquire(Rect{0, 10, 40, 1}) // runs through the build site
	buf = append(buf, 10*GridSize+0, 10*GridSize+39)
	s.Paths.SetWaypoints(crossing, buf)
	farAway, buf2, _ := s.Paths.Acquire(Rect{0, 100, 40, 1}) // 90 rows south
	buf2 = append(buf2, 100*GridSize+0)
	s.Paths.SetWaypoints(farAway, buf2)

	var doomed []PathID
	s.OnInvalidate = func(id PathID) { doomed = append(doomed, id) }

	if !s.PlaceBuilding(Rect{20, 8, 4, 4}, 0, 0) {
		t.Fatalf("placement must succeed")
	}
	t.Logf("invalidated path IDs: %v (crossing=%08x farAway=%08x)", doomed, uint32(crossing), uint32(farAway))
	t.Logf("crossing valid=%v  farAway valid=%v  live=%d", s.Paths.Valid(crossing), s.Paths.Valid(farAway), s.Paths.Live())
	if len(doomed) != 1 || doomed[0] != crossing {
		t.Fatalf("exactly the crossing path must be invalidated, got %v", doomed)
	}
	if s.Paths.Valid(crossing) || !s.Paths.Valid(farAway) {
		t.Fatalf("wrong survivor: crossing=%v farAway=%v", s.Paths.Valid(crossing), s.Paths.Valid(farAway))
	}
}

// Edge 3: cancel mid-build restores the cells exactly — and the
// dilated layer with them.
func TestStampCancelRestoresCells(t *testing.T) {
	g, s := newStampFixture()
	win := Rect{0, 0, 9, 9}
	layer := s.D.Layer(0)

	before := dump(g, win) + "layer:\n" + dumpLayer(layer, win)
	t.Logf("before placement:\n%s", before)

	site := Rect{3, 3, 3, 3}
	if !s.PlaceBuilding(site, 0, 0) {
		t.Fatalf("placement must succeed")
	}
	during := dump(g, win) + "layer:\n" + dumpLayer(layer, win)
	t.Logf("during build:\n%s", during)
	if during == before {
		t.Fatalf("stamp must change grid and layer")
	}

	s.RemoveBuilding(site) // cancel
	after := dump(g, win) + "layer:\n" + dumpLayer(layer, win)
	t.Logf("after cancel:\n%s", after)
	if after != before {
		t.Fatalf("cancel must restore exactly:\nbefore:\n%s\nafter:\n%s", before, after)
	}
}

// Edge 4: a builder standing inside its own footprint gets a move-out
// order to the nearest free cell outside, before the stamp lands.
func TestStampBuilderMoveOut(t *testing.T) {
	_, s := newStampFixture()
	var order string
	stampedWhenOrdered := false
	s.OnMoveOut = func(fx, fy, tx, ty int32) {
		order = fmt.Sprintf("move-out: builder (%d,%d) -> (%d,%d)", fx, fy, tx, ty)
		stampedWhenOrdered = s.D.g.FlagsAt(fx, fy)&OccupiedStatic != 0
	}
	site := Rect{4, 4, 3, 3}
	if !s.PlaceBuilding(site, 5, 5) { // builder dead center
		t.Fatalf("placement must succeed")
	}
	t.Logf("%s (issued before stamp: %v)", order, !stampedWhenOrdered)
	if order != "move-out: builder (5,5) -> (3,3)" {
		t.Fatalf("unexpected move-out order: %q (ring scan must pick the deterministic first free cell)", order)
	}
	if stampedWhenOrdered {
		t.Fatalf("move-out order must be issued BEFORE the footprint stamps")
	}

	// builder outside the footprint: no order
	order = ""
	if !s.PlaceBuilding(Rect{20, 20, 2, 2}, 0, 0) {
		t.Fatalf("second placement must succeed")
	}
	if order != "" {
		t.Fatalf("no move-out order expected for a builder outside the footprint, got %q", order)
	}
}

// Builder with NO vacant cell anywhere outside the footprint →
// placement refused before any mutation. (The move-out target is the
// nearest FREE cell; whether the builder can actually reach it is the
// move order's problem once pathing lands — #108.)
func TestStampBuilderNoVacantCellRefused(t *testing.T) {
	g := NewGrid() // entire map unwalkable...
	site := Rect{11, 11, 3, 3}
	site.forEach(func(x, y int32) { g.SetFlags(x, y, Walkable|Buildable) }) // ...except the site
	s := &Stamper{D: NewDilatedSet(g, []LayerKey{groundKey(1)}), Paths: NewPathStore(8, 32)}

	before := dump(g, Rect{9, 9, 7, 7})
	t.Logf("island site, builder inside, nowhere to vacate to:\n%s", before)
	if s.PlaceBuilding(site, 12, 12) {
		t.Fatalf("placement with no vacate cell must be refused")
	}
	if d := dump(g, Rect{9, 9, 7, 7}); d != before {
		t.Fatalf("refused placement mutated the grid")
	}
}

// Stale PathID discipline: released IDs go invalid, slot reuse bumps
// the generation, double release is a quiet no-op.
func TestStampPathStoreStaleIDs(t *testing.T) {
	ps := NewPathStore(2, 4)
	a, _, ok := ps.Acquire(Rect{0, 0, 1, 1})
	if !ok {
		t.Fatal("acquire failed")
	}
	if !ps.Release(a) || ps.Release(a) {
		t.Fatalf("first release true, second (stale) false")
	}
	b, _, _ := ps.Acquire(Rect{0, 0, 1, 1})
	if ps.Valid(a) {
		t.Fatalf("stale ID must not validate after slot reuse")
	}
	if a.Slot() == b.Slot() && a.Gen() == b.Gen() {
		t.Fatalf("reused slot must carry a new generation")
	}
	// exhaustion is a gameplay outcome
	c, _, _ := ps.Acquire(Rect{0, 0, 1, 1})
	if _, _, ok := ps.Acquire(Rect{0, 0, 1, 1}); ok {
		t.Fatalf("acquire past capacity must fail")
	}
	_ = c
}

// The R-GC gate: stamp + unstamp with live paths to invalidate
// allocates zero.
func BenchmarkStampUnstampInvalidate(b *testing.B) {
	_, s := newStampFixture()
	noop := func(PathID) {}
	s.OnInvalidate = noop
	b.ReportAllocs()
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		if id, buf, ok := s.Paths.Acquire(Rect{30, 30, 10, 10}); ok {
			buf = append(buf, int32(i))
			s.Paths.SetWaypoints(id, buf)
		}
		site := Rect{32, 32, 3, 3}
		if !s.PlaceBuilding(site, 0, 0) {
			b.Fatal("placement failed")
		}
		s.RemoveBuilding(site)
	}
}

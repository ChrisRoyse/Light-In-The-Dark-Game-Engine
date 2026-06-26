package litd

import (
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
)

// idsOf renders a unit slice as its entity-index sequence for SoT logs.
func idsOf(us []Unit) []uint32 {
	out := make([]uint32, len(us))
	for i, u := range us {
		out[i] = u.id.Index()
	}
	return out
}

// TestQueries — spatial queries return id-ordered snapshots, deterministic
// across runs, with kill-during-iteration safe and empty queries non-nil.
// SoT: the entity-id sequences printed for each query.
func TestQueries(t *testing.T) {
	scene := func() ([]uint32, []uint32, []uint32) {
		w := sim.NewWorld(sim.Caps{Units: 16})
		g := newGame(w)
		// cluster near origin + one far away.
		unitAt(t, w, g, 0, 100, 100)
		unitAt(t, w, g, 0, 150, 150)
		unitAt(t, w, g, 0, 120, 80)
		unitAt(t, w, g, 0, 5000, 5000) // far
		stepN(w, 1)                    // populate spatial buckets

		inRange := idsOf(g.UnitsInRange(Vec2{100, 100}, 200, nil))
		inRect := idsOf(g.UnitsIn(NewRect(Vec2{0, 0}, Vec2{300, 300}), nil))
		all := idsOf(g.AllUnits(nil))
		return inRange, inRect, all
	}
	r1a, r1b, r1c := scene()
	r2a, r2b, r2c := scene()
	t.Logf("FSV run1: inRange=%v inRect=%v all=%v", r1a, r1b, r1c)
	t.Logf("FSV run2: inRange=%v inRect=%v all=%v", r2a, r2b, r2c)

	// id-ordered ascending.
	for _, seq := range [][]uint32{r1a, r1b, r1c} {
		for i := 1; i < len(seq); i++ {
			if seq[i] <= seq[i-1] {
				t.Fatalf("not ascending id order: %v", seq)
			}
		}
	}
	// the 3 clustered units are in range/rect; the far one is not.
	if len(r1a) != 3 || len(r1b) != 3 {
		t.Fatalf("range/rect should find 3 clustered units, got %v / %v", r1a, r1b)
	}
	if len(r1c) != 4 {
		t.Fatalf("AllUnits should find 4, got %v", r1c)
	}
	// determinism.
	if !eqU32(r1a, r2a) || !eqU32(r1b, r2b) || !eqU32(r1c, r2c) {
		t.Fatalf("nondeterministic query order across runs")
	}

	// Empty query over a region with nothing: non-nil, length 0.
	w := sim.NewWorld(sim.Caps{Units: 4})
	g := newGame(w)
	unitAt(t, w, g, 0, 100, 100)
	stepN(w, 1)
	empty := g.UnitsIn(NewRect(Vec2{9000, 9000}, Vec2{9500, 9500}), nil)
	t.Logf("FSV empty query: len=%d nil=%v", len(empty), empty == nil)
	if empty == nil || len(empty) != 0 {
		t.Fatalf("empty query must be non-nil empty slice, got nil=%v len=%d", empty == nil, len(empty))
	}

	// Kill during iteration: slice length unchanged, handles go invalid.
	w2 := sim.NewWorld(sim.Caps{Units: 8})
	g2 := newGame(w2)
	unitAt(t, w2, g2, 0, 100, 100)
	unitAt(t, w2, g2, 0, 110, 110)
	stepN(w2, 1)
	snap := g2.UnitsInRange(Vec2{100, 100}, 200, nil)
	before := len(snap)
	validBefore := snap[0].Valid()
	snap[0].Kill()
	stepN(w2, 1) // process the kill
	t.Logf("FSV kill-in-iteration: len before=%d after=%d ; handle valid before=%v after=%v",
		before, len(snap), validBefore, snap[0].Valid())
	if len(snap) != before {
		t.Fatalf("snapshot length changed after kill: %d -> %d", before, len(snap))
	}
	if snap[0].Valid() {
		t.Fatalf("killed handle still valid (snapshot should not auto-update but handle must invalidate)")
	}

	// filter: only player-0 owned (all are) vs impossible owner.
	none := g2.UnitsInRange(Vec2{100, 100}, 200, func(v UnitView) bool { return v.OwnerPlayer() == 5 })
	t.Logf("FSV filter owner==5: len=%d (want 0)", len(none))
	if len(none) != 0 {
		t.Fatalf("filter should exclude all, got %d", len(none))
	}
}

// TestUnitSet — insertion-ordered, dedup on add, order-preserving remove,
// reinsert appends at end, Compact drops dead. SoT: the iteration order.
func TestUnitSet(t *testing.T) {
	w := sim.NewWorld(sim.Caps{Units: 8})
	g := newGame(w)
	a, _ := unitAt(t, w, g, 0, 10, 10)
	b, _ := unitAt(t, w, g, 0, 20, 20)
	c, _ := unitAt(t, w, g, 0, 30, 30)

	s := g.NewUnitSet()
	s.Add(a)
	s.Add(b)
	s.Add(c)
	s.Add(b) // dup ignored
	t.Logf("FSV after adds (a,b,c,b-dup): order=%v count=%d", idsOf(s.Units()), s.Count())
	if s.Count() != 3 {
		t.Fatalf("dup add changed count: %d", s.Count())
	}
	if got := idsOf(s.Units()); !eqU32(got, []uint32{a.id.Index(), b.id.Index(), c.id.Index()}) {
		t.Fatalf("insertion order wrong: %v", got)
	}

	s.Remove(b)
	t.Logf("FSV after remove(b): order=%v contains(b)=%v", idsOf(s.Units()), s.Contains(b))
	if s.Contains(b) || !eqU32(idsOf(s.Units()), []uint32{a.id.Index(), c.id.Index()}) {
		t.Fatalf("remove broke order: %v", idsOf(s.Units()))
	}

	s.Add(b) // reinsert -> goes to end
	t.Logf("FSV after reinsert(b): order=%v", idsOf(s.Units()))
	if !eqU32(idsOf(s.Units()), []uint32{a.id.Index(), c.id.Index(), b.id.Index()}) {
		t.Fatalf("reinsert order wrong: %v", idsOf(s.Units()))
	}

	// Compact drops dead members, keeps order.
	c.Kill()
	stepN(w, 1)
	s.Compact()
	t.Logf("FSV after kill(c)+Compact: order=%v count=%d", idsOf(s.Units()), s.Count())
	if s.Count() != 2 || !eqU32(idsOf(s.Units()), []uint32{a.id.Index(), b.id.Index()}) {
		t.Fatalf("Compact wrong: %v", idsOf(s.Units()))
	}
}

// BenchmarkAppendUnitsIn — the pooled zero-alloc query twin.
func BenchmarkAppendUnitsIn(b *testing.B) {
	w := sim.NewWorld(sim.Caps{Units: 64})
	g := newGame(w)
	for i := 0; i < 20; i++ {
		id, ok := w.CreateUnit(fixed.Vec2{X: fromFloat(float64(100 + i*5)), Y: fromFloat(100)}, fixed.Angle(0))
		if !ok {
			b.Fatal("spawn failed")
		}
		w.Owners.Add(w.Ents, id, 0, 0, 0)
	}
	stepN(w, 1)
	rect := NewRect(Vec2{0, 0}, Vec2{500, 500})
	dst := make([]Unit, 0, 64)
	b.ResetTimer()
	b.ReportAllocs()
	for i := 0; i < b.N; i++ {
		dst = g.AppendUnitsIn(dst[:0], rect, nil)
	}
	if len(dst) == 0 {
		b.Fatal("query found nothing")
	}
}

func eqU32(a, b []uint32) bool {
	if len(a) != len(b) {
		return false
	}
	for i := range a {
		if a[i] != b[i] {
			return false
		}
	}
	return true
}

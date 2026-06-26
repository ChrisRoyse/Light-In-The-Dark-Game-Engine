package sim

import (
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
)

// #582 — MoverStore pool + spline arena. SoT = the store columns read
// directly after each op.

func TestMoverCreateResolveCancel(t *testing.T) {
	s := NewMoverStore(8, 64)
	if s.Cap() != 8 || s.WaypointCap() != 64 {
		t.Fatalf("caps wrong: movers=%d wp=%d", s.Cap(), s.WaypointCap())
	}
	id := s.Create(MoverSpec{Kind: MoverHoming, Speed: 5 * fixed.One, Owner: makeEntityID(3, 1)})
	if id == 0 {
		t.Fatal("Create returned invalid")
	}
	r, ok := s.resolve(id)
	if !ok {
		t.Fatal("resolve failed")
	}
	// SoT: columns hold what the spec set.
	if MoverKind(s.Kind[r]) != MoverHoming || s.Speed[r] != 5*fixed.One || s.Owner[r] != makeEntityID(3, 1) {
		t.Fatalf("columns wrong: kind=%d speed=%d owner=%v", s.Kind[r], s.Speed[r], s.Owner[r])
	}
	if s.Count() != 1 {
		t.Fatalf("count = %d", s.Count())
	}
	if !s.Cancel(id) || s.Alive(id) || s.Count() != 0 {
		t.Fatal("cancel failed")
	}
	if s.Cancel(id) {
		t.Fatal("double cancel returned true")
	}
}

func TestMoverGenerationReuse(t *testing.T) {
	s := NewMoverStore(4, 16)
	a := s.Create(MoverSpec{})
	s.Cancel(a)
	b := s.Create(MoverSpec{})
	if a.Index() != b.Index() {
		t.Fatal("slot not reused")
	}
	if a == b || s.Alive(a) {
		t.Fatal("stale handle still resolves after gen bump")
	}
}

func TestMoverSplineArena(t *testing.T) {
	s := NewMoverStore(4, 8)
	pts := []fixed.Vec2{{X: 1, Y: 2}, {X: 3, Y: 4}, {X: 5, Y: 6}}
	start, n, ok := s.AddWaypoints(pts)
	if !ok || n != 3 {
		t.Fatalf("AddWaypoints ok=%v n=%d", ok, n)
	}
	// SoT: arena holds the points at the span.
	for i := int32(0); i < n; i++ {
		if s.Waypoint(start+i) != pts[i] {
			t.Fatalf("waypoint %d = %v, want %v", i, s.Waypoint(start+i), pts[i])
		}
	}
	// Overflow: 8-slot arena, 3 used, 6 more must fail cleanly.
	if _, _, ok := s.AddWaypoints(make([]fixed.Vec2, 6)); ok {
		t.Fatal("arena overflow accepted")
	}
	if s.Dropped != 1 {
		t.Fatalf("Dropped = %d, want 1", s.Dropped)
	}
}

func TestMoverExhaustion(t *testing.T) {
	const cap = 3
	s := NewMoverStore(cap, 8)
	for i := 0; i < cap; i++ {
		if s.Create(MoverSpec{}) == 0 {
			t.Fatalf("create %d failed below cap", i)
		}
	}
	if s.Create(MoverSpec{}) != 0 {
		t.Fatal("create past cap returned a handle")
	}
	if s.Dropped != 1 {
		t.Fatalf("Dropped = %d, want 1", s.Dropped)
	}
}

func TestMoverCancelOwnedBy(t *testing.T) {
	s := NewMoverStore(8, 16)
	victim := makeEntityID(7, 1)
	survivor := makeEntityID(8, 1)
	m1 := s.Create(MoverSpec{Owner: victim})
	m2 := s.Create(MoverSpec{Owner: survivor})
	m3 := s.Create(MoverSpec{Owner: victim})
	s.CancelOwnedBy([]EntityID{victim})
	if s.Alive(m1) || s.Alive(m3) {
		t.Fatal("victim-owned movers survived")
	}
	if !s.Alive(m2) {
		t.Fatal("survivor mover wrongly cancelled")
	}
	if s.Count() != 1 {
		t.Fatalf("count = %d, want 1", s.Count())
	}
}

func TestMoverZeroAlloc(t *testing.T) {
	s := NewMoverStore(64, 16)
	avg := testing.AllocsPerRun(1000, func() {
		id := s.Create(MoverSpec{Kind: MoverLinear, Speed: fixed.One})
		s.Cancel(id)
	})
	if avg != 0 {
		t.Fatalf("create/cancel churn allocated %.2f objs/op, want 0", avg)
	}
}

func TestNewWorldWiresMovers(t *testing.T) {
	w := NewWorld(Caps{})
	if w.Movers == nil || w.Movers.Cap() != EngineCaps.Movers || w.Movers.WaypointCap() != EngineCaps.MoverWaypoints {
		t.Fatalf("Movers not wired: %v", w.Movers)
	}
}

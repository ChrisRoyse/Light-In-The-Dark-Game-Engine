package sim

import "testing"

// #562 — set algebra. SoT = dst's Members span after each op.

func mkGroup(s *GroupStore, es ...uint32) GroupID {
	g := s.CreateGroup()
	for _, e := range es {
		s.GroupAdd(g, ent(e))
	}
	return g
}

func TestGroupUnion(t *testing.T) {
	s := NewGroupStore(8, 512)
	a := mkGroup(s, 1, 2, 3)
	b := mkGroup(s, 3, 4)
	dst := s.CreateGroup()
	if !s.GroupUnion(dst, a, b) {
		t.Fatal("union returned false")
	}
	got := members(s, dst)
	want := []EntityID{ent(1), ent(2), ent(3), ent(4)} // a order, then b's new
	if len(got) != 4 {
		t.Fatalf("union size = %d (%v)", len(got), got)
	}
	for i := range want {
		if got[i] != want[i] {
			t.Fatalf("union = %v, want %v", got, want)
		}
	}
}

func TestGroupIntersect(t *testing.T) {
	s := NewGroupStore(8, 512)
	a := mkGroup(s, 1, 2, 3, 4)
	b := mkGroup(s, 2, 4, 9)
	dst := s.CreateGroup()
	s.GroupIntersect(dst, a, b)
	got := members(s, dst)
	want := []EntityID{ent(2), ent(4)} // a's order, members also in b
	if len(got) != 2 || got[0] != want[0] || got[1] != want[1] {
		t.Fatalf("intersect = %v, want %v", got, want)
	}
}

func TestGroupDifference(t *testing.T) {
	s := NewGroupStore(8, 512)
	a := mkGroup(s, 1, 2, 3, 4)
	b := mkGroup(s, 2, 4)
	dst := s.CreateGroup()
	s.GroupDifference(dst, a, b)
	got := members(s, dst)
	want := []EntityID{ent(1), ent(3)}
	if len(got) != 2 || got[0] != want[0] || got[1] != want[1] {
		t.Fatalf("difference = %v, want %v", got, want)
	}
}

func TestGroupCopy(t *testing.T) {
	s := NewGroupStore(8, 512)
	a := mkGroup(s, 5, 6, 7)
	dst := mkGroup(s, 99) // pre-populated; copy must clear it first
	s.GroupCopy(dst, a)
	got := members(s, dst)
	want := []EntityID{ent(5), ent(6), ent(7)}
	if len(got) != 3 {
		t.Fatalf("copy size = %d (%v)", len(got), got)
	}
	for i := range want {
		if got[i] != want[i] {
			t.Fatalf("copy = %v, want %v", got, want)
		}
	}
}

func TestGroupAlgebraRejectsAliasing(t *testing.T) {
	s := NewGroupStore(8, 512)
	a := mkGroup(s, 1, 2)
	b := mkGroup(s, 3)
	// dst aliasing a source must be a no-op (would corrupt a mid-read).
	if s.GroupUnion(a, a, b) {
		t.Fatal("union with dst==a returned true")
	}
	// a is untouched.
	if s.GroupCount(a) != 2 {
		t.Fatalf("a corrupted by aliasing union: count=%d", s.GroupCount(a))
	}
}

func TestGroupAlgebraZeroAlloc(t *testing.T) {
	s := NewGroupStore(8, 512)
	a := mkGroup(s, 1, 2, 3)
	b := mkGroup(s, 2, 3, 4)
	dst := s.CreateGroup()
	avg := testing.AllocsPerRun(1000, func() {
		s.GroupUnion(dst, a, b)
		s.GroupIntersect(dst, a, b)
		s.GroupDifference(dst, a, b)
	})
	if avg != 0 {
		t.Fatalf("algebra churn allocated %.2f objs/op, want 0", avg)
	}
}

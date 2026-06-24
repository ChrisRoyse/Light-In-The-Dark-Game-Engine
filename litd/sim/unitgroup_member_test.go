package sim

import "testing"

// #561 — membership ops. SoT = the Members span + Len column, read
// directly after each op.

func ent(i uint32) EntityID { return makeEntityID(i, 1) }

func members(s *GroupStore, id GroupID) []EntityID {
	row, _ := s.resolve(id)
	out := make([]EntityID, 0, s.Len[row])
	for i := int32(0); i < s.Len[row]; i++ {
		out = append(out, s.Members[s.Start[row]+i])
	}
	return out
}

func TestGroupAddUniqueInsertionOrder(t *testing.T) {
	s := NewGroupStore(4, 256)
	g := s.CreateGroup()
	for _, e := range []uint32{10, 20, 30} {
		if !s.GroupAdd(g, ent(e)) {
			t.Fatalf("add %d failed", e)
		}
	}
	// Duplicate add is a no-op that still reports membership.
	if !s.GroupAdd(g, ent(20)) {
		t.Fatal("re-add returned false")
	}
	if s.GroupCount(g) != 3 {
		t.Fatalf("count = %d, want 3 (dup not added)", s.GroupCount(g))
	}
	got := members(s, g)
	want := []EntityID{ent(10), ent(20), ent(30)}
	for i := range want {
		if got[i] != want[i] {
			t.Fatalf("members = %v, want insertion order %v", got, want)
		}
	}
	if !s.GroupContains(g, ent(20)) || s.GroupContains(g, ent(99)) {
		t.Fatal("Contains wrong")
	}
	if s.GroupFirst(g) != ent(10) {
		t.Fatalf("First = %v, want ent(10)", s.GroupFirst(g))
	}
}

func TestGroupRemoveSwap(t *testing.T) {
	s := NewGroupStore(4, 256)
	g := s.CreateGroup()
	for _, e := range []uint32{1, 2, 3, 4} {
		s.GroupAdd(g, ent(e))
	}
	// Swap-remove the middle: last (4) fills the hole.
	if !s.GroupRemove(g, ent(2)) {
		t.Fatal("remove failed")
	}
	got := members(s, g)
	want := []EntityID{ent(1), ent(4), ent(3)}
	if len(got) != 3 {
		t.Fatalf("count after remove = %d", len(got))
	}
	for i := range want {
		if got[i] != want[i] {
			t.Fatalf("swap-remove = %v, want %v", got, want)
		}
	}
	if s.GroupRemove(g, ent(99)) {
		t.Fatal("remove of absent returned true")
	}
}

func TestGroupRemoveOrderedStable(t *testing.T) {
	s := NewGroupStore(4, 256)
	g := s.CreateGroup()
	for _, e := range []uint32{1, 2, 3, 4} {
		s.GroupAdd(g, ent(e))
	}
	if !s.GroupRemoveOrdered(g, ent(2)) {
		t.Fatal("ordered remove failed")
	}
	got := members(s, g)
	want := []EntityID{ent(1), ent(3), ent(4)} // order preserved
	for i := range want {
		if got[i] != want[i] {
			t.Fatalf("ordered-remove = %v, want %v", got, want)
		}
	}
}

func TestGroupClearAndEach(t *testing.T) {
	s := NewGroupStore(4, 256)
	g := s.CreateGroup()
	for _, e := range []uint32{5, 6, 7} {
		s.GroupAdd(g, ent(e))
	}
	var seen []EntityID
	s.GroupEach(g, func(e EntityID) { seen = append(seen, e) })
	if len(seen) != 3 || seen[0] != ent(5) || seen[2] != ent(7) {
		t.Fatalf("Each visited %v, want [5 6 7] in order", seen)
	}
	s.GroupClear(g)
	if s.GroupCount(g) != 0 {
		t.Fatalf("count after clear = %d", s.GroupCount(g))
	}
}

func TestGroupSpanFullDropsMember(t *testing.T) {
	// Arena of 8; one group can now grow to fill it (#613), and only arena
	// exhaustion drops a member.
	s := NewGroupStore(2, 8)
	g := s.CreateGroup()
	for i := uint32(0); i < 8; i++ {
		if !s.GroupAdd(g, ent(100+i)) {
			t.Fatalf("add %d failed below arena capacity", i)
		}
	}
	// 9th add cannot fit the 8-slot arena.
	if s.GroupAdd(g, ent(999)) {
		t.Fatal("add past arena capacity returned true")
	}
	if s.DroppedMembers != 1 {
		t.Fatalf("DroppedMembers = %d, want 1", s.DroppedMembers)
	}
	if s.GroupCount(g) != 8 {
		t.Fatalf("count = %d, want 8 (arena-full overflow dropped)", s.GroupCount(g))
	}
}

func TestGroupOpsStaleHandleNoOp(t *testing.T) {
	s := NewGroupStore(4, 256)
	g := s.CreateGroup()
	s.GroupAdd(g, ent(1))
	s.DestroyGroup(g)
	// All ops on the stale handle are safe no-ops.
	if s.GroupAdd(g, ent(2)) || s.GroupRemove(g, ent(1)) || s.GroupContains(g, ent(1)) {
		t.Fatal("stale-handle op returned true")
	}
	if s.GroupCount(g) != 0 || s.GroupFirst(g) != 0 {
		t.Fatal("stale-handle query non-zero")
	}
	s.GroupClear(g)        // must not panic
	s.GroupEach(g, func(EntityID) { t.Fatal("Each visited a stale group") })
}

func TestGroupMembershipZeroAlloc(t *testing.T) {
	s := NewGroupStore(8, 512)
	g := s.CreateGroup()
	avg := testing.AllocsPerRun(1000, func() {
		s.GroupAdd(g, ent(1))
		s.GroupContains(g, ent(1))
		s.GroupRemove(g, ent(1))
	})
	if avg != 0 {
		t.Fatalf("membership churn allocated %.2f objs/op, want 0", avg)
	}
}

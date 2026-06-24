package sim

import "testing"

// #560 — GroupStore pool + shared-arena foundation. Asserts against the
// store columns (SoT): handle validity, generation staleness, LIFO
// reuse, fixed-span layout, exhaustion, and bookkeeping.

func TestGroupStoreCreateResolveDestroy(t *testing.T) {
	s := NewGroupStore(8, 512)
	if s.GroupCap() != 8 {
		t.Fatalf("GroupCap = %d, want 8", s.GroupCap())
	}
	if s.MembersPerGroup() != 64 {
		t.Fatalf("MembersPerGroup = %d, want 64 (512/8)", s.MembersPerGroup())
	}
	if s.Count() != 0 {
		t.Fatalf("fresh Count = %d", s.Count())
	}

	id := s.CreateGroup()
	if id == 0 {
		t.Fatal("CreateGroup returned invalid sentinel")
	}
	row, ok := s.resolve(id)
	if !ok || row == 0 {
		t.Fatalf("resolve(id) = (%d,%v)", row, ok)
	}
	if !s.live[row] || s.Len[row] != 0 {
		t.Fatalf("new group not live/empty: live=%v len=%d", s.live[row], s.Len[row])
	}
	// Fixed span: slot r owns Members[(r-1)*perCap : ...], Cap = perCap.
	if s.Start[row] != (row-1)*s.perCap || s.Cap[row] != s.perCap {
		t.Fatalf("span layout wrong: Start=%d want %d, Cap=%d want %d",
			s.Start[row], (row-1)*s.perCap, s.Cap[row], s.perCap)
	}
	if s.Count() != 1 {
		t.Fatalf("Count after create = %d", s.Count())
	}

	if !s.DestroyGroup(id) {
		t.Fatal("DestroyGroup of live group returned false")
	}
	if s.Alive(id) {
		t.Fatal("group alive after destroy")
	}
	if s.Count() != 0 {
		t.Fatalf("Count after destroy = %d", s.Count())
	}
	if s.DestroyGroup(id) {
		t.Fatal("double destroy returned true")
	}
}

func TestGroupStoreGenerationReuse(t *testing.T) {
	s := NewGroupStore(4, 256)
	a := s.CreateGroup()
	rowA := int32(a.Index())
	s.DestroyGroup(a)
	b := s.CreateGroup()
	if b.Index() != uint32(rowA) {
		t.Fatalf("slot not reused (LIFO): a=%d b=%d", rowA, b.Index())
	}
	if a == b {
		t.Fatal("reused slot produced identical handle — generation did not advance")
	}
	if s.Alive(a) {
		t.Fatal("old handle resolves after reuse")
	}
	if !s.Alive(b) {
		t.Fatal("new handle does not resolve")
	}
}

func TestGroupStoreExhaustion(t *testing.T) {
	const cap = 3
	s := NewGroupStore(cap, cap*8)
	var ids []GroupID
	for i := 0; i < cap; i++ {
		id := s.CreateGroup()
		if id == 0 {
			t.Fatalf("create %d failed below capacity", i)
		}
		ids = append(ids, id)
	}
	if id := s.CreateGroup(); id != 0 {
		t.Fatalf("create past capacity returned %d, want 0", id)
	}
	if s.DroppedGroups != 1 {
		t.Fatalf("DroppedGroups = %d, want 1", s.DroppedGroups)
	}
	s.DestroyGroup(ids[0])
	if s.CreateGroup() == 0 {
		t.Fatal("create failed after freeing a slot")
	}
}

func TestGroupStoreStaleHandles(t *testing.T) {
	s := NewGroupStore(4, 256)
	if _, ok := s.resolve(0); ok {
		t.Fatal("GroupID(0) resolved")
	}
	if _, ok := s.resolve(makeGroupID(9999, 0)); ok {
		t.Fatal("out-of-range handle resolved")
	}
	if s.Alive(makeGroupID(2, 0)) {
		t.Fatal("never-allocated slot alive")
	}
}

func TestGroupStoreZeroAlloc(t *testing.T) {
	s := NewGroupStore(64, 64*64)
	avg := testing.AllocsPerRun(1000, func() {
		id := s.CreateGroup()
		if id == 0 {
			t.Fatal("create failed mid-churn")
		}
		s.DestroyGroup(id)
	})
	if avg != 0 {
		t.Fatalf("create/destroy churn allocated %.2f objs/op, want 0", avg)
	}
}

func TestNewWorldWiresGroupStore(t *testing.T) {
	w := NewWorld(Caps{})
	if w.Groups == nil {
		t.Fatal("NewWorld did not construct w.Groups")
	}
	if w.Groups.GroupCap() != EngineCaps.UnitGroups {
		t.Fatalf("default group cap = %d, want %d", w.Groups.GroupCap(), EngineCaps.UnitGroups)
	}
	if len(w.Groups.Members) != EngineCaps.GroupMembers {
		t.Fatalf("default member arena = %d, want %d", len(w.Groups.Members), EngineCaps.GroupMembers)
	}
	w2 := NewWorld(Caps{UnitGroups: 16, GroupMembers: 256})
	if w2.Groups.GroupCap() != 16 {
		t.Fatalf("requested group cap 16 -> %d", w2.Groups.GroupCap())
	}
}

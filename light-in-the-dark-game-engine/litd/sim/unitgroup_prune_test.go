package sim

import "testing"

// #564 — dead-member auto-prune. SoT = group Members span after a kill +
// step through the cleanup phase.

func TestGroupPruneEntitiesStable(t *testing.T) {
	s := NewGroupStore(4, 256)
	g := s.CreateGroup()
	for _, e := range []uint32{1, 2, 3, 4, 5} {
		s.GroupAdd(g, ent(e))
	}
	// Kill the 2nd and 4th — survivors keep insertion order.
	n := s.PruneEntities([]EntityID{ent(2), ent(4)})
	if n != 2 {
		t.Fatalf("pruned %d, want 2", n)
	}
	got := members(s, g)
	want := []EntityID{ent(1), ent(3), ent(5)}
	if len(got) != 3 {
		t.Fatalf("after prune %v, want %v", got, want)
	}
	for i := range want {
		if got[i] != want[i] {
			t.Fatalf("prune not stable: %v, want %v", got, want)
		}
	}
}

func TestGroupPruneAcrossManyGroups(t *testing.T) {
	s := NewGroupStore(8, 512)
	g1 := s.CreateGroup()
	g2 := s.CreateGroup()
	s.GroupAdd(g1, ent(10))
	s.GroupAdd(g1, ent(20))
	s.GroupAdd(g2, ent(20)) // shared member
	s.GroupAdd(g2, ent(30))
	if n := s.PruneEntities([]EntityID{ent(20)}); n != 2 {
		t.Fatalf("pruned %d across 2 groups, want 2", n)
	}
	if s.GroupContains(g1, ent(20)) || s.GroupContains(g2, ent(20)) {
		t.Fatal("ent(20) survived prune in some group")
	}
	if s.GroupCount(g1) != 1 || s.GroupCount(g2) != 1 {
		t.Fatalf("counts after prune g1=%d g2=%d, want 1/1", s.GroupCount(g1), s.GroupCount(g2))
	}
}

func TestGroupPruneNoopEmptyDead(t *testing.T) {
	s := NewGroupStore(4, 256)
	g := s.CreateGroup()
	s.GroupAdd(g, ent(1))
	if n := s.PruneEntities(nil); n != 0 {
		t.Fatalf("prune(nil) removed %d", n)
	}
	if s.GroupCount(g) != 1 {
		t.Fatal("prune(nil) mutated group")
	}
}

func TestGroupPruneZeroAlloc(t *testing.T) {
	s := NewGroupStore(8, 512)
	g := s.CreateGroup()
	dead := []EntityID{ent(2), ent(4)}
	avg := testing.AllocsPerRun(500, func() {
		s.Len[1] = 0
		for _, e := range []uint32{1, 2, 3, 4, 5} {
			s.GroupAdd(g, ent(e))
		}
		s.PruneEntities(dead)
	})
	if avg != 0 {
		t.Fatalf("prune allocated %.2f objs/op, want 0", avg)
	}
}

// End-to-end: a unit in a group, killed via the kill path, is gone from
// the group after the cleanup-phase step.
func TestGroupPruneOnUnitDeath(t *testing.T) {
	w, u := queryWorld(t)
	g := w.Groups.CreateGroup()
	w.Groups.GroupAdd(g, u[0])
	w.Groups.GroupAdd(g, u[1])
	if w.Groups.GroupCount(g) != 2 {
		t.Fatalf("setup count = %d", w.Groups.GroupCount(g))
	}
	w.KillUnit(u[0]) // queue death
	w.Step()     // runs phase 7 cleanup → prune
	if w.Groups.GroupContains(g, u[0]) {
		t.Fatal("killed unit still in group after cleanup step")
	}
	if w.Groups.GroupCount(g) != 1 || !w.Groups.GroupContains(g, u[1]) {
		t.Fatalf("survivor lost: count=%d", w.Groups.GroupCount(g))
	}
}

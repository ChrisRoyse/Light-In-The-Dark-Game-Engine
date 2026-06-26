package sim

// #613 — best-fit member-span allocator FSV. SoT = the group store columns and
// member bytes read directly: large groups exceed the old fixed cap, growth
// relocates while preserving membership + order, mixed sizes co-exist in one
// arena, arena exhaustion drops cleanly without losing existing members, and
// save/load rebuilds the allocator to a hash-identical state.

import (
	"bytes"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/statehash"
)

func groupMembers(s *GroupStore, id GroupID) []EntityID {
	row, ok := s.resolve(id)
	if !ok {
		return nil
	}
	out := make([]EntityID, s.Len[row])
	copy(out, s.Members[s.Start[row]:s.Start[row]+s.Len[row]])
	return out
}

// TestSpanLargeGroupBeyondV1Cap: a single group grows to 200 members in one
// arena (the v1 fixed cap was 64) with insertion order preserved (R-UGR-7).
func TestSpanLargeGroupBeyondV1Cap(t *testing.T) {
	s := NewGroupStore(8, 512) // v1 perCap would have been 64
	g := s.CreateGroup()
	const N = 200
	for i := uint32(0); i < N; i++ {
		if !s.GroupAdd(g, ent(1000+i)) {
			t.Fatalf("add %d failed (arena should hold 200)", i)
		}
	}
	if s.GroupCount(g) != N {
		t.Fatalf("count=%d, want %d", s.GroupCount(g), N)
	}
	mem := groupMembers(s, g)
	for i := 0; i < N; i++ {
		if mem[i] != ent(1000+uint32(i)) {
			t.Fatalf("order broken at %d after relocation: got %d", i, mem[i])
		}
	}
	t.Logf("single group holds %d members (v1 cap was 64); order preserved across relocations", N)
}

// TestSpanMixedSizes: one large group + many small ones share a single arena.
func TestSpanMixedSizes(t *testing.T) {
	// 1024 arena: a 300-member group reserves 512 (2× doubling headroom) and
	// the small groups still fit alongside it.
	s := NewGroupStore(16, 1024)
	big := s.CreateGroup()
	for i := uint32(0); i < 300; i++ {
		if !s.GroupAdd(big, ent(i)) {
			t.Fatalf("big add %d failed", i)
		}
	}
	smalls := make([]GroupID, 0, 10)
	for k := 0; k < 10; k++ {
		g := s.CreateGroup()
		for j := uint32(0); j < 5; j++ {
			if !s.GroupAdd(g, ent(10000+uint32(k)*10+j)) {
				t.Fatalf("small[%d] add %d failed (arena should fit mixed sizes)", k, j)
			}
		}
		smalls = append(smalls, g)
	}
	if s.GroupCount(big) != 300 {
		t.Fatalf("big count=%d, want 300", s.GroupCount(big))
	}
	for k, g := range smalls {
		if s.GroupCount(g) != 5 {
			t.Fatalf("small[%d] count=%d, want 5", k, s.GroupCount(g))
		}
	}
	t.Logf("1×300 + 10×5 = %d members co-resident in a 512 arena", 300+50)
}

// TestSpanArenaExhaustionKeepsExisting: filling the arena drops further adds via
// the counter, and the group keeps every member it already had.
func TestSpanArenaExhaustionKeepsExisting(t *testing.T) {
	s := NewGroupStore(4, 16)
	g := s.CreateGroup()
	added := 0
	for i := uint32(0); i < 100; i++ {
		if s.GroupAdd(g, ent(i)) {
			added++
		}
	}
	t.Logf("arena=16: added=%d count=%d dropped=%d", added, s.GroupCount(g), s.DroppedMembers)
	if s.GroupCount(g) != int32(added) {
		t.Fatalf("count=%d != added=%d (existing members lost on a failed grow)", s.GroupCount(g), added)
	}
	if s.GroupCount(g) > 16 {
		t.Fatalf("count=%d exceeds arena 16", s.GroupCount(g))
	}
	if s.DroppedMembers == 0 {
		t.Fatal("expected dropped members on arena exhaustion")
	}
	// Membership integrity: the kept members are the first `added` in order.
	mem := groupMembers(s, g)
	for i := range mem {
		if mem[i] != ent(uint32(i)) {
			t.Fatalf("kept member %d = %d, want %d", i, mem[i], ent(uint32(i)))
		}
	}
}

// TestSpanDestroyCoalesceReuse: destroying groups returns their spans so a later
// large group reuses the coalesced arena.
func TestSpanDestroyCoalesceReuse(t *testing.T) {
	s := NewGroupStore(8, 64)
	// Fill the arena with 8 groups of 8.
	gs := make([]GroupID, 0, 8)
	for k := 0; k < 8; k++ {
		g := s.CreateGroup()
		for j := uint32(0); j < 8; j++ {
			s.GroupAdd(g, ent(uint32(k)*8+j))
		}
		gs = append(gs, g)
	}
	// Destroy them all → arena fully free + coalesced.
	for _, g := range gs {
		s.DestroyGroup(g)
	}
	// A single group should now grow to the whole 64-slot arena.
	big := s.CreateGroup()
	for i := uint32(0); i < 64; i++ {
		if !s.GroupAdd(big, ent(5000+i)) {
			t.Fatalf("reuse add %d failed — spans did not coalesce on destroy", i)
		}
	}
	t.Logf("after destroying 8×8 and coalescing, one group reused all 64 arena slots")
}

// TestSpanSaveLoadRebuild: build mixed groups, save, load into a fresh world,
// and confirm hash-identical state with the allocator rebuilt.
func TestSpanSaveLoadRebuild(t *testing.T) {
	build := func() *World {
		w := NewWorld(Caps{Units: 16, UnitGroups: 16, GroupMembers: 512})
		big := w.Groups.CreateGroup()
		for i := uint32(0); i < 120; i++ {
			w.Groups.GroupAdd(big, makeEntityID(i+1, 1))
		}
		for k := 0; k < 5; k++ {
			g := w.Groups.CreateGroup()
			for j := uint32(0); j < 7; j++ {
				w.Groups.GroupAdd(g, makeEntityID(uint32(k)*100+j+1, 1))
			}
		}
		return w
	}
	src := build()
	reg := NewHashRegistry()
	var s0 statehash.Snapshot
	want := src.HashState(reg, &s0).Top

	var buf bytes.Buffer
	if err := src.SaveState(&buf, 0xABCD); err != nil {
		t.Fatalf("save: %v", err)
	}
	dst := NewWorld(Caps{Units: 16, UnitGroups: 16, GroupMembers: 512})
	if err := dst.LoadState(bytes.NewReader(buf.Bytes()), 0xABCD); err != nil {
		t.Fatalf("load: %v", err)
	}
	var s1 statehash.Snapshot
	got := dst.HashState(NewHashRegistry(), &s1).Top
	t.Logf("pre-save hash=%016x post-load hash=%016x", want, got)
	if got != want {
		t.Fatalf("save/load hash mismatch: %016x != %016x (allocator rebuild diverged)", got, want)
	}
}

// TestSpanAddZeroAlloc: steady-state add (no growth) is zero-alloc.
func TestSpanAddZeroAlloc(t *testing.T) {
	s := NewGroupStore(8, 512)
	g := s.CreateGroup()
	for i := uint32(0); i < 64; i++ { // pre-grow past the doubling boundary
		s.GroupAdd(g, ent(i))
	}
	avg := testing.AllocsPerRun(200, func() {
		s.GroupClear(g)
		for i := uint32(0); i < 32; i++ {
			s.GroupAdd(g, ent(i))
		}
	})
	if avg != 0 {
		t.Fatalf("group add allocated %.2f objs/op, want 0", avg)
	}
}

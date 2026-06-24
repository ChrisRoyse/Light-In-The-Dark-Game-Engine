package sim

import (
	"bytes"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/statehash"
)

// #565 — unitgroups hash section + save block + load. SoT = the
// "unitgroups" sub-hash and the rebuilt store columns.

// armGroupPopulation builds a representative group population including a
// destroyed hole (so a slot carries a bumped generation and the free list
// is non-trivial) — the state most likely to expose a slot/gen/free-list
// serialization bug.
func armGroupPopulation(w *World) (live []GroupID) {
	g1 := w.Groups.CreateGroup()
	for _, e := range []uint32{1, 2, 3} {
		w.Groups.GroupAdd(g1, makeEntityID(e, 1))
	}
	g2 := w.Groups.CreateGroup()
	w.Groups.GroupAdd(g2, makeEntityID(10, 1))
	hole := w.Groups.CreateGroup()
	w.Groups.GroupAdd(hole, makeEntityID(99, 1))
	w.Groups.DestroyGroup(hole) // gen bump + free-list entry mid-pool
	g3 := w.Groups.CreateGroup()
	w.Groups.GroupAdd(g3, makeEntityID(20, 1))
	w.Groups.GroupAdd(g3, makeEntityID(21, 1))
	return []GroupID{g1, g2, g3}
}

func TestGroupSaveRoundTripHash(t *testing.T) {
	src := NewWorld(Caps{Units: 8, UnitGroups: 64, GroupMembers: 64 * 16})
	live := armGroupPopulation(src)

	reg := NewHashRegistry()
	var before statehash.Snapshot
	src.HashState(reg, &before)

	var buf bytes.Buffer
	if err := src.SaveState(&buf, 0); err != nil {
		t.Fatalf("SaveState: %v", err)
	}
	saved := append([]byte(nil), buf.Bytes()...)

	dst := NewWorld(Caps{Units: 8, UnitGroups: 64, GroupMembers: 64 * 16})
	if err := dst.LoadState(bytes.NewReader(saved), 0); err != nil {
		t.Fatalf("LoadState: %v", err)
	}

	var after statehash.Snapshot
	dst.HashState(reg, &after)

	gi := hashSystemIndex(t, "unitgroups")
	if before.Subs[gi] != after.Subs[gi] {
		t.Fatalf("unitgroups sub-hash differs: %016x -> %016x", before.Subs[gi], after.Subs[gi])
	}
	if before.Top != after.Top {
		t.Fatalf("top hash differs; diverged: %v", snapDiff(t, &before, &after))
	}

	// SoT: rebuilt columns match the source exactly.
	if dst.Groups.Count() != src.Groups.Count() {
		t.Fatalf("count %d != %d", dst.Groups.Count(), src.Groups.Count())
	}
	for _, g := range live {
		if !dst.Groups.Alive(g) {
			t.Fatalf("group %v not alive after load", g)
		}
		if dst.Groups.GroupCount(g) != src.Groups.GroupCount(g) {
			t.Fatalf("group %v count %d != %d", g, dst.Groups.GroupCount(g), src.Groups.GroupCount(g))
		}
		if !bytesEqualMembers(dst.Groups, src.Groups, g) {
			t.Fatalf("group %v members differ after load", g)
		}
	}
}

func bytesEqualMembers(a, b *GroupStore, g GroupID) bool {
	ma, mb := members(a, g), members(b, g)
	if len(ma) != len(mb) {
		return false
	}
	for i := range ma {
		if ma[i] != mb[i] {
			return false
		}
	}
	return true
}

// Two worlds running the same group ops hash identically; a divergent
// member makes only the unitgroups sub diverge (FirstDivergence localizes
// it). Proves the sub-hash is wired and isolating.
func TestGroupHashDeterminismAndLocalization(t *testing.T) {
	mk := func() *World {
		w := NewWorld(Caps{Units: 8, UnitGroups: 16, GroupMembers: 256})
		armGroupPopulation(w)
		return w
	}
	reg := NewHashRegistry()
	var a, b statehash.Snapshot
	mk().HashState(reg, &a)
	mk().HashState(reg, &b)
	if a.Top != b.Top {
		t.Fatalf("identical group ops diverged: %v", snapDiff(t, &a, &b))
	}

	// Perturb one member; only "unitgroups" must move.
	w2 := mk()
	g := w2.Groups.CreateGroup()
	w2.Groups.GroupAdd(g, makeEntityID(777, 1))
	var c statehash.Snapshot
	w2.HashState(reg, &c)
	gi := hashSystemIndex(t, "unitgroups")
	if a.Subs[gi] == c.Subs[gi] {
		t.Fatal("added member did not change the unitgroups sub-hash")
	}
	for i := range a.Subs {
		if i != gi && a.Subs[i] != c.Subs[i] {
			t.Fatalf("non-group sub %d changed — leak outside unitgroups", i)
		}
	}
}

// The #612 lesson: free-slot generations round-trip, so a CreateGroup
// after load mints the SAME handle the unbroken world would.
func TestGroupFreeSlotGenSurvivesSaveLoad(t *testing.T) {
	build := func() *World {
		w := NewWorld(Caps{Units: 8, UnitGroups: 16, GroupMembers: 256})
		// churn a slot so it carries an advanced generation in the free list
		for i := 0; i < 3; i++ {
			g := w.Groups.CreateGroup()
			w.Groups.DestroyGroup(g)
		}
		return w
	}
	unbroken := build()

	saved := build()
	var buf bytes.Buffer
	if err := saved.SaveState(&buf, 0); err != nil {
		t.Fatalf("SaveState: %v", err)
	}
	loaded := NewWorld(Caps{Units: 8, UnitGroups: 16, GroupMembers: 256})
	if err := loaded.LoadState(bytes.NewReader(buf.Bytes()), 0); err != nil {
		t.Fatalf("LoadState: %v", err)
	}

	hU := unbroken.Groups.CreateGroup()
	hL := loaded.Groups.CreateGroup()
	if hU != hL {
		t.Fatalf("post-load CreateGroup minted %v, unbroken minted %v — free-slot gen lost", hL, hU)
	}
}

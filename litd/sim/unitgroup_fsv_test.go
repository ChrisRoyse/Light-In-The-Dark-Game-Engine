package sim

import (
	"bytes"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/statehash"
)

// #567 — the unit-group acceptance suite: the cross-cutting properties
// the per-feature tests (#560–#566) don't assert together — a recorded
// determinism golden so a cross-platform divergence is caught, save/resume
// hash parity (save mid-scenario, resume in a fresh world, finish, compare
// to the unbroken run), and steady-state zero-alloc. Per-feature behavior
// (pool/membership/algebra/fills/prune/save) lives in the sibling tests.

// groupScenario drives a deterministic mixed sequence of group ops using
// synthetic entity ids — the hashed truth is the member bytes + slot/gen
// bookkeeping, so no real units are needed. It exercises create, unique
// add, both removals, all four algebra ops, clear, and a destroy (so a
// slot carries a bumped generation and the free list is non-trivial).
func groupScenario(w *World) {
	gs := w.Groups
	a := gs.CreateGroup()
	b := gs.CreateGroup()
	c := gs.CreateGroup()
	for i := uint32(1); i <= 12; i++ {
		gs.GroupAdd(a, makeEntityID(i, 1))
	}
	for i := uint32(6); i <= 18; i++ {
		gs.GroupAdd(b, makeEntityID(i, 1))
	}
	gs.GroupRemove(a, makeEntityID(3, 1))         // swap-remove
	gs.GroupRemoveOrdered(b, makeEntityID(7, 1))  // stable
	gs.GroupUnion(c, a, b)                         // c = a ∪ b
	d := gs.CreateGroup()
	gs.GroupIntersect(d, a, b) // d = a ∩ b
	e := gs.CreateGroup()
	gs.GroupDifference(e, a, b) // e = a ∖ b
	hole := gs.CreateGroup()
	gs.GroupAdd(hole, makeEntityID(99, 1))
	gs.DestroyGroup(hole) // gen bump + free-list churn
	f := gs.CreateGroup()
	gs.GroupCopy(f, c)
	gs.GroupClear(e)
}

func groupTopHash(w *World, reg *statehash.Registry) uint64 {
	var s statehash.Snapshot
	w.HashState(reg, &s)
	return s.Top
}

// TestGroupScenarioGolden records the full-state hash after the scenario.
// A change here means a determinism-relevant change to the group store's
// hashed bytes — intended (update the golden) or a regression.
func TestGroupScenarioGolden(t *testing.T) {
	w := NewWorld(Caps{Units: 32, UnitGroups: 64, GroupMembers: 64 * 16})
	groupScenario(w)
	reg := NewHashRegistry()
	// Recorded 2026-06-23 (#567); bumped da66…→ba33… (#572) when the empty
	// "kv" sub joined HashSystems (constant full-state shift; scenario
	// only touches the unitgroups sub).
	// Bumped 4846022f8bda62e5 → ab128a20d7c1c29a (#613): the best-fit span
	// allocator lifts the v1 fixed per-group cap (perCap=16 here). The union
	// c=a∪b is 17 members; v1 truncated it to 16 (DroppedMembers nonzero), so
	// the hashed member bytes + dropped counter legitimately change — groups
	// now hold their full membership (R-UGR-7). The empty-store sub-hash (every
	// non-group scenario) is unchanged, so no other determinism golden moves.
	const golden = uint64(0xab128a20d7c1c29a)
	got := groupTopHash(w, reg)
	if golden != 0 && got != golden {
		t.Fatalf("group golden hash %016x != recorded %016x (intended? update golden)", got, golden)
	}
	t.Logf("group scenario golden = %#016x", got)
}

// TestGroupTwoRunDeterminism: the scenario is a pure function of the ops —
// two independent worlds land on the identical full-state hash.
func TestGroupTwoRunDeterminism(t *testing.T) {
	mk := func() uint64 {
		w := NewWorld(Caps{Units: 32, UnitGroups: 64, GroupMembers: 64 * 16})
		groupScenario(w)
		return groupTopHash(w, NewHashRegistry())
	}
	if a, b := mk(), mk(); a != b {
		t.Fatalf("two scenario runs diverged: %016x != %016x", a, b)
	}
}

// TestGroupSaveResumeParity: a world saved partway through the scenario,
// reloaded into a fresh world, and finished must hash identically to a
// world that ran the whole scenario unbroken — the property a single
// save round-trip (no further ops) cannot prove (#612 class of bug).
func TestGroupSaveResumeParity(t *testing.T) {
	caps := Caps{Units: 32, UnitGroups: 64, GroupMembers: 64 * 16}

	// second half of the scenario, applied after the save point
	secondHalf := func(w *World) {
		gs := w.Groups
		// rebuild handles by slot is unnecessary — operate on fresh groups
		x := gs.CreateGroup()
		for i := uint32(20); i <= 30; i++ {
			gs.GroupAdd(x, makeEntityID(i, 1))
		}
		gs.GroupRemoveOrdered(x, makeEntityID(25, 1))
		y := gs.CreateGroup()
		gs.GroupCopy(y, x)
		gs.DestroyGroup(x) // churn a slot's generation post-load
		z := gs.CreateGroup()
		gs.GroupAdd(z, makeEntityID(31, 1))
	}

	// unbroken
	wu := NewWorld(caps)
	groupScenario(wu)
	secondHalf(wu)
	want := groupTopHash(wu, NewHashRegistry())

	// save after the first half, resume in a fresh world, finish
	ws := NewWorld(caps)
	groupScenario(ws)
	var buf bytes.Buffer
	if err := ws.SaveState(&buf, 0); err != nil {
		t.Fatalf("SaveState: %v", err)
	}
	wl := NewWorld(caps)
	if err := wl.LoadState(bytes.NewReader(buf.Bytes()), 0); err != nil {
		t.Fatalf("LoadState: %v", err)
	}
	secondHalf(wl)
	got := groupTopHash(wl, NewHashRegistry())

	if got != want {
		t.Fatalf("save/resume hash %016x != unbroken %016x — group state diverged across save", got, want)
	}
	t.Logf("FSV #567: group save/resume parity holds; hash %#016x", got)
}

// TestGroupSteadyStateZeroAlloc: the hot membership + fill churn allocates
// nothing once the world is built (R-GC-1).
func TestGroupSteadyStateZeroAlloc(t *testing.T) {
	w := NewWorld(Caps{Units: 32, UnitGroups: 64, GroupMembers: 64 * 16})
	a := w.Groups.CreateGroup()
	b := w.Groups.CreateGroup()
	dst := w.Groups.CreateGroup()
	for i := uint32(1); i <= 8; i++ {
		w.Groups.GroupAdd(a, makeEntityID(i, 1))
		w.Groups.GroupAdd(b, makeEntityID(i+4, 1))
	}
	avg := testing.AllocsPerRun(1000, func() {
		w.Groups.GroupAdd(a, makeEntityID(100, 1))
		w.Groups.GroupContains(a, makeEntityID(5, 1))
		w.Groups.GroupRemove(a, makeEntityID(100, 1))
		w.Groups.GroupUnion(dst, a, b)
		w.Groups.GroupIntersect(dst, a, b)
		w.Groups.PruneEntities([]EntityID{makeEntityID(3, 1)})
	})
	if avg != 0 {
		t.Fatalf("steady-state group churn allocated %.2f objs/op, want 0", avg)
	}
}

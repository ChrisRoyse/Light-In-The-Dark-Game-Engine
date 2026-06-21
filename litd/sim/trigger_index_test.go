package sim

// Full State Verification for the trigger event index (#458, ADR #451).
// SoT = the index bucket contents + the fire-order list returned by
// triggersFor. Happy path (exact fire set + order) + the four mandated
// edges, printing buckets and fire logs.

import "testing"

// a distinct non-death event kind for the negative case.
const evDamagedTest uint16 = 2

// idsOf renders a []TriggerID as their slot indices, for readable logs.
func idsOf(ts []TriggerID) []uint32 {
	out := make([]uint32, len(ts))
	for i, t := range ts {
		out[i] = t.Index()
	}
	return out
}

// onDeath creates a trigger registered on EvUnitDeath at the given scope.
func onDeath(w *World, scope uint32) TriggerID {
	t, _ := w.Triggers.New()
	w.Triggers.AddEvent(t, EventReg{Kind: EvUnitDeath, Scope: EntityID(scope)})
	return t
}

// TestTriggerIndexFireSetAndOrder — register 3 triggers on EvUnitDeath
// (slots 0,1,2) + 1 on a damage kind; a death lookup returns exactly the
// 3 in slot order, the damage trigger is absent.
func TestTriggerIndexFireSetAndOrder(t *testing.T) {
	w := NewWorld(Caps{})
	d0 := onDeath(w, GlobalScope)
	d1 := onDeath(w, GlobalScope)
	d2 := onDeath(w, GlobalScope)
	dmg, _ := w.Triggers.New()
	w.Triggers.AddEvent(dmg, EventReg{Kind: evDamagedTest, Scope: 0})

	w.ensureTriggerIndex()
	got := w.trigIndex.triggersFor(EvUnitDeath, GlobalScope)
	t.Logf("EvUnitDeath bucket entries=%d fire-order(slots)=%v (want [0 1 2])", len(got), idsOf(got))

	if len(got) != 3 {
		t.Fatalf("death fired %d triggers, want 3", len(got))
	}
	want := []TriggerID{d0, d1, d2}
	for i := range want {
		if got[i] != want[i] {
			t.Fatalf("fire order wrong at %d: got slot %d want slot %d", i, got[i].Index(), want[i].Index())
		}
	}
	// the damage trigger must not be in the death fire set.
	for _, g := range got {
		if g == dmg {
			t.Fatal("damage trigger fired on a death event")
		}
	}
	// and the damage bucket holds exactly it.
	dg := w.trigIndex.triggersFor(evDamagedTest, GlobalScope)
	t.Logf("damage bucket fire-order(slots)=%v (want [3])", idsOf(dg))
	if len(dg) != 1 || dg[0] != dmg {
		t.Fatalf("damage bucket wrong: %v", idsOf(dg))
	}
}

// TestTriggerIndexDescendingKinds — regression for the bucket-insertion
// aliasing bug (#458, surfaced by #462): when a higher kind is registered
// BEFORE a lower kind, the rebuild must insert the lower bucket ahead of
// the higher one without the two sharing an entries backing array. Before
// the fix, looking up the higher kind returned the lower kind's trigger.
func TestTriggerIndexDescendingKinds(t *testing.T) {
	w := NewWorld(Caps{})
	const kHi, kMid, kLo uint16 = 23, 22, 7
	// register strictly descending so every new bucket inserts at the front.
	hi, _ := w.Triggers.New()
	w.Triggers.AddEvent(hi, EventReg{Kind: kHi})
	mid, _ := w.Triggers.New()
	w.Triggers.AddEvent(mid, EventReg{Kind: kMid})
	lo, _ := w.Triggers.New()
	w.Triggers.AddEvent(lo, EventReg{Kind: kLo})

	w.ensureTriggerIndex()
	for kind, want := range map[uint16]TriggerID{kHi: hi, kMid: mid, kLo: lo} {
		got := w.trigIndex.triggersFor(kind, GlobalScope)
		t.Logf("kind %d → slots %v (want [%d])", kind, idsOf(got), want.Index())
		if len(got) != 1 || got[0] != want {
			t.Fatalf("kind %d routed to %v, want trigger slot %d", kind, idsOf(got), want.Index())
		}
	}
}

// TestTriggerIndexScopeFilter — edge 1: a scoped trigger fires only for
// its scope key; a global trigger fires for every key.
func TestTriggerIndexScopeFilter(t *testing.T) {
	w := NewWorld(Caps{})
	glob := onDeath(w, GlobalScope) // slot 0, global
	p7 := onDeath(w, 7)             // slot 1, scoped to key 7
	p3 := onDeath(w, 3)             // slot 2, scoped to key 3

	w.ensureTriggerIndex()
	for _, tc := range []struct {
		key  uint32
		want []TriggerID
	}{
		{7, []TriggerID{glob, p7}},
		{3, []TriggerID{glob, p3}},
		{1, []TriggerID{glob}},
		{GlobalScope, []TriggerID{glob}},
	} {
		got := w.trigIndex.triggersFor(EvUnitDeath, tc.key)
		t.Logf("scopeKey=%d -> fire(slots)=%v (want %v)", tc.key, idsOf(got), idsOf(tc.want))
		if len(got) != len(tc.want) {
			t.Fatalf("scopeKey %d: got %d want %d", tc.key, len(got), len(tc.want))
		}
		for i := range tc.want {
			if got[i] != tc.want[i] {
				t.Fatalf("scopeKey %d entry %d: got slot %d want slot %d", tc.key, i, got[i].Index(), tc.want[i].Index())
			}
		}
	}
}

// TestTriggerIndexEmptyKindNoOp — edge 2: a kind with no triggers returns
// an empty fire set (a dispatch no-op).
func TestTriggerIndexEmptyKindNoOp(t *testing.T) {
	w := NewWorld(Caps{})
	onDeath(w, GlobalScope)
	w.ensureTriggerIndex()
	got := w.trigIndex.triggersFor(9999, GlobalScope) // unused kind
	t.Logf("triggersFor(unused kind 9999) -> %v (want empty)", idsOf(got))
	if len(got) != 0 {
		t.Fatalf("empty kind returned %d triggers", len(got))
	}
}

// TestTriggerIndexMutateMidMatch — edge 3: register then unregister
// (destroy) and the bucket updates deterministically.
func TestTriggerIndexMutateMidMatch(t *testing.T) {
	w := NewWorld(Caps{})
	d0 := onDeath(w, GlobalScope)
	d1 := onDeath(w, GlobalScope)
	w.ensureTriggerIndex()
	t.Logf("before destroy: %v (want [0 1])", idsOf(w.trigIndex.triggersFor(EvUnitDeath, GlobalScope)))

	if !w.Triggers.Destroy(d0) {
		t.Fatal("destroy failed")
	}
	w.ensureTriggerIndex() // dirty bit set by Destroy → rebuild
	got := w.trigIndex.triggersFor(EvUnitDeath, GlobalScope)
	t.Logf("after destroy d0: %v (want [1])", idsOf(got))
	if len(got) != 1 || got[0] != d1 {
		t.Fatalf("bucket after destroy wrong: %v", idsOf(got))
	}

	// add a third — reuses slot 0 (fresh gen) and reappears in slot order.
	d2 := onDeath(w, GlobalScope)
	w.ensureTriggerIndex()
	got = w.trigIndex.triggersFor(EvUnitDeath, GlobalScope)
	t.Logf("after re-add (reused slot 0): %v", idsOf(got))
	if len(got) != 2 || got[0] != d2 || got[1] != d1 {
		t.Fatalf("bucket after re-add wrong: %v", idsOf(got))
	}
}

// TestTriggerIndexDoubleRunIdentical — edge 4: two runs produce identical
// fire-order lists.
func TestTriggerIndexDoubleRunIdentical(t *testing.T) {
	run := func() []uint32 {
		w := NewWorld(Caps{})
		onDeath(w, GlobalScope)
		onDeath(w, 5)
		onDeath(w, GlobalScope)
		w.ensureTriggerIndex()
		return idsOf(w.trigIndex.triggersFor(EvUnitDeath, 5))
	}
	a, b := run(), run()
	t.Logf("run1=%v run2=%v", a, b)
	if len(a) != len(b) {
		t.Fatalf("length differs: %v vs %v", a, b)
	}
	for i := range a {
		if a[i] != b[i] {
			t.Fatalf("fire order differs at %d: %v vs %v", i, a, b)
		}
	}
}

// TestTriggerIndexLookupZeroAlloc — R-GC-1: lookup allocates nothing once
// the scratch buffer has grown (steady-state dispatch).
func TestTriggerIndexLookupZeroAlloc(t *testing.T) {
	w := NewWorld(Caps{})
	onDeath(w, GlobalScope)
	onDeath(w, GlobalScope)
	w.ensureTriggerIndex()
	_ = w.trigIndex.triggersFor(EvUnitDeath, GlobalScope) // grow scratch
	n := testing.AllocsPerRun(1000, func() {
		_ = w.trigIndex.triggersFor(EvUnitDeath, GlobalScope)
	})
	t.Logf("triggersFor: %v allocs/op", n)
	if n != 0 {
		t.Fatalf("lookup allocates %v/op, want 0", n)
	}
}

// TestTriggerIndexRebuildOnlyWhenDirty — the index rebuilds on a
// structural change and not otherwise (the lazy-dirty contract).
func TestTriggerIndexRebuildOnlyWhenDirty(t *testing.T) {
	w := NewWorld(Caps{})
	onDeath(w, GlobalScope)
	if !w.Triggers.dirty {
		t.Fatal("structural change did not set dirty")
	}
	w.ensureTriggerIndex()
	if w.Triggers.dirty {
		t.Fatal("dirty not cleared after rebuild")
	}
	w.ensureTriggerIndex() // no change → stays clean, no rebuild needed
	if w.Triggers.dirty {
		t.Fatal("dirty spuriously set")
	}
	t.Logf("dirty bit: set on structural change, cleared on rebuild, stable otherwise")
}

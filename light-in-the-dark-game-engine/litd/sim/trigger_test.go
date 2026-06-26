package sim

// Full State Verification for the first-class Trigger slab (#456, ADR
// #451). SoT = the trigger slab rows + the save bytes + World.HashState.
// Happy path + the four mandated edges, printing slab state before/after
// so evidence is the bytes, not a return value.

import (
	"bytes"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/statehash"
)

// dumpTrigger prints one slab row for FSV evidence.
func dumpTrigger(t *testing.T, w *World, id TriggerID, label string) {
	ts := w.Triggers
	sl := ts.slot(id)
	if sl == nil {
		t.Logf("%s: id=%#x INVALID (stale/dead)", label, uint64(id))
		return
	}
	t.Logf("%s: id=%#x idx=%d gen=%d enabled=%v on=%v cond=%d events=%v actions=%v",
		label, uint64(id), id.Index(), id.Generation(), sl.enabled, sl.on, sl.cond, sl.events, sl.actions)
}

// buildTwoTriggers registers two handlers and creates two triggers with
// distinct fields. Re-running on a fresh world reproduces the identical
// slab — the property the save format relies on.
func buildTwoTriggers(w *World) (TriggerID, TriggerID) {
	condRef := w.RegisterHandlerID("cond.t456", trueFn)
	actRef := w.RegisterHandlerID("action.t456", echoFn)

	a, _ := w.Triggers.New()
	w.Triggers.AddEvent(a, EventReg{Kind: EvUnitDeath, Scope: 0})
	w.Triggers.AddAction(a, actRef)
	w.Triggers.SetCondition(a, w.Cond(condRef)) // real boolexpr leaf (#457)

	b, _ := w.Triggers.New()
	w.Triggers.AddEvent(b, EventReg{Kind: 42, Scope: EntityID(7)})
	w.Triggers.AddAction(b, actRef)
	w.Triggers.SetEnabled(b, false) // a disabled trigger
	return a, b
}

// TestTriggerHappyPath — X+X=Y: 2 created → exactly 2 rows, indices 0,1.
func TestTriggerHappyPath(t *testing.T) {
	w := NewWorld(Caps{})
	a, b := buildTwoTriggers(w)

	if w.Triggers.Count() != 2 {
		t.Fatalf("Count=%d want 2", w.Triggers.Count())
	}
	if a.Index() != 0 || b.Index() != 1 {
		t.Fatalf("indices not 0,1: a=%d b=%d", a.Index(), b.Index())
	}
	dumpTrigger(t, w, a, "trigger A")
	dumpTrigger(t, w, b, "trigger B")

	if !w.Triggers.Valid(a) || !w.Triggers.Valid(b) {
		t.Fatal("freshly created triggers invalid")
	}
	if !w.Triggers.Enabled(a) || w.Triggers.Enabled(b) {
		t.Fatalf("enabled state wrong: A=%v B=%v want true,false", w.Triggers.Enabled(a), w.Triggers.Enabled(b))
	}
	if len(w.Triggers.Events(a)) != 1 || len(w.Triggers.Actions(a)) != 1 {
		t.Fatal("A events/actions not recorded")
	}
}

// TestTriggerSaveLoadIdentity — create 2, save, load into a fresh world
// that re-registered the same handlers, assert rows + hash identical and
// re-save byte-identical.
func TestTriggerSaveLoadIdentity(t *testing.T) {
	src := NewWorld(Caps{})
	a, b := buildTwoTriggers(src)
	// also destroy-and-recreate to exercise a non-trivial free list + gen.
	c, _ := src.Triggers.New()
	src.Triggers.Destroy(c) // c's slot now dead, on the free list, gen bumped

	reg := NewHashRegistry()
	var before statehash.Snapshot
	src.HashState(reg, &before)
	dumpTrigger(t, src, a, "BEFORE A")
	dumpTrigger(t, src, b, "BEFORE B")
	t.Logf("BEFORE: count=%d freeLen=%d triggers-sub=%016x top=%016x",
		src.Triggers.Count(), len(src.Triggers.free), subOf(reg, &before, "triggers"), before.Top)

	var buf bytes.Buffer
	if err := src.SaveState(&buf, 0); err != nil {
		t.Fatal(err)
	}
	saved := append([]byte(nil), buf.Bytes()...)

	dst := NewWorld(Caps{})
	dst.RegisterHandlerID("cond.t456", trueFn) // re-register same names/order
	dst.RegisterHandlerID("action.t456", echoFn)
	if err := dst.LoadState(bytes.NewReader(saved), 0); err != nil {
		t.Fatalf("load failed: %v", err)
	}

	var after statehash.Snapshot
	dst.HashState(reg, &after)
	dumpTrigger(t, dst, a, "AFTER A")
	dumpTrigger(t, dst, b, "AFTER B")
	t.Logf("AFTER:  count=%d freeLen=%d triggers-sub=%016x top=%016x",
		dst.Triggers.Count(), len(dst.Triggers.free), subOf(reg, &after, "triggers"), after.Top)

	if before.Top != after.Top {
		t.Fatalf("hash diverged across save/load: %016x != %016x", before.Top, after.Top)
	}
	if !dst.Triggers.Valid(a) || dst.Triggers.Enabled(b) {
		t.Fatal("restored trigger state wrong (A invalid or B not disabled)")
	}
	if dst.Triggers.Valid(c) {
		t.Fatal("destroyed trigger c came back alive after load")
	}
	// re-save byte-identical
	var buf2 bytes.Buffer
	if err := dst.SaveState(&buf2, 0); err != nil {
		t.Fatal(err)
	}
	if !bytes.Equal(saved, buf2.Bytes()) {
		t.Fatal("re-save of restored world not byte-identical")
	}
	t.Logf("PASS: rows + hash identical across save/load; re-save byte-identical (%d B)", len(saved))
}

// TestTriggerStaleHandleRejected — edge 1: destroy then access via the
// stale handle → invalid (gen mismatch). Slot reuse mints a fresh gen.
func TestTriggerStaleHandleRejected(t *testing.T) {
	w := NewWorld(Caps{})
	a, _ := w.Triggers.New()
	t.Logf("created a=%#x valid=%v", uint64(a), w.Triggers.Valid(a))
	if !w.Triggers.Destroy(a) {
		t.Fatal("destroy failed")
	}
	if w.Triggers.Valid(a) {
		t.Fatal("stale handle still valid after destroy")
	}
	// reuse the slot: same index, new generation.
	a2, _ := w.Triggers.New()
	t.Logf("reused slot: stale a=%#x valid=%v | fresh a2=%#x valid=%v",
		uint64(a), w.Triggers.Valid(a), uint64(a2), w.Triggers.Valid(a2))
	if a2.Index() != a.Index() {
		t.Fatalf("reuse did not recycle slot: a.idx=%d a2.idx=%d", a.Index(), a2.Index())
	}
	if a2.Generation() == a.Generation() {
		t.Fatal("generation not bumped on reuse — stale handle would alias")
	}
	if w.Triggers.Valid(a) {
		t.Fatal("stale handle aliases the reused slot (gen check failed)")
	}
	if !w.Triggers.Valid(a2) {
		t.Fatal("fresh handle invalid")
	}
}

// TestTriggerCapFailClosed — edge 2: create at slab capacity+1 fails
// closed (no grow, no panic).
func TestTriggerCapFailClosed(t *testing.T) {
	w := NewWorld(Caps{Triggers: 2})
	if w.Triggers.Cap() != 2 {
		t.Fatalf("cap=%d want 2", w.Triggers.Cap())
	}
	a, okA := w.Triggers.New()
	b, okB := w.Triggers.New()
	c, okC := w.Triggers.New() // capacity+1
	t.Logf("cap=2: new#1 ok=%v new#2 ok=%v new#3 ok=%v (c=%#x)", okA, okB, okC, uint64(c))
	if !okA || !okB {
		t.Fatal("first two creates should succeed")
	}
	if okC || c != NoTrigger {
		t.Fatal("create past capacity must fail closed")
	}
	// after a destroy, a slot frees and create succeeds again.
	w.Triggers.Destroy(a)
	d, okD := w.Triggers.New()
	t.Logf("after destroy a: new ok=%v d.idx=%d", okD, d.Index())
	if !okD {
		t.Fatal("create after destroy should reuse the freed slot")
	}
	_ = b
}

// TestTriggerDoubleRunIdenticalHash — edge 4: two independent runs build
// the same triggers → identical ids and identical hash.
func TestTriggerDoubleRunIdenticalHash(t *testing.T) {
	reg := NewHashRegistry()
	run := func() (uint64, TriggerID, TriggerID) {
		w := NewWorld(Caps{})
		a, b := buildTwoTriggers(w)
		var s statehash.Snapshot
		w.HashState(reg, &s)
		return s.Top, a, b
	}
	h1, a1, b1 := run()
	h2, a2, b2 := run()
	t.Logf("run1 ids=%#x,%#x top=%016x", uint64(a1), uint64(b1), h1)
	t.Logf("run2 ids=%#x,%#x top=%016x", uint64(a2), uint64(b2), h2)
	if a1 != a2 || b1 != b2 {
		t.Fatal("trigger ids not deterministic across runs")
	}
	if h1 != h2 {
		t.Fatalf("hash not deterministic: %016x != %016x", h1, h2)
	}
}

// TestTriggerHashZeroAllocValidate — Valid is a hot-path lookup; no alloc.
func TestTriggerHashZeroAllocValidate(t *testing.T) {
	w := NewWorld(Caps{})
	a, _ := w.Triggers.New()
	var sink bool
	n := testing.AllocsPerRun(1000, func() { sink = w.Triggers.Valid(a) })
	_ = sink
	if n != 0 {
		t.Fatalf("Valid allocates %v/op, want 0", n)
	}
	t.Logf("Triggers.Valid: %v allocs/op", n)
}

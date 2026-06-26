package sim

// Full State Verification for trigger lifecycle (#460, ADR #451). SoT =
// the enabled/on flags in the slab + save bytes + the action side-effects
// (per-unit UserData). Each verb shown at its SoT, plus the four edges.

import (
	"bytes"
	"testing"
)

const evLifecycle uint16 = 2

// condAlwaysFalse never passes — proves Execute bypasses the condition.
func condAlwaysFalse(w *World, e Event) bool { return false }

// TestLifecycleExecuteBypassesCondition — A.Execute(B) runs B's actions
// though B's event never fired and B's condition is failing.
func TestLifecycleExecuteBypassesCondition(t *testing.T) {
	w := NewWorld(Caps{})
	falseRef := w.RegisterHandlerID("cond.false", condAlwaysFalse)
	markRef := w.RegisterHandlerID("act.mark42", actMark42)
	u := spawnUnit(w)

	b, _ := w.Triggers.New()
	w.Triggers.AddEvent(b, EventReg{Kind: evLifecycle})
	w.Triggers.SetCondition(b, w.Cond(falseRef)) // would block a normal fire
	w.Triggers.AddAction(b, markRef)

	// Normal emit: condition false → B does NOT fire (SoT proof).
	emitAndStep(w, Event{Kind: evLifecycle, Src: u, Arg: 1})
	t.Logf("after normal emit (cond false): UserData=%d (want 0)", w.UserData(u))
	if w.UserData(u) != 0 {
		t.Fatal("B fired despite failing condition")
	}

	// Execute bypasses the (failing) condition → actions run.
	w.ExecuteTrigger(b, Event{Kind: evLifecycle, Src: u, Arg: 1})
	t.Logf("after ExecuteTrigger(B): UserData=%d (want 42 — condition bypassed)", w.UserData(u))
	if w.UserData(u) != 42 {
		t.Fatal("ExecuteTrigger did not bypass the condition")
	}
}

// TestLifecycleInitiallyOff — edge 1: Initially-On=false → no fire until
// Enable.
func TestLifecycleInitiallyOff(t *testing.T) {
	w := NewWorld(Caps{})
	markRef := w.RegisterHandlerID("act.mark42", actMark42)
	u := spawnUnit(w)
	tr, _ := w.Triggers.New()
	w.Triggers.AddEvent(tr, EventReg{Kind: evLifecycle})
	w.Triggers.AddAction(tr, markRef)
	w.Triggers.SetInitiallyOn(tr, false)
	t.Logf("after SetInitiallyOn(false): IsEnabled=%v InitiallyOn=%v", w.Triggers.IsEnabled(tr), w.Triggers.InitiallyOn(tr))
	if w.Triggers.IsEnabled(tr) {
		t.Fatal("Initially-Off trigger is enabled")
	}

	emitAndStep(w, Event{Kind: evLifecycle, Src: u, Arg: 1})
	t.Logf("emit while initially-off: UserData=%d (want 0)", w.UserData(u))
	if w.UserData(u) != 0 {
		t.Fatal("initially-off trigger fired")
	}

	w.Triggers.Enable(tr)
	emitAndStep(w, Event{Kind: evLifecycle, Src: u, Arg: 1})
	t.Logf("emit after Enable: UserData=%d (want 42)", w.UserData(u))
	if w.UserData(u) != 42 {
		t.Fatal("trigger did not fire after Enable")
	}
}

// TestLifecycleEvaluatePurity — edge 2: Evaluate() on a false condition
// returns false with zero side effects (SoT unchanged).
func TestLifecycleEvaluatePurity(t *testing.T) {
	w := NewWorld(Caps{})
	condRef := w.RegisterHandlerID("cond.amount>10", condAmountGt10)
	markRef := w.RegisterHandlerID("act.mark42", actMark42)
	u := spawnUnit(w)
	tr, _ := w.Triggers.New()
	w.Triggers.SetCondition(tr, w.Cond(condRef))
	w.Triggers.AddAction(tr, markRef)

	before := w.UserData(u)
	gotFalse := w.EvaluateTrigger(tr, Event{Src: u, Arg: 5})  // 5 > 10 == false
	gotTrue := w.EvaluateTrigger(tr, Event{Src: u, Arg: 20}) // 20 > 10 == true
	after := w.UserData(u)
	t.Logf("Evaluate(Arg=5)=%v Evaluate(Arg=20)=%v | UserData before=%d after=%d (must be unchanged)",
		gotFalse, gotTrue, before, after)
	if gotFalse || !gotTrue {
		t.Fatalf("Evaluate results wrong: false-case=%v true-case=%v", gotFalse, gotTrue)
	}
	if before != after {
		t.Fatal("Evaluate had a side effect (action ran)")
	}
}

// TestLifecycleDisableMidCascade — edge 3: an action that disables a
// downstream trigger takes effect on that trigger's event (it is
// re-checked at fire time), deterministically.
func TestLifecycleDisableMidCascade(t *testing.T) {
	w := NewWorld(Caps{})
	const k1, k2 uint16 = 2, 3
	u := spawnUnit(w)

	tB, _ := w.Triggers.New() // downstream, on k2
	w.Triggers.AddEvent(tB, EventReg{Kind: k2})
	bMark := w.RegisterHandlerID("act.bMark", func(wr *World, e Event) bool { wr.SetUserData(e.Src, 99); return true })
	w.Triggers.AddAction(tB, bMark)

	// A (on k1): disable B, then cascade-emit k2.
	aRef := w.RegisterHandlerID("act.disableBthenCascade", func(wr *World, e Event) bool {
		wr.Triggers.Disable(tB)
		wr.Emit(Event{Kind: k2, Src: e.Src})
		return true
	})
	tA, _ := w.Triggers.New()
	w.Triggers.AddEvent(tA, EventReg{Kind: k1})
	w.Triggers.AddAction(tA, aRef)

	emitAndStep(w, Event{Kind: k1, Src: u})
	t.Logf("A disabled B then cascaded k2: UserData=%d (want 0 — B was disabled before its event)", w.UserData(u))
	if w.UserData(u) != 0 {
		t.Fatal("B fired though disabled mid-cascade")
	}
}

// TestLifecycleSaveLoadDisabled — edge 4: save while disabled → load
// preserves the disabled flag.
func TestLifecycleSaveLoadDisabled(t *testing.T) {
	src := NewWorld(Caps{})
	mk := func(w *World) TriggerID {
		w.RegisterHandlerID("act.mark42", actMark42)
		tr, _ := w.Triggers.New()
		w.Triggers.AddEvent(tr, EventReg{Kind: evLifecycle})
		ref, _ := w.HandlerRefOf("act.mark42")
		w.Triggers.AddAction(tr, ref)
		return tr
	}
	tr := mk(src)
	src.Triggers.Disable(tr)
	t.Logf("src: IsEnabled=%v (disabled before save)", src.Triggers.IsEnabled(tr))

	var buf bytes.Buffer
	if err := src.SaveState(&buf, 0); err != nil {
		t.Fatal(err)
	}
	dst := NewWorld(Caps{})
	mk(dst)
	if err := dst.LoadState(bytes.NewReader(buf.Bytes()), 0); err != nil {
		t.Fatalf("load failed: %v", err)
	}
	t.Logf("dst after load: IsEnabled=%v (want false)", dst.Triggers.IsEnabled(tr))
	if dst.Triggers.IsEnabled(tr) {
		t.Fatal("disabled flag not preserved across save/load")
	}
	// and it stays silent on an emitted event.
	u := spawnUnit(dst)
	dst.Emit(Event{Kind: evLifecycle, Src: u, Arg: 1})
	dst.Step()
	if dst.UserData(u) != 0 {
		t.Fatal("restored disabled trigger fired")
	}
	t.Logf("restored disabled trigger stayed silent: UserData=%d", dst.UserData(u))
}

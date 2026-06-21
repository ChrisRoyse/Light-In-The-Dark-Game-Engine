package sim

// Full State Verification for trigger dispatch — the ECA core (#459, ADR
// #451). SoT = the action side-effects at their own store (per-unit
// UserData) + the fire/skip observations + Game state hash. Happy path
// (condition gate observable at the SoT) + the four mandated edges.

import (
	"bytes"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/statehash"
)

// condAmountGt10 passes when the event payload (Arg) exceeds 10.
func condAmountGt10(w *World, e Event) bool { return e.Arg > 10 }

// actMark42 writes 42 into the source unit's UserData — the observable
// SoT proving the action ran.
func actMark42(w *World, e Event) bool { w.SetUserData(e.Src, 42); return true }

const evDamagedDispatch uint16 = 2

// spawnUnit makes a live unit for UserData to hang off.
func spawnUnit(w *World) EntityID {
	id, ok := w.CreateUnit(fixed.Vec2{X: fixed.FromInt(10), Y: fixed.FromInt(10)}, 0)
	if !ok {
		panic("CreateUnit failed")
	}
	return id
}

// emitAndStep queues e then advances one tick so phase-6 flushes it.
func emitAndStep(w *World, e Event) {
	w.Emit(e)
	w.Step()
}

// TestDispatchConditionGate — X+X=Y at the SoT: deal 5 (cond false →
// UserData unchanged), deal 20 (cond true → UserData==42).
func TestDispatchConditionGate(t *testing.T) {
	w := NewWorld(Caps{})
	condRef := w.RegisterHandlerID("cond.amount>10", condAmountGt10)
	actRef := w.RegisterHandlerID("act.mark42", actMark42)
	u := spawnUnit(w)

	tr, _ := w.Triggers.New()
	w.Triggers.AddEvent(tr, EventReg{Kind: evDamagedDispatch, Scope: 0})
	w.Triggers.SetCondition(tr, w.Cond(condRef))
	w.Triggers.AddAction(tr, actRef)

	t.Logf("SoT before deal-5: UserData(u)=%d", w.UserData(u))
	emitAndStep(w, Event{Kind: evDamagedDispatch, Src: u, Arg: 5})
	t.Logf("SoT after deal-5 (cond false, skip): UserData(u)=%d (want 0)", w.UserData(u))
	if w.UserData(u) != 0 {
		t.Fatal("condition false but action ran (UserData changed)")
	}

	emitAndStep(w, Event{Kind: evDamagedDispatch, Src: u, Arg: 20})
	t.Logf("SoT after deal-20 (cond true, fire): UserData(u)=%d (want 42)", w.UserData(u))
	if w.UserData(u) != 42 {
		t.Fatal("condition true but action did not run (UserData unchanged)")
	}
}

// TestDispatchDisabledNeverFires — edge 1: a disabled trigger never
// fires though its event is emitted.
func TestDispatchDisabledNeverFires(t *testing.T) {
	w := NewWorld(Caps{})
	actRef := w.RegisterHandlerID("act.mark42", actMark42)
	u := spawnUnit(w)
	tr, _ := w.Triggers.New()
	w.Triggers.AddEvent(tr, EventReg{Kind: evDamagedDispatch, Scope: 0})
	w.Triggers.AddAction(tr, actRef)
	w.Triggers.SetEnabled(tr, false)

	emitAndStep(w, Event{Kind: evDamagedDispatch, Src: u, Arg: 20})
	t.Logf("disabled trigger, event emitted: UserData(u)=%d (want 0)", w.UserData(u))
	if w.UserData(u) != 0 {
		t.Fatal("disabled trigger fired")
	}
}

// TestDispatchCascade — edge 2: an action that emits an event fires a
// dependent trigger in the same flush, in order.
func TestDispatchCascade(t *testing.T) {
	w := NewWorld(Caps{})
	var order []string
	const k1, k2 uint16 = 2, 3
	u := spawnUnit(w)

	// action on k1: record, then emit k2 (the cascade).
	a1 := w.RegisterHandlerID("act.firstAndCascade", func(wr *World, e Event) bool {
		order = append(order, "k1-action")
		wr.Emit(Event{Kind: k2, Src: e.Src, Arg: e.Arg})
		return true
	})
	a2 := w.RegisterHandlerID("act.second", func(wr *World, e Event) bool {
		order = append(order, "k2-action")
		wr.SetUserData(e.Src, 7)
		return true
	})
	t1, _ := w.Triggers.New()
	w.Triggers.AddEvent(t1, EventReg{Kind: k1})
	w.Triggers.AddAction(t1, a1)
	t2, _ := w.Triggers.New()
	w.Triggers.AddEvent(t2, EventReg{Kind: k2})
	w.Triggers.AddAction(t2, a2)

	emitAndStep(w, Event{Kind: k1, Src: u, Arg: 1})
	t.Logf("cascade order=%v UserData=%d (want [k1-action k2-action], 7)", order, w.UserData(u))
	if len(order) != 2 || order[0] != "k1-action" || order[1] != "k2-action" {
		t.Fatalf("cascade order wrong: %v", order)
	}
	if w.UserData(u) != 7 {
		t.Fatal("cascade dependent trigger did not run")
	}
}

// TestDispatchSleepSaveResume — edge 3: an action sleeps; save mid-sleep;
// load; the remaining actions resume at the correct wake tick.
func TestDispatchSleepSaveResume(t *testing.T) {
	build := func(w *World) (TriggerID, EntityID) {
		w.RegisterHandlerID("act.sleepThenMark", func(wr *World, e Event) bool {
			wr.SetUserData(e.Src, 1) // step 1 done
			wr.TriggerSleep(5)       // suspend the trigger thread
			return true
		})
		w.RegisterHandlerID("act.afterSleep", func(wr *World, e Event) bool {
			wr.SetUserData(e.Src, 2) // step 2 — only after the sleep
			return true
		})
		u := spawnUnit(w)
		tr, _ := w.Triggers.New()
		w.Triggers.AddEvent(tr, EventReg{Kind: evDamagedDispatch})
		a1, _ := w.HandlerRefOf("act.sleepThenMark")
		a2, _ := w.HandlerRefOf("act.afterSleep")
		w.Triggers.AddAction(tr, a1)
		w.Triggers.AddAction(tr, a2)
		return tr, u
	}

	src := NewWorld(Caps{})
	_, u := build(src)
	emitAndStep(src, Event{Kind: evDamagedDispatch, Src: u, Arg: 1})
	t.Logf("after fire+sleep: tick=%d UserData(u)=%d pendingSleepers=%d (want 1, 1)",
		src.Tick(), src.UserData(u), src.Sched.PendingSleepers())
	if src.UserData(u) != 1 || src.Sched.PendingSleepers() != 1 {
		t.Fatal("sleep did not suspend after step 1")
	}

	var buf bytes.Buffer
	if err := src.SaveState(&buf, 0); err != nil {
		t.Fatal(err)
	}

	// fresh world re-registers the same handlers (same order) and loads.
	dst := NewWorld(Caps{})
	build(dst) // re-register; its own units/triggers are overwritten by load
	if err := dst.LoadState(bytes.NewReader(buf.Bytes()), 0); err != nil {
		t.Fatalf("load failed: %v", err)
	}
	t.Logf("loaded at tick=%d UserData(u)=%d sleepers=%d", dst.Tick(), dst.UserData(u), dst.Sched.PendingSleepers())
	if dst.UserData(u) != 1 {
		t.Fatal("loaded world lost step-1 side effect")
	}
	// step until the wake tick (sleep was 5 ticks from the fire tick).
	wokeAt := uint32(0)
	for i := 0; i < 10; i++ {
		dst.Step()
		if dst.UserData(u) == 2 {
			wokeAt = dst.Tick()
			break
		}
	}
	t.Logf("resumed: UserData(u)=%d woke at tick=%d (fired at tick 1, +5 → 6)", dst.UserData(u), wokeAt)
	if dst.UserData(u) != 2 {
		t.Fatal("suspended action never resumed after load")
	}
	if wokeAt != 6 {
		t.Fatalf("resumed at tick %d, want 6", wokeAt)
	}
}

// TestDispatchZeroAllocSteadyState — R-GC-1/R-GC-3: once the index is
// built, firing an event (condition eval + action run) allocates nothing.
func TestDispatchZeroAllocSteadyState(t *testing.T) {
	w := NewWorld(Caps{})
	condRef := w.RegisterHandlerID("cond.amount>10", condAmountGt10)
	actRef := w.RegisterHandlerID("act.mark42", actMark42)
	u := spawnUnit(w)
	tr, _ := w.Triggers.New()
	w.Triggers.AddEvent(tr, EventReg{Kind: evDamagedDispatch})
	w.Triggers.SetCondition(tr, w.Cond(condRef))
	w.Triggers.AddAction(tr, actRef)
	w.ensureTriggerIndex() // build the index (dirty) once, outside measurement

	e := Event{Kind: evDamagedDispatch, Src: u, Arg: 20}
	n := testing.AllocsPerRun(1000, func() { w.dispatchTriggers(e) })
	t.Logf("dispatchTriggers (condition eval + action run): %v allocs/op", n)
	if n != 0 {
		t.Fatalf("steady-state dispatch allocates %v/op, want 0", n)
	}
}

// TestDispatchObservability — the OnTriggerDispatch hook reports every
// outcome (fired / skip-condition / skip-disabled / skip-stale) in order,
// and installing it never changes the side-effects; nil stays zero-alloc.
func TestDispatchObservability(t *testing.T) {
	w := NewWorld(Caps{})
	condRef := w.RegisterHandlerID("cond.amount>10", condAmountGt10)
	actRef := w.RegisterHandlerID("act.mark42", actMark42)
	u := spawnUnit(w)

	fired, _ := w.Triggers.New() // fires when amount>10
	w.Triggers.AddEvent(fired, EventReg{Kind: evDamagedDispatch})
	w.Triggers.SetCondition(fired, w.Cond(condRef))
	w.Triggers.AddAction(fired, actRef)

	off, _ := w.Triggers.New() // disabled → skip-disabled
	w.Triggers.AddEvent(off, EventReg{Kind: evDamagedDispatch})
	w.Triggers.AddAction(off, actRef)
	w.Triggers.SetEnabled(off, false)

	var log []string
	w.OnTriggerDispatch = func(d TriggerDispatch) {
		log = append(log, d.Outcome.String())
	}

	// amount=5: cond false on `fired`, `off` disabled.
	emitAndStep(w, Event{Kind: evDamagedDispatch, Src: u, Arg: 5})
	t.Logf("amount=5 outcomes=%v (want [skip-condition skip-disabled])", log)
	if len(log) != 2 || log[0] != "skip-condition" || log[1] != "skip-disabled" {
		t.Fatalf("low-amount outcomes wrong: %v", log)
	}

	log = log[:0]
	emitAndStep(w, Event{Kind: evDamagedDispatch, Src: u, Arg: 20})
	t.Logf("amount=20 outcomes=%v (want [fired skip-disabled])", log)
	if len(log) != 2 || log[0] != "fired" || log[1] != "skip-disabled" {
		t.Fatalf("high-amount outcomes wrong: %v", log)
	}
	if w.UserData(u) != 42 {
		t.Fatal("observed dispatch changed side-effects (action did not run)")
	}

	// nil hook: dispatch stays zero-alloc.
	w.OnTriggerDispatch = nil
	w.ensureTriggerIndex()
	e := Event{Kind: evDamagedDispatch, Src: u, Arg: 20}
	if n := testing.AllocsPerRun(1000, func() { w.dispatchTriggers(e) }); n != 0 {
		t.Fatalf("nil-hook dispatch allocates %v/op, want 0", n)
	}
}

// TestDispatchDoubleRunIdentical — edge 4: two runs of the same scenario
// produce identical side-effects and identical state hash.
func TestDispatchDoubleRunIdentical(t *testing.T) {
	reg := NewHashRegistry()
	run := func() (int32, uint64) {
		w := NewWorld(Caps{})
		condRef := w.RegisterHandlerID("cond.amount>10", condAmountGt10)
		actRef := w.RegisterHandlerID("act.mark42", actMark42)
		u := spawnUnit(w)
		tr, _ := w.Triggers.New()
		w.Triggers.AddEvent(tr, EventReg{Kind: evDamagedDispatch})
		w.Triggers.SetCondition(tr, w.Cond(condRef))
		w.Triggers.AddAction(tr, actRef)
		emitAndStep(w, Event{Kind: evDamagedDispatch, Src: u, Arg: 20})
		var s statehash.Snapshot
		w.HashState(reg, &s)
		return w.UserData(u), s.Top
	}
	d1, h1 := run()
	d2, h2 := run()
	t.Logf("run1 UserData=%d top=%016x | run2 UserData=%d top=%016x", d1, h1, d2, h2)
	if d1 != d2 || h1 != h2 {
		t.Fatalf("non-deterministic: data %d/%d hash %016x/%016x", d1, d2, h1, h2)
	}
}

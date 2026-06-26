package sim

// FSV for the periodic-timer trigger primitive (#464, ADR #451). SoT = the
// action's UserData side-effect counter + the scheduler's pending-sleeper
// count + a save/load round-trip. A periodic trigger re-arms itself through a
// value-typed scheduler continuation (no Go closure), so it survives save/load
// — the structural fix for the #450 Game_Every gap.

import (
	"bytes"
	"testing"
)

// actBump increments the source unit's UserData each time it runs — the
// observable proof the periodic fired.
func actBump(w *World, e Event) bool { w.SetUserData(e.Src, w.UserData(e.Src)+1); return true }

// TestPeriodicFiresEveryPeriod — happy path: arm a period-3 trigger; over 10
// ticks it fires at ticks 3,6,9 → counter 3. SoT = the action's counter.
func TestPeriodicFiresEveryPeriod(t *testing.T) {
	w := NewWorld(Caps{})
	u := spawnUnit(w)
	ref := w.RegisterHandlerID("act.bump", func(wr *World, e Event) bool {
		wr.SetUserData(u, wr.UserData(u)+1) // count on the fixed probe unit
		return true
	})
	tr, _ := w.Triggers.New()
	w.Triggers.AddAction(tr, ref)
	w.ArmPeriodic(tr, 3)

	t.Logf("BEFORE: tick=%d counter=%d", w.Tick(), w.UserData(u))
	for i := 0; i < 10; i++ {
		w.Step()
	}
	got := w.UserData(u)
	t.Logf("AFTER 10 ticks (period 3): counter=%d (want 3 — fired at 3,6,9)", got)
	if got != 3 {
		t.Fatalf("periodic fired %d times in 10 ticks at period 3, want 3", got)
	}
}

// TestPeriodicSaveResume — edge 1: save mid-schedule, load into a fresh
// re-registered world, and the periodic keeps firing on the original cadence.
func TestPeriodicSaveResume(t *testing.T) {
	build := func(w *World) (TriggerID, EntityID) {
		u := spawnUnit(w)
		w.RegisterHandlerID("act.bump", func(wr *World, e Event) bool {
			wr.SetUserData(u, wr.UserData(u)+1)
			return true
		})
		tr, _ := w.Triggers.New()
		a, _ := w.HandlerRefOf("act.bump")
		w.Triggers.AddAction(tr, a)
		return tr, u
	}

	src := NewWorld(Caps{})
	tr, u := build(src)
	src.ArmPeriodic(tr, 3)
	for i := 0; i < 7; i++ { // fires at 3,6 → counter 2; next fire scheduled for 9
		src.Step()
	}
	t.Logf("BEFORE save: tick=%d counter=%d sleepers=%d (want counter 2)", src.Tick(), src.UserData(u), src.Sched.PendingSleepers())
	if src.UserData(u) != 2 {
		t.Fatalf("pre-save counter=%d, want 2", src.UserData(u))
	}

	var buf bytes.Buffer
	if err := src.SaveState(&buf, 0); err != nil {
		t.Fatal(err)
	}

	dst := NewWorld(Caps{})
	build(dst) // re-register the same handler name → same ref
	if err := dst.LoadState(bytes.NewReader(buf.Bytes()), 0); err != nil {
		t.Fatalf("load: %v", err)
	}
	t.Logf("LOADED: tick=%d counter=%d sleepers=%d", dst.Tick(), dst.UserData(u), dst.Sched.PendingSleepers())
	if dst.UserData(u) != 2 {
		t.Fatalf("loaded counter=%d, want 2 (side effect lost)", dst.UserData(u))
	}
	if dst.Sched.PendingSleepers() != 1 {
		t.Fatalf("loaded periodic schedule missing: sleepers=%d, want 1", dst.Sched.PendingSleepers())
	}
	// resume: next fire at tick 9, then 12 → over 6 more ticks counter 2→4.
	for i := 0; i < 6; i++ {
		dst.Step()
	}
	t.Logf("RESUMED 6 ticks: tick=%d counter=%d (want 4 — fired at 9,12)", dst.Tick(), dst.UserData(u))
	if dst.UserData(u) != 4 {
		t.Fatalf("periodic did not resume after load: counter=%d, want 4", dst.UserData(u))
	}
}

// TestPeriodicDestroyStops — edge 2: destroying the trigger stops the periodic
// (no re-arm); the counter freezes.
func TestPeriodicDestroyStops(t *testing.T) {
	w := NewWorld(Caps{})
	u := spawnUnit(w)
	ref := w.RegisterHandlerID("act.bump", func(wr *World, e Event) bool {
		wr.SetUserData(u, wr.UserData(u)+1)
		return true
	})
	tr, _ := w.Triggers.New()
	w.Triggers.AddAction(tr, ref)
	w.ArmPeriodic(tr, 2)

	for i := 0; i < 5; i++ { // fires at 2,4 → counter 2
		w.Step()
	}
	mid := w.UserData(u)
	t.Logf("before destroy: counter=%d (want 2)", mid)
	w.Triggers.Destroy(tr)
	for i := 0; i < 6; i++ {
		w.Step()
	}
	end := w.UserData(u)
	t.Logf("after destroy + 6 ticks: counter=%d (want %d — periodic stopped)", end, mid)
	if end != mid {
		t.Fatalf("destroyed periodic kept firing: %d → %d", mid, end)
	}
}

// TestPeriodicDisablePauses — edge 3: a disabled periodic keeps its schedule
// but does not fire; re-enabling resumes it (WC3 paused-timer semantics).
func TestPeriodicDisablePauses(t *testing.T) {
	w := NewWorld(Caps{})
	u := spawnUnit(w)
	ref := w.RegisterHandlerID("act.bump", func(wr *World, e Event) bool {
		wr.SetUserData(u, wr.UserData(u)+1)
		return true
	})
	tr, _ := w.Triggers.New()
	w.Triggers.AddAction(tr, ref)
	w.ArmPeriodic(tr, 2)

	for i := 0; i < 3; i++ { // fire at 2 → counter 1
		w.Step()
	}
	t.Logf("before disable: counter=%d (want 1)", w.UserData(u))
	w.Triggers.SetEnabled(tr, false)
	for i := 0; i < 6; i++ { // disabled window: no fire
		w.Step()
	}
	paused := w.UserData(u)
	t.Logf("after disable + 6 ticks: counter=%d (want 1 — paused, schedule kept)", paused)
	if paused != 1 {
		t.Fatalf("disabled periodic fired: counter=%d, want 1", paused)
	}
	w.Triggers.SetEnabled(tr, true)
	for i := 0; i < 4; i++ { // re-enabled: fires again
		w.Step()
	}
	resumed := w.UserData(u)
	t.Logf("after re-enable + 4 ticks: counter=%d (want >1 — periodic resumed)", resumed)
	if resumed <= paused {
		t.Fatalf("re-enabled periodic did not resume: counter=%d", resumed)
	}
}

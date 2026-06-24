package litd

import (
	"bytes"
	"testing"
	"time"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
)

// #556 — serializable continuation timer API. SoT is the recorded fire
// log (the observable continuation effect) plus the sim store via the
// Timer query methods.

const (
	contTick Cont = 100
	contLog  Cont = 101
)

func TestAfterContFiresWithPayload(t *testing.T) {
	w, g := timerHarness()
	var got []Payload
	g.RegisterCont(contLog, func(_ *Game, p Payload) { got = append(got, p) })

	tm := g.AfterCont(100*time.Millisecond, contLog, Payload{A: 7, B: 8, C: 9, D: 10}) // 2 ticks
	if !tm.Valid() {
		t.Fatal("AfterCont returned invalid timer")
	}
	if tm.FiresRemaining() != 1 {
		t.Fatalf("FiresRemaining = %d, want 1", tm.FiresRemaining())
	}
	stepN(w, 1)
	if len(got) != 0 {
		t.Fatalf("fired early: %v", got)
	}
	stepN(w, 1)
	if len(got) != 1 || got[0] != (Payload{7, 8, 9, 10}) {
		t.Fatalf("fired payloads = %v, want one {7 8 9 10}", got)
	}
	if tm.Valid() {
		t.Fatal("single cont timer still valid after fire")
	}
}

func TestLoopContRepeats(t *testing.T) {
	w, g := timerHarness()
	fires := 0
	g.RegisterCont(contTick, func(_ *Game, _ Payload) { fires++ })
	tm := g.LoopCont(100*time.Millisecond, contTick, Payload{}) // every 2 ticks
	if tm.FiresRemaining() != -1 {
		t.Fatalf("loop FiresRemaining = %d, want -1", tm.FiresRemaining())
	}
	stepN(w, 6) // fires at 2,4,6
	if fires != 3 {
		t.Fatalf("loop fired %d, want 3", fires)
	}
	tm.Cancel()
	stepN(w, 6)
	if fires != 3 {
		t.Fatalf("loop fired after Cancel: %d", fires)
	}
}

func TestCountContFiresNTimes(t *testing.T) {
	w, g := timerHarness()
	fires := 0
	g.RegisterCont(contTick, func(_ *Game, _ Payload) { fires++ })
	g.CountCont(50*time.Millisecond, 4, contTick, Payload{}) // every tick, 4 times
	stepN(w, 20)
	if fires != 4 {
		t.Fatalf("count fired %d, want 4", fires)
	}
}

func TestContTimerPauseResume(t *testing.T) {
	w, g := timerHarness()
	fires := 0
	g.RegisterCont(contTick, func(_ *Game, _ Payload) { fires++ })
	tm := g.LoopCont(100*time.Millisecond, contTick, Payload{})
	stepN(w, 1)
	tm.SetPaused(true)
	if !tm.Paused() {
		t.Fatal("SetPaused(true) did not pause")
	}
	stepN(w, 100)
	if fires != 0 {
		t.Fatalf("paused cont timer fired %d times", fires)
	}
	tm.SetPaused(false)
	if tm.Paused() {
		t.Fatal("still paused after SetPaused(false)")
	}
	stepN(w, 2)
	if fires == 0 {
		t.Fatal("resumed cont timer never fired")
	}
}

func TestLoopContOwnedAutoCancels(t *testing.T) {
	w, g := timerHarness()
	fires := 0
	g.RegisterCont(contTick, func(_ *Game, _ Payload) { fires++ })
	owner, ok := w.CreateUnit(fixed.Vec2{}, 0)
	if !ok {
		t.Fatal("CreateUnit failed")
	}
	g.LoopContOwned(100*time.Millisecond, Unit{id: owner, g: g}, contTick, Payload{})
	stepN(w, 4) // fires at 2,4
	before := fires
	if before == 0 {
		t.Fatal("owned loop never fired")
	}
	w.KillUnit(owner)
	stepN(w, 10)
	if fires != before {
		t.Fatalf("owned loop kept firing after owner death: %d -> %d", before, fires)
	}
}

// The headline R-TMR-2 property: a continuation timer survives save/load
// and fires post-load, because Cont rebinds against the registry that
// world-setup re-registers — no closure is ever serialized.
func TestContTimerSurvivesSaveLoad(t *testing.T) {
	w, g := timerHarness()
	g.RegisterCont(contTick, func(*Game, Payload) {})
	g.LoopCont(150*time.Millisecond, contTick, Payload{A: 42}) // every 3 ticks

	stepN(w, 1)
	var buf bytes.Buffer
	if err := w.SaveState(&buf, 0); err != nil {
		t.Fatalf("SaveState: %v", err)
	}

	// Fresh world+game; re-register the SAME Cont (the rebind step).
	w2 := sim.NewWorld(sim.Caps{})
	g2 := newGame(w2)
	fires := 0
	g2.RegisterCont(contTick, func(_ *Game, p Payload) {
		if p.A != 42 {
			t.Errorf("payload A=%d, want 42 (lost across save)", p.A)
		}
		fires++
	})
	if err := w2.LoadState(bytes.NewReader(buf.Bytes()), 0); err != nil {
		t.Fatalf("LoadState: %v", err)
	}
	stepN(w2, 6)
	if fires == 0 {
		t.Fatal("loaded continuation timer never fired — rebind/rebuild broken")
	}
}

// #557 — save-unsafe contract: a Go-closure After/Every timer is dropped
// from the save (its scheduler record is transient), while a continuation
// timer in the same world survives. SoT: PendingTransient before save, the
// loaded world's timer/scheduler state, and that LoadState SUCCEEDS (the
// closure record would otherwise reference an unregistered ContID).
func TestClosureTimerDroppedOnSave(t *testing.T) {
	w, g := timerHarness()
	g.RegisterCont(contTick, func(*Game, Payload) {})
	clo := g.Every(100*time.Millisecond, func(Timer) {})  // save-unsafe closure
	g.LoopCont(100*time.Millisecond, contTick, Payload{}) // save-safe continuation

	// SoT: the scheduler holds exactly one transient (save-unsafe) record.
	if got := w.Sched.PendingTransient(); got != 1 {
		t.Fatalf("PendingTransient = %d, want 1 (the closure timer)", got)
	}
	if !clo.Valid() {
		t.Fatal("closure timer invalid before save")
	}

	var buf bytes.Buffer
	if err := w.SaveState(&buf, 0); err != nil {
		t.Fatalf("SaveState: %v", err)
	}
	// Save must not kill the source's live closure timer.
	if !clo.Valid() {
		t.Fatal("closure timer dropped from the SOURCE after save (should only be omitted from the blob)")
	}

	// Load into a fresh game that NEVER used closure timers (so contGoTimer
	// is unregistered). Before #557 the saved closure record would make this
	// fail "unregistered ContID"; now it was dropped, so load succeeds.
	w2 := sim.NewWorld(sim.Caps{})
	g2 := newGame(w2)
	g2.RegisterCont(contTick, func(*Game, Payload) {})
	if err := w2.LoadState(bytes.NewReader(buf.Bytes()), 0); err != nil {
		t.Fatalf("LoadState rejected a save with a (dropped) closure timer: %v", err)
	}
	// The continuation timer survived; no closure record came back.
	if w2.Timers.Count() != 1 {
		t.Fatalf("loaded continuation timers = %d, want 1", w2.Timers.Count())
	}
	if w2.Sched.PendingSleepers() != 0 {
		t.Fatalf("loaded scheduler has %d sleepers, want 0 (closure dropped)", w2.Sched.PendingSleepers())
	}
}

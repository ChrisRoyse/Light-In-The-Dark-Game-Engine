package litd

import (
	"testing"
	"time"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
)

// threadHarness builds a fresh world+game (mirrors timerHarness).
func threadHarness() (*sim.World, *Game) {
	w := sim.NewWorld(sim.Caps{})
	return w, newGame(w)
}

// TestThreadPolledWaitResumesOnQuantizedTickFSV — happy path + quantization.
// A thread records the tick at three points: synchronous start, after a
// 100 ms wait (2 ticks), after a 50 ms wait (1 tick). Known input → known
// output: the resume ticks must be exactly [0, 2, 3].
//
// SoT: the scheduler sleep queue (PendingSleepers), the suspended-thread
// counter, and the observed resume ticks (w.Tick) — NOT the wait return.
func TestThreadPolledWaitResumesOnQuantizedTickFSV(t *testing.T) {
	w, g := threadHarness()

	var ticks []uint32
	t.Logf("FSV BEFORE Run: tick=%d pendingSleepers=%d suspended=%d",
		w.Tick(), w.Sched.PendingSleepers(), g.SuspendedThreadCount())

	th := g.Run(func(th *Thread) {
		ticks = append(ticks, w.Tick()) // synchronous slice, tick 0
		th.PolledWait(100 * time.Millisecond)
		ticks = append(ticks, w.Tick()) // resume slice
		th.PolledWait(50 * time.Millisecond)
		ticks = append(ticks, w.Tick()) // resume slice, then return
	})

	// After Run returns, the thread is parked on its first wait: exactly one
	// descriptive record sits on the SHARED scheduler queue, and the save
	// guard sees one parked Go thread.
	t.Logf("FSV AFTER Run (parked on wait 1): ticks=%v valid=%v pendingSleepers=%d suspended=%d",
		ticks, th.Valid(), w.Sched.PendingSleepers(), g.SuspendedThreadCount())
	if len(ticks) != 1 || ticks[0] != 0 {
		t.Fatalf("synchronous start slice ran at ticks=%v, want [0]", ticks)
	}
	if got := w.Sched.PendingSleepers(); got != 1 {
		t.Fatalf("pendingSleepers after Run = %d, want 1 (one parked wait record)", got)
	}
	if got := g.SuspendedThreadCount(); got != 1 {
		t.Fatalf("suspendedThreadCount after Run = %d, want 1", got)
	}
	if !th.Valid() {
		t.Fatalf("thread invalid while parked; want valid until done")
	}

	stepN(w, 1) // tick 1: not yet due (wakes at tick 2)
	t.Logf("FSV after tick 1: ticks=%v pendingSleepers=%d suspended=%d", ticks, w.Sched.PendingSleepers(), g.SuspendedThreadCount())
	if len(ticks) != 1 {
		t.Fatalf("thread resumed early at tick 1: ticks=%v", ticks)
	}

	stepN(w, 1) // tick 2: first wait (100ms=2 ticks) due → resume, re-wait 50ms
	t.Logf("FSV after tick 2 (resume 1): ticks=%v pendingSleepers=%d suspended=%d valid=%v",
		ticks, w.Sched.PendingSleepers(), g.SuspendedThreadCount(), th.Valid())
	if len(ticks) != 2 || ticks[1] != 2 {
		t.Fatalf("resume 1 at ticks=%v, want second resume at tick 2", ticks)
	}
	if got := w.Sched.PendingSleepers(); got != 1 {
		t.Fatalf("pendingSleepers after resume 1 = %d, want 1 (re-armed wait)", got)
	}

	stepN(w, 1) // tick 3: second wait (50ms=1 tick) due → resume, return, done
	t.Logf("FSV after tick 3 (resume 2 + done): ticks=%v pendingSleepers=%d suspended=%d valid=%v",
		ticks, w.Sched.PendingSleepers(), g.SuspendedThreadCount(), th.Valid())
	want := []uint32{0, 2, 3}
	if len(ticks) != 3 || ticks[0] != want[0] || ticks[1] != want[1] || ticks[2] != want[2] {
		t.Fatalf("resume tick sequence = %v, want %v", ticks, want)
	}
	if got := w.Sched.PendingSleepers(); got != 0 {
		t.Fatalf("pendingSleepers after done = %d, want 0", got)
	}
	if got := g.SuspendedThreadCount(); got != 0 {
		t.Fatalf("suspendedThreadCount after done = %d, want 0", got)
	}
	if th.Valid() {
		t.Fatalf("thread still valid after completion; want retired")
	}
}

// TestThreadPolledWaitZeroNoSuspendFSV — edge (1)+(2): PolledWait(0) and a
// negative duration return immediately, SAME tick, with NO suspension
// record created (JASS `if duration > 0` guard).
//
// SoT: pendingSleepers stays 0 across the whole thread; the before/after
// ticks recorded around each guard wait are identical; the thread runs to
// completion entirely within the Run() call (no Step needed).
func TestThreadPolledWaitZeroNoSuspendFSV(t *testing.T) {
	w, g := threadHarness()

	var before, afterZero, afterNeg, end uint32
	sleepersDuring := -1

	th := g.Run(func(th *Thread) {
		before = w.Tick()
		th.PolledWait(0)
		afterZero = w.Tick()
		th.PolledWait(-5 * time.Second)
		afterNeg = w.Tick()
		sleepersDuring = w.Sched.PendingSleepers()
		end = w.Tick()
	})

	t.Logf("FSV guard-wait: before=%d afterZero=%d afterNeg=%d end=%d sleepersDuring=%d pendingSleepersAfter=%d suspended=%d valid=%v",
		before, afterZero, afterNeg, end, sleepersDuring, w.Sched.PendingSleepers(), g.SuspendedThreadCount(), th.Valid())

	if before != 0 || afterZero != 0 || afterNeg != 0 || end != 0 {
		t.Fatalf("guard waits suspended across ticks: before=%d afterZero=%d afterNeg=%d end=%d, want all 0",
			before, afterZero, afterNeg, end)
	}
	if sleepersDuring != 0 {
		t.Fatalf("guard waits created a suspension record: pendingSleepers during=%d, want 0", sleepersDuring)
	}
	if w.Sched.PendingSleepers() != 0 || g.SuspendedThreadCount() != 0 {
		t.Fatalf("residual suspension after guard-only thread: sleepers=%d suspended=%d",
			w.Sched.PendingSleepers(), g.SuspendedThreadCount())
	}
	if th.Valid() {
		t.Fatalf("guard-only thread still valid; should have completed inside Run")
	}
}

// TestThreadResumeOrderDeterministicFSV — edge (3): two threads parked on
// the same wake tick resume in spawn (registration-seq) order, identically
// across two fresh worlds (replay safety, S-2).
//
// SoT: the recorded resume-order slice, compared between runs.
func TestThreadResumeOrderDeterministicFSV(t *testing.T) {
	run := func() []int {
		w, g := threadHarness()
		order := []int{}
		// A then B, both wait 100 ms (→ wake tick 2). A registered first.
		g.Run(func(th *Thread) { th.PolledWait(100 * time.Millisecond); order = append(order, 1) })
		g.Run(func(th *Thread) { th.PolledWait(100 * time.Millisecond); order = append(order, 2) })
		stepN(w, 3)
		return order
	}
	r1 := run()
	r2 := run()
	t.Logf("FSV same-tick thread resume order run1=%v run2=%v", r1, r2)
	if len(r1) != 2 || r1[0] != 1 || r1[1] != 2 {
		t.Fatalf("same-tick resume order = %v, want spawn order [1 2]", r1)
	}
	if r1[0] != r2[0] || r1[1] != r2[1] {
		t.Fatalf("resume order nondeterministic: run1=%v run2=%v", r1, r2)
	}
}

// TestThreadQuantizationCeilingFSV — quantization boundaries (R-EXEC-5):
// a sub-tick wait (1 ms) takes one full tick; a 1.5-tick wait (75 ms)
// ceilings to two ticks.
//
// SoT: the observed resume tick for each wait value.
func TestThreadQuantizationCeilingFSV(t *testing.T) {
	resumeTickFor := func(d time.Duration) uint32 {
		w, g := threadHarness()
		var resume uint32
		g.Run(func(th *Thread) { th.PolledWait(d); resume = w.Tick() })
		stepN(w, 10)
		return resume
	}
	one := resumeTickFor(1 * time.Millisecond)    // sub-tick → 1 tick
	floor := resumeTickFor(50 * time.Millisecond) // exactly 1 tick
	ceil := resumeTickFor(75 * time.Millisecond)  // 1.5 ticks → ceil 2
	two := resumeTickFor(100 * time.Millisecond)  // exactly 2 ticks
	t.Logf("FSV quantization: 1ms→tick%d 50ms→tick%d 75ms→tick%d 100ms→tick%d", one, floor, ceil, two)
	if one != 1 || floor != 1 {
		t.Fatalf("one-tick floor wrong: 1ms→%d 50ms→%d, want both 1", one, floor)
	}
	if ceil != 2 {
		t.Fatalf("75ms ceiling wrong: →%d, want 2", ceil)
	}
	if two != 2 {
		t.Fatalf("100ms wrong: →%d, want 2", two)
	}
}

// TestThreadAmbientCurrentThreadFSV — the argument-free ambient surface
// that backs helpers.PolledWait: while a thread runs, CurrentThread()
// returns it; outside any thread it is nil.
//
// SoT: the identity captured inside the slice vs the running Thread, and
// the nil reading after completion.
func TestThreadAmbientCurrentThreadFSV(t *testing.T) {
	w, g := threadHarness()

	if CurrentThread() != nil {
		t.Fatalf("CurrentThread outside any thread = %p, want nil", CurrentThread())
	}

	var insideStart, insideResume *Thread
	th := g.Run(func(self *Thread) {
		insideStart = CurrentThread()
		self.PolledWait(50 * time.Millisecond)
		insideResume = CurrentThread()
	})
	t.Logf("FSV ambient: running=%p insideStart=%p (afterRun, before resume) outside=%p", th, insideStart, CurrentThread())
	if insideStart != th {
		t.Fatalf("CurrentThread inside start slice = %p, want running thread %p", insideStart, th)
	}
	if CurrentThread() != nil {
		t.Fatalf("CurrentThread after Run returned (thread parked) = %p, want nil", CurrentThread())
	}

	stepN(w, 1)
	t.Logf("FSV ambient after resume: insideResume=%p running=%p outside=%p", insideResume, th, CurrentThread())
	if insideResume != th {
		t.Fatalf("CurrentThread inside resume slice = %p, want %p", insideResume, th)
	}
	if CurrentThread() != nil {
		t.Fatalf("CurrentThread after completion = %p, want nil", CurrentThread())
	}
}

// TestThreadNilSafetyFSV — nil receivers / nil fn are clean no-ops.
func TestThreadNilSafetyFSV(t *testing.T) {
	_, g := threadHarness()
	if got := g.Run(nil); got != nil {
		t.Fatalf("Run(nil fn) = %v, want nil thread", got)
	}
	var gNil *Game
	if got := gNil.Run(func(*Thread) {}); got != nil {
		t.Fatalf("nilGame.Run = %v, want nil", got)
	}
	var tNil *Thread
	tNil.PolledWait(time.Second) // must not panic
	if tNil.Valid() {
		t.Fatalf("nil thread Valid() = true, want false")
	}
	if gNil.SuspendedThreadCount() != 0 {
		t.Fatalf("nilGame.SuspendedThreadCount != 0")
	}
}

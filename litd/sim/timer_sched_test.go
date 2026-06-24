package sim

import (
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim/sched"
)

// #553 — schedule index + advance drain. SoT = the heap (fire order)
// and the store columns (live/WakeTick) read after each advance.

// fireLog records the (tick, payload) of every timer continuation that
// ran, in fire order — the observable outcome of advance().
type fireLog struct {
	ticks []uint32
	args  []int64
}

// newSchedWithLogger returns a scheduler whose continuation `cont`
// appends to log the current tick and st[0] each time it fires.
func newSchedWithLogger(cont sched.ContID, log *fireLog) *sched.Scheduler {
	sc := sched.New()
	sc.Register(cont, func(s *sched.Scheduler, st sched.State) {
		log.ticks = append(log.ticks, s.Now())
		log.args = append(log.args, st[0])
	})
	return sc
}

// drive advances both the scheduler clock and the timer store one tick
// at a time up to and including `toTick`, mirroring scriptPhase.
func drive(s *TimerStore, sc *sched.Scheduler, toTick uint32) {
	for sc.Now() < toTick {
		sc.Step()
		s.advance(sc.Now(), sc)
	}
}

func TestTimerAdvanceFiresOnWakeTick(t *testing.T) {
	var log fireLog
	sc := newSchedWithLogger(1, &log)
	s := NewTimerStore(16)
	// Single timer: created at now=0, interval 3 → fires at tick 3.
	s.Create(sc.Now(), TimerSingle, 3, 0, 1, [4]int64{99}, 0)
	if s.HeapLen() != 1 {
		t.Fatalf("HeapLen after create = %d, want 1", s.HeapLen())
	}
	drive(s, sc, 2)
	if len(log.ticks) != 0 {
		t.Fatalf("fired early at ticks %v", log.ticks)
	}
	drive(s, sc, 3)
	if len(log.ticks) != 1 || log.ticks[0] != 3 {
		t.Fatalf("fire ticks = %v, want [3]", log.ticks)
	}
	if log.args[0] != 99 {
		t.Fatalf("payload = %d, want 99", log.args[0])
	}
	// Single timer is gone from both pool and heap.
	if s.Count() != 0 || s.HeapLen() != 0 {
		t.Fatalf("after single fire: count=%d heap=%d, want 0,0", s.Count(), s.HeapLen())
	}
}

func TestTimerAdvanceSameTickOrderBySeq(t *testing.T) {
	var log fireLog
	sc := newSchedWithLogger(1, &log)
	s := NewTimerStore(16)
	// Three timers all waking at tick 5, created in payload order 10,20,30.
	// Fire order must be allocation (Seq) order: 10,20,30 — NOT heap-
	// insertion accident.
	s.Create(0, TimerSingle, 5, 0, 1, [4]int64{10}, 0)
	s.Create(0, TimerSingle, 5, 0, 1, [4]int64{20}, 0)
	s.Create(0, TimerSingle, 5, 0, 1, [4]int64{30}, 0)
	drive(s, sc, 5)
	want := []int64{10, 20, 30}
	if len(log.args) != 3 {
		t.Fatalf("fired %d, want 3 (%v)", len(log.args), log.args)
	}
	for i := range want {
		if log.args[i] != want[i] {
			t.Fatalf("fire order = %v, want %v", log.args, want)
		}
	}
}

func TestTimerAdvanceDifferentTicksAscending(t *testing.T) {
	var log fireLog
	sc := newSchedWithLogger(1, &log)
	s := NewTimerStore(16)
	// Out-of-order creation, in-order fire by WakeTick.
	s.Create(0, TimerSingle, 7, 0, 1, [4]int64{7}, 0)
	s.Create(0, TimerSingle, 2, 0, 1, [4]int64{2}, 0)
	s.Create(0, TimerSingle, 4, 0, 1, [4]int64{4}, 0)
	drive(s, sc, 10)
	want := []uint32{2, 4, 7}
	if len(log.ticks) != 3 {
		t.Fatalf("fired ticks %v", log.ticks)
	}
	for i := range want {
		if log.ticks[i] != want[i] {
			t.Fatalf("fire ticks = %v, want %v", log.ticks, want)
		}
	}
}

func TestTimerAdvanceLoop(t *testing.T) {
	var log fireLog
	sc := newSchedWithLogger(1, &log)
	s := NewTimerStore(16)
	s.Create(0, TimerLoop, 3, 0, 1, [4]int64{1}, 0)
	drive(s, sc, 10) // fires at 3,6,9
	want := []uint32{3, 6, 9}
	if len(log.ticks) != 3 {
		t.Fatalf("loop fired ticks %v, want %v", log.ticks, want)
	}
	for i := range want {
		if log.ticks[i] != want[i] {
			t.Fatalf("loop ticks = %v, want %v", log.ticks, want)
		}
	}
	// Still live & still scheduled.
	if s.Count() != 1 || s.HeapLen() != 1 {
		t.Fatalf("loop after fires: count=%d heap=%d", s.Count(), s.HeapLen())
	}
}

func TestTimerAdvanceCount(t *testing.T) {
	var log fireLog
	sc := newSchedWithLogger(1, &log)
	s := NewTimerStore(16)
	s.Create(0, TimerCount, 2, 3, 1, [4]int64{}, 0) // fires at 2,4,6 then frees
	drive(s, sc, 20)
	if len(log.ticks) != 3 {
		t.Fatalf("count fired %d times (%v), want 3", len(log.ticks), log.ticks)
	}
	if s.Count() != 0 || s.HeapLen() != 0 {
		t.Fatalf("count after exhaustion: count=%d heap=%d, want 0,0", s.Count(), s.HeapLen())
	}
}

func TestTimerCancelRemovesHeapEntry(t *testing.T) {
	var log fireLog
	sc := newSchedWithLogger(1, &log)
	s := NewTimerStore(16)
	a := s.Create(0, TimerSingle, 5, 0, 1, [4]int64{1}, 0)
	s.Create(0, TimerSingle, 5, 0, 1, [4]int64{2}, 0)
	if s.HeapLen() != 2 {
		t.Fatalf("heap=%d, want 2", s.HeapLen())
	}
	// Cancel a BEFORE it fires — heap must shrink immediately (no stale
	// entry left to accumulate), and a must never fire.
	s.Cancel(a)
	if s.HeapLen() != 1 {
		t.Fatalf("heap after cancel = %d, want 1", s.HeapLen())
	}
	drive(s, sc, 5)
	if len(log.args) != 1 || log.args[0] != 2 {
		t.Fatalf("fired %v, want only [2]", log.args)
	}
}

func TestTimerSelfCancelInContinuation(t *testing.T) {
	// A loop timer whose continuation cancels itself must fire exactly
	// once and leave no live/scheduled state — the post-fire re-resolve
	// guards against a double-free / reschedule of a dead slot.
	s := NewTimerStore(16)
	sc := sched.New()
	var self TimerID
	fired := 0
	sc.Register(1, func(_ *sched.Scheduler, _ sched.State) {
		fired++
		s.Cancel(self)
	})
	self = s.Create(0, TimerLoop, 2, 0, 1, [4]int64{}, 0)
	drive(s, sc, 20)
	if fired != 1 {
		t.Fatalf("self-cancel fired %d times, want 1", fired)
	}
	if s.Count() != 0 || s.HeapLen() != 0 {
		t.Fatalf("after self-cancel: count=%d heap=%d, want 0,0", s.Count(), s.HeapLen())
	}
}

func TestTimerContinuationCreatesTimer(t *testing.T) {
	// A continuation that creates a new timer must not fire it in the
	// same drain (1-tick floor) — the drain terminates.
	s := NewTimerStore(16)
	sc := sched.New()
	var log fireLog
	chained := 0
	sc.Register(1, func(c *sched.Scheduler, st sched.State) {
		log.ticks = append(log.ticks, c.Now())
		if chained < 3 {
			chained++
			s.Create(c.Now(), TimerSingle, 1, 0, 1, [4]int64{}, 0)
		}
	})
	s.Create(0, TimerSingle, 1, 0, 1, [4]int64{}, 0)
	drive(s, sc, 10)
	// tick1 fires original; each fire chains one more → ticks 1,2,3,4.
	want := []uint32{1, 2, 3, 4}
	if len(log.ticks) != len(want) {
		t.Fatalf("chain ticks = %v, want %v", log.ticks, want)
	}
	for i := range want {
		if log.ticks[i] != want[i] {
			t.Fatalf("chain ticks = %v, want %v", log.ticks, want)
		}
	}
}

func TestTimerAdvanceDeterministic(t *testing.T) {
	run := func() []uint32 {
		var log fireLog
		sc := newSchedWithLogger(1, &log)
		s := NewTimerStore(64)
		// A mix of modes and intervals.
		s.Create(0, TimerLoop, 3, 0, 1, [4]int64{1}, 0)
		s.Create(0, TimerCount, 2, 4, 1, [4]int64{2}, 0)
		s.Create(0, TimerSingle, 5, 0, 1, [4]int64{3}, 0)
		s.Create(0, TimerLoop, 4, 0, 1, [4]int64{4}, 0)
		drive(s, sc, 24)
		return log.ticks
	}
	a, b := run(), run()
	if len(a) != len(b) {
		t.Fatalf("nondeterministic length %d vs %d", len(a), len(b))
	}
	for i := range a {
		if a[i] != b[i] {
			t.Fatalf("nondeterministic at %d: %d vs %d", i, a[i], b[i])
		}
	}
}

func TestTimerAdvanceZeroAlloc(t *testing.T) {
	s := NewTimerStore(64)
	sc := sched.New()
	sc.Register(1, func(_ *sched.Scheduler, _ sched.State) {})
	s.Create(0, TimerLoop, 1, 0, 1, [4]int64{}, 0) // fires every tick
	avg := testing.AllocsPerRun(2000, func() {
		sc.Step()
		s.advance(sc.Now(), sc)
	})
	if avg != 0 {
		t.Fatalf("advance churn allocated %.2f objs/op, want 0", avg)
	}
}

// Integration: a timer armed on the World fires through the real
// 7-phase Step at phase 2 (scriptPhase), proving the wiring — not just
// the store-level drive() helper.
func TestTimerFiresThroughWorldStep(t *testing.T) {
	w := NewWorld(Caps{Units: 8})
	fired := []uint32{}
	w.Sched.Register(1, func(s *sched.Scheduler, _ sched.State) {
		fired = append(fired, s.Now())
	})
	// Arm at the current tick (0) to fire 2 ticks later.
	w.Timers.Create(w.Tick(), TimerSingle, 2, 0, 1, [4]int64{}, 0)
	w.Step() // tick 1
	if len(fired) != 0 {
		t.Fatalf("fired early: %v", fired)
	}
	w.Step() // tick 2 — should fire
	if len(fired) != 1 || fired[0] != 2 {
		t.Fatalf("fired = %v, want [2]", fired)
	}
	if w.Timers.Count() != 0 {
		t.Fatalf("single timer not freed: count=%d", w.Timers.Count())
	}
}

// #554 — owner auto-cancel on death. An owned timer must not outlive
// its owner; an unowned timer (and a timer owned by a survivor) is
// untouched.
func TestTimerOwnerAutoCancelOnDeath(t *testing.T) {
	w := NewWorld(Caps{Units: 16})
	fired := map[string]int{}
	w.Sched.Register(1, func(_ *sched.Scheduler, st sched.State) {
		switch st[0] {
		case 1:
			fired["owned"]++
		case 2:
			fired["unowned"]++
		case 3:
			fired["survivor"]++
		}
	})
	doomed, _ := w.CreateUnit(fixed.Vec2{}, 0)
	survivor, _ := w.CreateUnit(fixed.Vec2{}, 0)

	// Loop timers so they would fire repeatedly if not cancelled.
	owned := w.Timers.Create(w.Tick(), TimerLoop, 2, 0, 1, [4]int64{1}, doomed)
	w.Timers.Create(w.Tick(), TimerLoop, 2, 0, 1, [4]int64{2}, 0)              // unowned
	w.Timers.Create(w.Tick(), TimerLoop, 2, 0, 1, [4]int64{3}, survivor)       // owned by survivor

	if w.Timers.Count() != 3 {
		t.Fatalf("setup: count=%d, want 3", w.Timers.Count())
	}
	// Let them fire once (tick 2).
	w.Step() // 1
	w.Step() // 2 — all three fire
	if fired["owned"] != 1 || fired["unowned"] != 1 || fired["survivor"] != 1 {
		t.Fatalf("first fire counts = %v, want all 1", fired)
	}
	// Kill the doomed owner; its timer must be auto-cancelled in phase 7.
	w.KillUnit(doomed)
	w.Step() // 3 — cleanup cancels owned timer (no fire this tick anyway)
	if w.Timers.Alive(owned) {
		t.Fatal("owned timer still alive after owner death")
	}
	if w.Timers.Count() != 2 {
		t.Fatalf("count after owner death = %d, want 2", w.Timers.Count())
	}
	// Run more ticks: owned must never fire again; the other two keep going.
	before := fired["owned"]
	w.Step() // 4 — unowned+survivor fire
	w.Step() // 5
	if fired["owned"] != before {
		t.Fatalf("owned timer fired after owner death: %d -> %d", before, fired["owned"])
	}
	if fired["unowned"] < 2 || fired["survivor"] < 2 {
		t.Fatalf("survivors stopped firing: %v", fired)
	}
}

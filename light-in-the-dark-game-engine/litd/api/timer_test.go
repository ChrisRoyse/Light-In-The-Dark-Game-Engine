package litd

import (
	"testing"
	"time"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
)

// timerHarness builds a fresh world+game and returns a stepper that
// advances whole sim ticks (the real phase-2 scheduler drain path).
func timerHarness() (*sim.World, *Game) {
	w := sim.NewWorld(sim.Caps{})
	return w, newGame(w)
}

// TestTimerAfterOneShotFSV — happy path + edge (1): After fires exactly
// once, on the quantized tick, then the handle goes invalid.
// SoT: the recorded fire-tick and the timer table (Valid/Remaining).
func TestTimerAfterOneShotFSV(t *testing.T) {
	w, g := timerHarness()

	fired := []uint32{}
	t1 := g.After(1*time.Millisecond, func() { fired = append(fired, w.Tick()) })

	// Edge (1): a 1 ms request quantizes to the one-tick floor (50 ms);
	// scheduled wake = current tick + 1.
	t.Logf("FSV After(1ms) BEFORE: tick=%d valid=%v timeout=%v remaining=%v elapsed=%v pendingSleepers=%d",
		w.Tick(), t1.Valid(), t1.Timeout(), t1.Remaining(), t1.Elapsed(), w.Sched.PendingSleepers())
	if t1.Timeout() != 50*time.Millisecond {
		t.Fatalf("timeout = %v, want 50ms (one-tick floor)", t1.Timeout())
	}
	if t1.Remaining() != 50*time.Millisecond {
		t.Fatalf("remaining at creation = %v, want 50ms", t1.Remaining())
	}

	stepN(w, 1)
	t.Logf("FSV After(1ms) AFTER 1 tick: tick=%d fired=%v valid=%v pendingSleepers=%d",
		w.Tick(), fired, t1.Valid(), w.Sched.PendingSleepers())
	if len(fired) != 1 || fired[0] != 1 {
		t.Fatalf("one-shot fired=%v, want exactly [1]", fired)
	}
	if t1.Valid() {
		t.Fatalf("one-shot still valid after firing; want auto-retired")
	}

	// Run far past expiry: it must never fire again.
	stepN(w, 50)
	if len(fired) != 1 {
		t.Fatalf("one-shot fired %d times, want 1: %v", len(fired), fired)
	}
}

// TestTimerSameTickOrderFSV — edge (2): two timers expiring on the same
// tick fire in creation order, identically across runs (replay safety).
// SoT: the recorded fire-order slice, compared between two fresh worlds.
func TestTimerSameTickOrderFSV(t *testing.T) {
	run := func() []int {
		w, g := timerHarness()
		order := []int{}
		// Both quantize to tick 2 (100 ms). Created A then B.
		g.After(100*time.Millisecond, func() { order = append(order, 1) }) // A
		g.After(100*time.Millisecond, func() { order = append(order, 2) }) // B
		stepN(w, 3)
		return order
	}
	r1 := run()
	r2 := run()
	t.Logf("FSV same-tick order run1=%v run2=%v", r1, r2)
	if len(r1) != 2 || r1[0] != 1 || r1[1] != 2 {
		t.Fatalf("same-tick order = %v, want creation order [1 2]", r1)
	}
	if r1[0] != r2[0] || r1[1] != r2[1] {
		t.Fatalf("same-tick order nondeterministic: run1=%v run2=%v", r1, r2)
	}
}

// TestTimerPauseResumeFreezesFSV — edge (3): Pause mid-countdown freezes
// the remaining time exactly across 10 idle ticks; Resume continues from
// the frozen remainder with no drift and no missed fire.
// SoT: Remaining() before/during/after pause and the actual fire tick.
func TestTimerPauseResumeFreezesFSV(t *testing.T) {
	w, g := timerHarness()
	fired := []uint32{}
	// Every 200 ms = 4 ticks.
	tm := g.Every(200*time.Millisecond, func(Timer) { fired = append(fired, w.Tick()) })

	stepN(w, 1) // now tick 1; remaining 3 ticks = 150 ms
	remBefore := tm.Remaining()
	t.Logf("FSV pause BEFORE: tick=%d remaining=%v", w.Tick(), remBefore)
	tm.Pause()
	remPaused := tm.Remaining()

	stepN(w, 10) // idle while paused
	remStillPaused := tm.Remaining()
	t.Logf("FSV pause DURING (paused 10 ticks): tick=%d remaining=%v firesSoFar=%v",
		w.Tick(), remStillPaused, fired)

	if remBefore != 150*time.Millisecond {
		t.Fatalf("remaining before pause = %v, want 150ms", remBefore)
	}
	if remPaused != 150*time.Millisecond || remStillPaused != 150*time.Millisecond {
		t.Fatalf("remaining not frozen: atPause=%v after10=%v, want 150ms", remPaused, remStillPaused)
	}
	if len(fired) != 0 {
		t.Fatalf("paused timer fired while frozen: %v", fired)
	}

	tm.Resume()
	resumeTick := w.Tick()
	stepN(w, 3) // frozen remainder was 3 ticks
	t.Logf("FSV pause AFTER resume(@tick %d) +3 ticks: tick=%d fired=%v remaining=%v",
		resumeTick, w.Tick(), fired, tm.Remaining())
	if len(fired) != 1 || fired[0] != resumeTick+3 {
		t.Fatalf("resumed fire = %v, want exactly [%d] (3 ticks after resume, no drift)", fired, resumeTick+3)
	}
}

// TestTimerStopInCallbackFSV — edge (4): a periodic timer that Stops
// itself inside its own callback fires exactly once.
// SoT: the fire count and the handle validity afterward.
func TestTimerStopInCallbackFSV(t *testing.T) {
	w, g := timerHarness()
	count := 0
	var self Timer
	self = g.Every(50*time.Millisecond, func(t Timer) {
		count++
		t.Stop() // t and self are the same identity
	})
	_ = self
	stepN(w, 20)
	t.Logf("FSV stop-in-callback: count=%d valid=%v pendingSleepers=%d", count, self.Valid(), w.Sched.PendingSleepers())
	if count != 1 {
		t.Fatalf("self-stopping periodic fired %d times, want 1", count)
	}
	if self.Valid() {
		t.Fatalf("self-stopped timer still valid")
	}
}

// TestTimerPeriodicNoDriftFSV — Every fires on exact tick multiples of
// its period, with no accumulating drift, over many cycles.
// SoT: the full fire-tick table.
func TestTimerPeriodicNoDriftFSV(t *testing.T) {
	w, g := timerHarness()
	fired := []uint32{}
	g.Every(100*time.Millisecond, func(Timer) { fired = append(fired, w.Tick()) }) // 2 ticks
	stepN(w, 20)
	want := []uint32{2, 4, 6, 8, 10, 12, 14, 16, 18, 20}
	t.Logf("FSV periodic fire ticks=%v want=%v", fired, want)
	if len(fired) != len(want) {
		t.Fatalf("fire count = %d, want %d: %v", len(fired), len(want), fired)
	}
	for i := range want {
		if fired[i] != want[i] {
			t.Fatalf("fire[%d] = %d, want %d (drift) — full=%v", i, fired[i], want[i], fired)
		}
	}
}

// TestTimerStopIdempotentAndStaleHandleFSV — Stop is idempotent and a
// stale handle (post-Stop, or to a recycled slot) is a safe no-op.
// SoT: validity transitions and that a stale handle never disturbs the
// live timer occupying its recycled slot.
func TestTimerStopIdempotentAndStaleHandleFSV(t *testing.T) {
	_, g := timerHarness()

	t1 := g.After(time.Second, func() {})
	t.Logf("FSV stale BEFORE: t1.valid=%v", t1.Valid())
	if !t1.Valid() {
		t.Fatalf("fresh timer invalid")
	}
	t1.Stop()
	if t1.Valid() {
		t.Fatalf("timer valid after Stop")
	}
	t1.Stop() // idempotent: must not panic

	// Recycle the slot under a fresh generation.
	t2 := g.After(time.Second, func() {})
	t.Logf("FSV stale AFTER recycle: t1.slot=%d t2.slot=%d t1.gen=%d t2.gen=%d t1.valid=%v t2.valid=%v",
		t1.slot, t2.slot, t1.gen, t2.gen, t1.Valid(), t2.Valid())
	if t1.slot != t2.slot {
		t.Fatalf("expected slot reuse: t1.slot=%d t2.slot=%d", t1.slot, t2.slot)
	}
	if t1.gen == t2.gen {
		t.Fatalf("generation not bumped on recycle: both %d", t1.gen)
	}
	if t1.Valid() {
		t.Fatalf("stale handle valid after slot recycle (aliasing!)")
	}
	if !t2.Valid() {
		t.Fatalf("recycled-slot timer invalid")
	}
	// Stale t1.Stop() must not retire the live t2 in the shared slot.
	t1.Stop()
	if !t2.Valid() {
		t.Fatalf("stale handle Stop() destroyed the live timer in its slot")
	}
}

// TestTimerZeroValueNoOpFSV — the zero-value Timer and a nil-callback
// constructor degrade to safe no-ops (R-API-5).
func TestTimerZeroValueNoOpFSV(t *testing.T) {
	var z Timer
	// None of these may panic.
	z.Pause()
	z.Resume()
	z.Stop()
	t.Logf("FSV zero-value: valid=%v timeout=%v remaining=%v elapsed=%v",
		z.Valid(), z.Timeout(), z.Remaining(), z.Elapsed())
	if z.Valid() || z.Timeout() != 0 || z.Remaining() != 0 || z.Elapsed() != 0 {
		t.Fatalf("zero-value timer not inert")
	}

	_, g := timerHarness()
	if nt := g.After(time.Second, nil); nt.Valid() {
		t.Fatalf("After(nil) returned a valid timer")
	}
	if nt := g.Every(time.Second, nil); nt.Valid() {
		t.Fatalf("Every(nil) returned a valid timer")
	}
}

// TestTimerSteadyStateZeroAllocFSV — R-GC-1: 500 periodic timers firing
// and re-arming every tick add zero per-tick allocations at steady
// state. SoT: testing.AllocsPerRun delta over a warmed-up scheduler.
func TestTimerSteadyStateZeroAllocFSV(t *testing.T) {
	w, g := timerHarness()
	const n = 500
	for i := 0; i < n; i++ {
		g.Every(50*time.Millisecond, func(Timer) {}) // 1 tick: fires every tick
	}
	// Warm up: grow the sched heap and any slices to steady capacity.
	stepN(w, 10)

	allocs := testing.AllocsPerRun(50, func() { w.Step() })
	t.Logf("FSV %d-timer steady-state allocs/tick = %.2f (pendingSleepers=%d)",
		n, allocs, w.Sched.PendingSleepers())
	if w.Sched.PendingSleepers() != n {
		t.Fatalf("expected %d live records, got %d", n, w.Sched.PendingSleepers())
	}
	if allocs != 0 {
		t.Fatalf("steady-state allocs/tick = %.2f, want 0 (R-GC-1)", allocs)
	}
}

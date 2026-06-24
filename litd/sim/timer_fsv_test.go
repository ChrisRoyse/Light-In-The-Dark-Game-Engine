package sim

import (
	"bytes"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim/sched"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/statehash"
)

// #559 — the timer-wheel acceptance suite: the cross-cutting properties
// the per-feature tests (#551–#557, #611) don't assert together —
// many-tick determinism, save/resume hash parity, and a recorded golden
// so a cross-platform divergence is caught. Per-feature behavior
// (modes/order/quantize/exhaustion/owner/pause/closure-drop) lives in
// timer_test.go, timer_sched_test.go, timer_save_test.go.

// timerScenario arms a deterministic, mixed timer population on w. The
// continuations only churn the timer store itself (create/cancel), which
// is hashed state — so no external SoT is needed; the "timers" sub-hash
// is the verdict. ContID 1 is a no-op; ContID 2 re-arms a short single
// timer each fire (steady creation pressure exercising slot reuse).
func timerScenario(w *World) {
	w.Sched.Register(1, func(_ *sched.Scheduler, _ sched.State) {})
	w.Sched.Register(2, func(s *sched.Scheduler, st sched.State) {
		// Re-arm a fresh single timer 3 ticks out — keeps the pool
		// churning so slot reuse + seq advance are exercised every cycle.
		w.Timers.Create(s.Now(), TimerSingle, 3, 0, 1, [4]int64{st[0] + 1}, 0)
	})
	w.Timers.Create(0, TimerLoop, 3, 0, 1, [4]int64{10}, 0)
	w.Timers.Create(0, TimerCount, 2, 8, 1, [4]int64{20}, 0)
	w.Timers.Create(0, TimerLoop, 5, 0, 2, [4]int64{30}, 0) // self-propagating
	w.Timers.Create(0, TimerSingle, 7, 0, 1, [4]int64{40}, 0)
	w.Timers.Create(0, TimerLoop, 11, 0, 2, [4]int64{50}, 0)
}

func timerTopHash(w *World, reg *statehash.Registry) uint64 {
	var s statehash.Snapshot
	w.HashState(reg, &s)
	return s.Top
}

// TestTimerScenarioDeterministic — two independent worlds running the
// identical scenario for 300 ticks must reach a bit-identical state hash.
func TestTimerScenarioDeterministic(t *testing.T) {
	reg := NewHashRegistry()
	run := func() uint64 {
		w := NewWorld(Caps{Units: 8, Timers: 256})
		timerScenario(w)
		for i := 0; i < 300; i++ {
			w.Step()
		}
		return timerTopHash(w, reg)
	}
	a, b := run(), run()
	if a != b {
		t.Fatalf("timer scenario nondeterministic: %016x != %016x", a, b)
	}
	t.Logf("FSV: 300-tick mixed-timer scenario deterministic, hash=%016x", a)
}

// TestTimerScenarioSaveResumeParity — saving mid-run and resuming in a
// fresh world reaches the SAME hash at tick 300 as a world that ran
// straight through. The headline R-TMR-2/R-TMR-7 property: timers
// round-trip and keep firing identically across a save boundary.
func TestTimerScenarioSaveResumeParity(t *testing.T) {
	reg := NewHashRegistry()

	// Straight run to 300.
	straight := NewWorld(Caps{Units: 8, Timers: 256})
	timerScenario(straight)
	for i := 0; i < 300; i++ {
		straight.Step()
	}
	want := timerTopHash(straight, reg)

	// Run to 150, save, load into a fresh world, resume to 300.
	src := NewWorld(Caps{Units: 8, Timers: 256})
	timerScenario(src)
	for i := 0; i < 150; i++ {
		src.Step()
	}
	var buf bytes.Buffer
	if err := src.SaveState(&buf, 0); err != nil {
		t.Fatalf("SaveState: %v", err)
	}
	dst := NewWorld(Caps{Units: 8, Timers: 256})
	// Re-register the continuation registry on the load target (code, not
	// state) before loading — exactly how a real world-setup rebinds.
	timerScenarioConts(dst)
	if err := dst.LoadState(bytes.NewReader(buf.Bytes()), 0); err != nil {
		t.Fatalf("LoadState: %v", err)
	}
	for i := 0; i < 150; i++ {
		dst.Step()
	}
	got := timerTopHash(dst, reg)
	if got != want {
		t.Fatalf("save/resume hash %016x != straight-run hash %016x", got, want)
	}
	t.Logf("FSV: save@150 + resume to 300 == straight run, hash=%016x", got)
}

// timerScenarioConts registers only the continuation registry (no timer
// creation) — the rebind step a loaded world performs at setup.
func timerScenarioConts(w *World) {
	w.Sched.Register(1, func(_ *sched.Scheduler, _ sched.State) {})
	w.Sched.Register(2, func(s *sched.Scheduler, st sched.State) {
		w.Timers.Create(s.Now(), TimerSingle, 3, 0, 1, [4]int64{st[0] + 1}, 0)
	})
}

// TestTimerScenarioGolden — a recorded cross-platform fingerprint. A
// divergence here on any platform localizes a timer-wheel determinism
// break. Update only when the scenario or hash vocabulary intentionally
// changes.
func TestTimerScenarioGolden(t *testing.T) {
	reg := NewHashRegistry()
	w := NewWorld(Caps{Units: 8, Timers: 256})
	timerScenario(w)
	for i := 0; i < 300; i++ {
		w.Step()
	}
	// Tracks HashSystems membership (each change is a constant full-state
	// shift; scenario unchanged): unitgroups+ (#565) → kv+ (#572) →
	// userdata− (#571) → customevents+ (#617) ⇒ 0xdc6ee40391256cff.
	const golden = uint64(0x0d4004f3ea190807) // rebumped #590 "missiles" sub-hash retired
	got := timerTopHash(w, reg)
	if golden != 0 && got != golden {
		t.Fatalf("timer golden hash %016x != recorded %016x (intended change? update golden)", got, golden)
	}
	t.Logf("FSV timer golden: %016x", got)
}

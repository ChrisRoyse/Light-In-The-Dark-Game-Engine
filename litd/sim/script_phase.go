package sim

import "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim/sched"

// Phase 2 wiring (tick-and-scheduler.md §3.1, §4 phase 2): the
// production stackless scheduler drains its sleeper queue inside the
// world tick, in strict (wakeTick, seq) order, exactly one suspension
// at a time on the sim goroutine (R-EXEC-1). The scheduler tick
// advances in lockstep with the world tick — Sched.Now() == Tick()
// always holds after phase 2.
//
// All public wait durations are milliseconds of game time and
// quantize UP to whole 50 ms ticks (R-EXEC-5); no sub-tick timing
// exists anywhere in the API. Suspension records are value types in
// a capacity-stable heap and dispatched waiter lists are pooled
// (R-GC-2, #99); the queue serializes via the sched save format v1
// (#97) and feeds the state hash.

// TickMS is the fixed game-time length of one tick (R-SIM-1).
const TickMS = 50

// QuantizeMS converts a game-time duration in milliseconds to whole
// ticks, rounding UP. 0 yields 0, which the scheduler treats as
// "resume next tick" (R-EXEC-5).
func QuantizeMS(ms uint32) uint32 {
	return (ms + TickMS - 1) / TickMS
}

// AfterMS suspends: cont resumes with st after ms milliseconds of
// game time, quantized up to whole ticks. AfterMS(0, …) resumes next
// tick. Timers and script waits share one sleeper queue.
func (w *World) AfterMS(ms uint32, cont sched.ContID, st sched.State) {
	w.Sched.After(QuantizeMS(ms), cont, st)
}

// scriptPhase is tick phase 2: resume everything due this tick.
func (w *World) scriptPhase() {
	w.Sched.Step()
	if w.OnScriptPhase != nil {
		w.OnScriptPhase(w.tick)
	}
}

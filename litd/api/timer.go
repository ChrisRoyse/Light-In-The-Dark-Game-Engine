package litd

// Timers (timers.md; deduplication-policy.md D5 §6 ex. 6; R-EXEC-5).
//
// The WC3 timer zoo — CreateTimer/TimerStart/Pause/Resume/Destroy plus
// the create+start convenience BJs, the bj_lastStartedTimer side
// channel, and the GetExpiredTimer thread-local — collapses onto two
// constructors and a small handle:
//
//	g.After(d, func())        // one-shot       (D2: create+start are one act)
//	g.Every(d, func(Timer))   // periodic       (callback receives its Timer,
//	                                              replacing GetExpiredTimer)
//	t.Pause()/Resume()/Stop() // D1 passthroughs
//	t.Elapsed()/Remaining()/Timeout()  // D5 typed getters, already minimal
//
// All timing is sim-tick based (R-EXEC-5): a time.Duration is quantized
// UP to whole 50 ms ticks at call time, never wall clock (R-SIM-2), so
// After(75*time.Millisecond) fires at the next 100 ms tick boundary.
// Timers are entries in the same deterministic scheduler queue as script
// waits, so same-tick expiries fire in creation-sequence order (the
// scheduler keys records by (wakeTick, seq)) — the classic replay
// divergence is structurally impossible, not merely tested.
//
// Save/load note: a timer's callback is a Go closure, which — like a
// JASS anonymous function — cannot be serialized. The scheduler RECORD
// (wakeTick, slot, generation, epoch) is value-typed and serializes with
// the queue, but the closure table is rebuilt by Go code, so Go-closure
// timers do not survive a sim save/load. Script (Lua) timers, when that
// layer lands, persist through their own registered continuations.

import (
	"time"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim/sched"
)

// contGoTimer is the single scheduler continuation that fires every
// Go-closure timer. It is reserved at a value far above any
// script-assigned ContID (which a Lua host hands out from low numbers),
// so it can never collide.
const contGoTimer sched.ContID = 1 << 30

// timerEntry is one slot in the Game's timer table. All scheduling
// state is value-typed; only fn is a heap reference, set once at
// creation, so a steady-state periodic fire allocates nothing.
type timerEntry struct {
	fn              func(Timer) // callback; nil when the slot is retired
	gen             uint32      // bumped on retire — stale handles/records detectable
	epoch           uint32      // bumped on every (re)arm — invalidates superseded records
	periodTicks     uint32      // configured timeout in ticks (>=1)
	wakeTick        uint32      // tick the current armed record fires (valid when !paused)
	pausedRemaining uint32      // ticks left when paused (valid when paused)
	periodic        bool
	paused          bool
	alive           bool
}

// After schedules f to run once after d of game time, returning a Timer
// for Pause/Resume/Stop and the elapsed/remaining/timeout queries.
// d quantizes up to whole ticks with a one-tick floor; d<=0 fires next
// tick. A nil f or nil game yields the zero-value Timer (no-op).
// JASS: CreateTimer, CreateTimerBJ, StartTimerBJ, TimerStart
func (g *Game) After(d time.Duration, f func()) Timer {
	if g == nil || f == nil {
		return Timer{}
	}
	return g.newTimer(d, false, func(Timer) { f() })
}

// Every schedules f to run repeatedly every d of game time, drift-free
// (each fire re-arms from the schedule, not from callback completion).
// The callback receives its own Timer so it can Stop/Pause itself or
// read its remaining time — the GetExpiredTimer global is gone. A nil f
// or nil game yields the zero-value Timer (no-op).
func (g *Game) Every(d time.Duration, f func(t Timer)) Timer {
	if g == nil || f == nil {
		return Timer{}
	}
	return g.newTimer(d, true, f)
}

// newTimer allocates a slot, arms the first cycle, and returns the handle.
func (g *Game) newTimer(d time.Duration, periodic bool, fn func(Timer)) Timer {
	if !g.timerContReg {
		// Lazy one-time registration of the firing continuation on this
		// game's scheduler. Done here rather than at construction so a
		// game that never uses timers carries no scheduler entry.
		g.w.Sched.Register(contGoTimer, func(_ *sched.Scheduler, st sched.State) {
			g.fireTimer(st)
		})
		g.timerContReg = true
	}
	slot := g.allocTimerSlot()
	e := &g.timers[slot]
	e.fn = fn
	e.periodic = periodic
	e.paused = false
	e.periodTicks = durationToTicks(d)
	e.alive = true
	g.armTimer(slot)
	return Timer{slot: slot, gen: e.gen, g: g}
}

// allocTimerSlot returns a reusable retired slot (whose generation was
// already bumped on retire) or grows the table. Fresh entries start at
// generation 1 so a zero-value Timer{slot:0, gen:0} never matches slot 0.
func (g *Game) allocTimerSlot() uint32 {
	if n := len(g.timerFree); n > 0 {
		slot := g.timerFree[n-1]
		g.timerFree = g.timerFree[:n-1]
		return slot
	}
	g.timers = append(g.timers, timerEntry{gen: 1})
	return uint32(len(g.timers) - 1)
}

// armTimer (re)schedules slot for its next fire, drift-free: the record
// wakes exactly periodTicks ticks from now, and because the scheduler
// fires precisely at wakeTick, re-arming from inside a fire keeps fires
// on exact multiples of the origin. epoch is bumped so any older
// still-pending record for this slot (e.g. one left over across a
// pause) is recognized as stale when it fires.
func (g *Game) armTimer(slot uint32) {
	e := &g.timers[slot]
	e.epoch++
	delay := e.periodTicks
	if delay == 0 {
		delay = 1
	}
	e.wakeTick = g.w.Sched.Now() + delay
	g.w.Sched.After(delay, contGoTimer, sched.State{int64(slot), int64(e.gen), int64(e.epoch)})
}

// fireTimer is the scheduler callback for every Go-closure timer. It
// validates the record against the table (a slot may have been Stopped,
// reused, paused, or re-armed since this record was queued) before
// running the user callback, then handles one-shot retirement and
// drift-free periodic re-arm — re-reading the entry after the callback,
// which may have grown the table or mutated this timer.
func (g *Game) fireTimer(st sched.State) {
	slot := uint32(st[0])
	gen := uint32(st[1])
	epoch := uint32(st[2])
	if int(slot) >= len(g.timers) {
		return
	}
	e := &g.timers[slot]
	if !e.alive || e.gen != gen || e.epoch != epoch {
		return // stopped, reused, or superseded by a re-arm — stale record
	}
	if e.paused {
		return // paused before this record fired; Resume arms a fresh one
	}
	fn := e.fn
	periodic := e.periodic
	fn(Timer{slot: slot, gen: gen, g: g})

	// The callback may have reallocated g.timers (by creating timers) or
	// retired/paused this one; re-resolve and re-check identity.
	if int(slot) >= len(g.timers) {
		return
	}
	e = &g.timers[slot]
	if !e.alive || e.gen != gen {
		return // callback Stopped it (or it was reused)
	}
	if !periodic {
		g.retireTimer(slot)
		return
	}
	if e.paused {
		return // callback paused it; Resume will re-arm
	}
	g.armTimer(slot)
}

// retireTimer frees a slot for reuse and bumps its generation so any
// outstanding handle or queued record for the old identity is invalid.
func (g *Game) retireTimer(slot uint32) {
	e := &g.timers[slot]
	e.alive = false
	e.fn = nil
	e.gen++
	if e.gen == 0 {
		e.gen = 1
	}
	g.timerFree = append(g.timerFree, slot)
}

// entry resolves the handle to its live table entry, or nil when the
// handle is zero-value, out of range, retired, or generation-stale.
func (t Timer) entry() *timerEntry {
	if t.g == nil || int(t.slot) >= len(t.g.timers) {
		return nil
	}
	e := &t.g.timers[t.slot]
	if !e.alive || e.gen != t.gen {
		return nil
	}
	return e
}

// Pause freezes the countdown, preserving the exact remaining time.
// The outstanding scheduler record is left in place; it is ignored if
// it fires while paused, and Resume arms a fresh record for the frozen
// remainder. No-op on an already-paused or invalid timer.
// JASS: PauseTimer
func (t Timer) Pause() {
	if t.sid != 0 {
		t.g.w.Timers.Pause(t.sid, t.g.w.Tick())
		return
	}
	e := t.entry()
	if e == nil {
		t.g.reportInvalid("Timer.Pause")
		return
	}
	if e.paused {
		return
	}
	e.paused = true
	now := t.g.w.Sched.Now()
	if e.wakeTick > now {
		e.pausedRemaining = e.wakeTick - now
	} else {
		e.pausedRemaining = 0
	}
}

// Resume restarts a paused timer from its frozen remaining time. It
// arms a fresh record (bumping epoch), so any record left pending from
// before the pause is recognized as stale when it fires. No-op on a
// running or invalid timer.
// JASS: ResumeTimer
func (t Timer) Resume() {
	if t.sid != 0 {
		t.g.w.Timers.Resume(t.sid, t.g.w.Tick())
		return
	}
	e := t.entry()
	if e == nil {
		t.g.reportInvalid("Timer.Resume")
		return
	}
	if !e.paused {
		return
	}
	e.paused = false
	rem := e.pausedRemaining
	if rem == 0 {
		rem = 1
	}
	e.epoch++
	e.wakeTick = t.g.w.Sched.Now() + rem
	t.g.w.Sched.After(rem, contGoTimer, sched.State{int64(t.slot), int64(e.gen), int64(e.epoch)})
}

// Stop cancels the timer and releases its slot (DestroyTimer). Any
// pending record goes stale via the generation bump. Idempotent: Stop
// on an already-stopped or zero-value timer is a silent no-op, so a
// callback may Stop its own periodic timer safely.
// JASS: DestroyTimer, DestroyTimerBJ
func (t Timer) Stop() {
	if t.sid != 0 {
		t.g.w.Timers.Cancel(t.sid)
		return
	}
	if t.entry() == nil {
		return
	}
	t.g.retireTimer(t.slot)
}

// Timeout returns the timer's configured period (TimerGetTimeout), or 0
// on an invalid timer.
// JASS: TimerGetTimeout
func (t Timer) Timeout() time.Duration {
	if t.sid != 0 {
		if iv, ok := t.g.w.Timers.IntervalOf(t.sid); ok {
			return ticksToDuration(iv)
		}
		return 0
	}
	e := t.entry()
	if e == nil {
		return 0
	}
	return ticksToDuration(e.periodTicks)
}

// Remaining returns the game time until the timer next fires
// (TimerGetRemaining), 0 on an invalid timer. While paused it is the
// frozen remainder.
// JASS: TimerGetRemaining
func (t Timer) Remaining() time.Duration {
	if t.sid != 0 {
		if rem, ok := t.g.w.Timers.RemainingTicks(t.sid, t.g.w.Tick()); ok {
			return ticksToDuration(rem)
		}
		return 0
	}
	e := t.entry()
	if e == nil {
		return 0
	}
	if e.paused {
		return ticksToDuration(e.pausedRemaining)
	}
	now := t.g.w.Sched.Now()
	if e.wakeTick <= now {
		return 0
	}
	return ticksToDuration(e.wakeTick - now)
}

// Elapsed returns the game time since the current cycle started
// (TimerGetElapsed) = Timeout - Remaining, 0 on an invalid timer.
// JASS: TimerGetElapsed
func (t Timer) Elapsed() time.Duration {
	if t.sid != 0 {
		iv, ok := t.g.w.Timers.IntervalOf(t.sid)
		if !ok {
			return 0
		}
		rem, _ := t.g.w.Timers.RemainingTicks(t.sid, t.g.w.Tick())
		if rem > iv {
			rem = iv
		}
		return ticksToDuration(iv - rem)
	}
	e := t.entry()
	if e == nil {
		return 0
	}
	var rem uint32
	if e.paused {
		rem = e.pausedRemaining
	} else if now := t.g.w.Sched.Now(); e.wakeTick > now {
		rem = e.wakeTick - now
	}
	if rem > e.periodTicks {
		rem = e.periodTicks
	}
	return ticksToDuration(e.periodTicks - rem)
}

// durationToTicks quantizes a game-time duration up to whole 50 ms
// ticks with a one-tick floor (R-EXEC-5). d<=0 and sub-tick durations
// fire on the next tick. Durations beyond the uint32 millisecond range
// are clamped rather than wrapped.
func durationToTicks(d time.Duration) uint32 {
	if d <= 0 {
		return 1
	}
	ms := d / time.Millisecond
	const maxMS = int64(^uint32(0))
	if int64(ms) > maxMS {
		ms = time.Duration(maxMS)
	}
	t := sim.QuantizeMS(uint32(ms))
	if t == 0 {
		t = 1
	}
	return t
}

// ticksToDuration converts whole sim ticks back to game-time duration.
func ticksToDuration(ticks uint32) time.Duration {
	return time.Duration(ticks) * sim.TickMS * time.Millisecond
}

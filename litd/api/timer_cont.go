package litd

// Serializable timer API (PRD2 01-timer-wheel/api.md §1.2, #556).
//
// The continuation form is the path gameplay and ability code MUST use:
// a timer names *what to run* by a stable Cont id registered at world
// setup, and carries *the data it needs* in a value-typed Payload. Both
// serialize trivially, so these timers survive save/load — unlike the
// Go-closure After/Every sugar (timer.go), whose closures cannot be
// persisted (the #270 lesson, reclassified in #557).
//
// A Cont value is used directly as the sim scheduler's ContID (the sim
// TimerStore invokes it on fire), so it shares the script-owned ContID
// space: pick Cont values distinct from any RegisterScriptCont ids —
// a collision is caught loudly at registration (sched.Register panics
// on a duplicate), never silently.

import (
	"time"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim/sched"
)

// Cont names a continuation registered at world setup via RegisterCont.
// It is the stable integer a serializable timer stores instead of a Go
// closure (R-TMR-2).
type Cont uint16

// Payload is the value-typed bundle a timer carries to its continuation
// — typically an owner EntityID, an ability/effect ref, a target, and a
// scalar. It serializes as four int64s with the timer.
type Payload struct{ A, B, C, D int64 }

func (p Payload) state() sched.State { return sched.State{p.A, p.B, p.C, p.D} }

// RegisterCont binds a Cont id to fn on this game's scheduler. Call once
// per Cont during world setup, in deterministic order, on BOTH the
// original and any save-loaded world (this is what re-binds a loaded
// timer's Cont — no function pointer is ever serialized). Panics on a
// duplicate id or nil fn (fail-closed: a dangling continuation must not
// sit in the queue past a save).
func (g *Game) RegisterCont(c Cont, fn func(*Game, Payload)) {
	if fn == nil {
		panic("litd: RegisterCont with nil func")
	}
	g.w.Sched.Register(sched.ContID(c), func(_ *sched.Scheduler, st sched.State) {
		fn(g, Payload{st[0], st[1], st[2], st[3]})
	})
}

// AfterCont schedules continuation c to run once after d of game time,
// returning a serializable Timer. d quantizes up to whole ticks with a
// one-tick floor. A nil game yields the zero-value Timer.
func (g *Game) AfterCont(d time.Duration, c Cont, p Payload) Timer {
	return g.createContTimer(d, sim.TimerSingle, 0, c, p, 0)
}

// LoopCont schedules continuation c to run every d of game time until
// cancelled (drift-free: each fire reschedules from the prior wake).
func (g *Game) LoopCont(d time.Duration, c Cont, p Payload) Timer {
	return g.createContTimer(d, sim.TimerLoop, 0, c, p, 0)
}

// CountCont schedules continuation c to run exactly n times, every d of
// game time, then free. n<=0 is clamped to a single fire.
func (g *Game) CountCont(d time.Duration, n int, c Cont, p Payload) Timer {
	rem := uint32(0)
	if n > 0 {
		rem = uint32(n)
	}
	return g.createContTimer(d, sim.TimerCount, rem, c, p, 0)
}

// AfterContOwned is AfterCont bound to owner: the timer auto-cancels if
// the owner unit dies before it fires (R-TMR-6), preventing a teardown
// timer from outliving its caster.
func (g *Game) AfterContOwned(d time.Duration, owner Unit, c Cont, p Payload) Timer {
	return g.createContTimer(d, sim.TimerSingle, 0, c, p, owner.id)
}

// LoopContOwned is LoopCont bound to owner: the loop stops when the
// owner dies (the spawner pattern — a dead camp marker stops respawning).
func (g *Game) LoopContOwned(d time.Duration, owner Unit, c Cont, p Payload) Timer {
	return g.createContTimer(d, sim.TimerLoop, 0, c, p, owner.id)
}

// createContTimer is the shared constructor. Returns the zero Timer on a
// nil game or pool exhaustion (sim Create returns TimerID(0)).
func (g *Game) createContTimer(d time.Duration, mode sim.TimerMode, remaining uint32, c Cont, p Payload, owner sim.EntityID) Timer {
	if g == nil {
		return Timer{}
	}
	id := g.w.Timers.Create(g.w.Tick(), mode, durationToTicks(d), remaining, uint16(c), p.state(), owner)
	if id == 0 {
		return Timer{} // pool exhausted — gameplay-level "timer not created"
	}
	return Timer{g: g, sid: id}
}

// Cancel tears down the timer (serializable or closure), idempotent on a
// stale/zero handle. The spec's name for Stop on the continuation API.
func (t Timer) Cancel() { t.Stop() }

// Paused reports whether the timer is frozen. Always false for a closure
// timer that is not paused, and for a stale handle.
func (t Timer) Paused() bool {
	if t.sid != 0 {
		return t.g != nil && t.g.w.Timers.IsPaused(t.sid)
	}
	if e := t.entry(); e != nil {
		return e.paused
	}
	return false
}

// SetPaused freezes (p=true) or resumes (p=false) the timer, preserving
// the remaining time. No-op on a stale handle or a redundant transition.
func (t Timer) SetPaused(p bool) {
	if p {
		t.Pause()
	} else {
		t.Resume()
	}
}

// FiresRemaining returns how many fires are left: the exact count for a
// CountCont, -1 for an unbounded loop, 1 for a pending single, and 0 for
// a stale/done handle. (Named to avoid colliding with the time-valued
// Remaining; the spec's Remaining()->int maps here.)
func (t Timer) FiresRemaining() int {
	if t.sid != 0 {
		if n, ok := t.g.w.Timers.FiresLeft(t.sid); ok {
			return n
		}
		return 0
	}
	// Closure timers: periodic loops are unbounded, one-shots fire once.
	if e := t.entry(); e != nil {
		if e.periodic {
			return -1
		}
		return 1
	}
	return 0
}

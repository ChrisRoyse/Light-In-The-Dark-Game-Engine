package ai

// Package ai is the second scheduler domain (#272; execution-model.md §6
// R-EXEC-3; tick-and-scheduler.md §3.4; milestones.md §9; decision
// D-2026-06-11-6). It hosts AI scripts in their own isolated cooperative
// scheduler contexts — one deterministic, serializable scheduler instance per
// AI player, drained in a dedicated sub-phase of tick phase 2.
//
// The WC3 lesson the domain reproduces structurally: AI is a *foreign domain*
// with a message-passing boundary, not privileged script. No shared globals
// with the map-script domain; the only capabilities reachable from an
// AIController are AIView (read) and AICommander (typed commands onto the same
// ordered command stream player input uses). Because AI acts only by enqueuing
// commands into the deterministic stream, it cannot desync a match, and because
// it has no hooks into map-script state it can be disabled or replaced wholesale.
//
// This file is the isolation boundary itself: the capability interfaces and the
// Context that binds a player's scheduler to exactly those capabilities and
// nothing else. domain.go hosts the per-player schedulers and the tick phase.

import "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim/sched"

// AIView is the read-only window an AI context has onto the simulation — the
// same filtered, read-only discipline as the §4 query filters. No method on it
// mutates sim state; it is the ONLY read path across the domain boundary.
//
// The query surface here is deliberately small: it is the contract, not the
// full port. The complete commonai read family (the GetUnitCount /
// GetPlayerState / GetEnemyPower natives) extends this interface at M5.5
// (#274+); the domain and its isolation guarantee do not change when it does.
type AIView interface {
	// Now is the current sim tick — the read-only clock the AI shares with the
	// rest of the sim (its own scheduler tick advances in lockstep with it).
	Now() uint32
	// Self is the player id this AI controls.
	Self() int
	// UnitCount reports how many live units of unitTypeID the given player owns.
	// Representative read query; never mutates.
	UnitCount(player, unitTypeID int) int
}

// CommandKind tags a typed AI command. The integer-pair operands (A, B) carry
// the command-specific payload, exactly as WC3's integer-pair command stack did
// — but typed, so the boundary is checkable rather than a bag of ints. The
// concrete command set and the ordered stream live in commands.go (#273).
type CommandKind uint16

// AICommand is one typed command an AI enqueues onto the ordered command stream
// — the typed-Go descendant of WC3's integer-pair command stack (R-EXEC-3). It
// is a fixed-size value with no pointers: it copies by value, never escapes, and
// would serialize directly if a command ever outlived its tick (it does not —
// commands are drained the same tick they are issued, in phase 1's contract).
type AICommand struct {
	Kind   CommandKind
	Player int32
	A, B   int32
}

// AICommander is the ONLY write path out of an AI context. It enqueues typed
// commands onto the same ordered stream player input and replays use; it cannot
// touch sim state directly. The message-passing boundary (R-EXEC-3) is exactly
// this: read through AIView, act through AICommander, nothing else.
type AICommander interface {
	// Issue enqueues a command. The sim drains the stream in phase 1 of the
	// next tick (or this tick, per the phase contract), validating each command
	// before it writes any order — the AI never bypasses that validation.
	Issue(cmd AICommand)
}

// Context is a single AI player's whole world. It is the only type an
// AIController ever touches, and it is the structural enforcement of R-EXEC-3:
// it exposes the player's own cooperative scheduler (to suspend/resume decision
// loops), its AIView (read), and its AICommander (commands) — and it holds no
// reference to the sim World, to the map-script domain, or to any other
// player's context. There is no field or method through which AI script code
// can reach map-script state; isolation is a property of the type, not a
// convention the script is trusted to honour.
type Context struct {
	player int
	s      *sched.Scheduler
	view   AIView
	cmd    AICommander

	// resumes counts continuation resumptions since the last reset — the
	// domain's counted (never timed) tick-slice meter. Incremented by the
	// wrapper Register installs around every continuation, so it measures
	// actual AI work done, deterministically, on every machine.
	resumes int

	// disabled freezes the context: the domain skips it in Tick but its
	// suspensions stay intact and still serialize. Set via Domain.Disable.
	disabled bool
}

// Player returns the player id this context controls.
func (c *Context) Player() int { return c.player }

// View returns the read-only simulation view.
func (c *Context) View() AIView { return c.view }

// Commander returns the command-issuing capability.
func (c *Context) Commander() AICommander { return c.cmd }

// Now returns the AI scheduler's current tick.
func (c *Context) Now() uint32 { return c.s.Now() }

// Register binds a continuation id to fn on this context's scheduler. The
// registered function is wrapped so each resumption advances the domain's
// counted tick-slice meter; the wrapper is allocated once here, never per tick,
// so steady-state ticking allocates nothing. fn is otherwise the script's own
// continuation — it suspends again via After/WaitEvent or completes.
func (c *Context) Register(id sched.ContID, fn sched.Func) {
	c.s.Register(id, func(s *sched.Scheduler, st sched.State) {
		c.resumes++
		fn(s, st)
	})
}

// After suspends id to resume after delayTicks ticks (delay 0 ⇒ next tick).
func (c *Context) After(delayTicks uint32, id sched.ContID, st sched.State) {
	c.s.After(delayTicks, id, st)
}

// WaitEvent suspends id until ev next fires on this context's scheduler.
func (c *Context) WaitEvent(ev sched.EventID, id sched.ContID, st sched.State) {
	c.s.WaitEvent(ev, id, st)
}

// FireEvent resumes this context's waiters parked on ev. Events are intra-AI
// only — there is no cross-context event channel, by isolation design.
func (c *Context) FireEvent(ev sched.EventID) { c.s.FireEvent(ev) }

// PendingSleepers reports how many suspensions sit in this context's scheduler.
func (c *Context) PendingSleepers() int { return c.s.PendingSleepers() }

// PendingWaiters reports how many of this context's continuations wait on ev.
func (c *Context) PendingWaiters(ev sched.EventID) int { return c.s.PendingWaiters(ev) }

// AIController is a player's AI logic. The domain calls Install once, handing it
// the player's Context; the controller registers its decision-loop
// continuations and kicks off the first suspension. Thereafter the domain ticks
// it. The controller receives a *Context and nothing else — by construction it
// cannot reach the sim World or another player's state (R-EXEC-3, enforced by
// the type system, not by convention).
type AIController interface {
	Install(ctx *Context)
}

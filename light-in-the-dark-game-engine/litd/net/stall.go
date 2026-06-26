package net

// stall.go: host-side stall handling (#71, D-2026-06-11-26). The lockstep gate
// (lockstep.go) blocks the sim at the boundary of the first turn whose aggregate
// has not arrived — Pump reports (waitingTurn, blocked). A block is normal for a
// frame or two; a SUSTAINED block means a peer has stalled. This is the policy
// that turns "blocked on turn T" into the match-pause / grace-countdown / drop /
// resume decisions the host broadcasts.
//
// Like DelayController, this is a pure clock-injected state machine: it takes the
// monotonic wall-clock as an explicit duration and emits a decision, so it holds
// no IO and is deterministic under synthetic time. The host loop feeds it the
// gate's block state plus the set of players still missing for the blocked turn;
// it acts on the returned decision (raise the waiting UI, remove the laggards,
// re-aggregate over survivors, broadcast resume at the same turn boundary so
// every client resumes in lockstep). litd/sim stays untouched: a dropped
// player's units simply stop receiving orders (sim rules unchanged).

import (
	"fmt"
	"time"
)

// DefaultGrace is the stall grace period before a non-responsive player is
// dropped (D-2026-06-11-26). Session-configurable via NewStallController.
const DefaultGrace = 30 * time.Second

// StallPhase is the host's stall state for the turn the gate is blocked on.
type StallPhase uint8

const (
	// PhaseRunning: the sim is advancing (the gate is not blocked).
	PhaseRunning StallPhase = iota
	// PhaseWaiting: blocked within the grace window — the match is paused and the
	// waiting UI (naming the laggards) is shown while the countdown runs.
	PhaseWaiting
)

// String renders the phase for logs / FSV.
func (p StallPhase) String() string {
	if p == PhaseWaiting {
		return "waiting"
	}
	return "running"
}

// StallDecision is what the host should do after one Observe. At most one of
// Began / Resumed is true per call; Dropped is non-empty only on the grace-expiry
// observation (which also sets Resumed, since the drop unblocks the turn).
type StallDecision struct {
	Phase      StallPhase    // the phase AFTER this observation
	Began      bool          // this call entered Waiting — raise the waiting UI
	Resumed    bool          // this call left Waiting — lower the UI, resume the sim
	Dropped    []uint8       // players to drop now (grace expired); discard their pending turns
	ResumeTurn uint64        // the turn boundary all clients resume at (the blocked turn)
	Remaining  time.Duration // grace left while Waiting (drives the countdown); 0 otherwise
}

// StallController applies the grace policy to the gate's block state. Not safe
// for concurrent use; drive it from the host loop alongside the gate. The zero
// value is not usable — construct via NewStallController.
type StallController struct {
	grace   time.Duration
	phase   StallPhase
	waiting uint64        // turn currently blocked on (valid while phase==PhaseWaiting)
	since   time.Duration // monotonic time the current Waiting began
}

// NewStallController builds a controller with the given grace period (must be
// positive — a zero/negative grace would drop on the first blocked frame, which
// is never the intent; reject it fail-closed).
func NewStallController(grace time.Duration) (*StallController, error) {
	if grace <= 0 {
		return nil, fmt.Errorf("net: stall grace must be positive, got %v", grace)
	}
	return &StallController{grace: grace, phase: PhaseRunning}, nil
}

// Observe feeds one gate poll: whether the gate is blocked, the turn it is blocked
// on, the players still missing a submission for that turn (the laggards), and the
// current monotonic time. It returns the host's decision.
//
//   - Not blocked: if we were Waiting, the awaited turn has arrived → Resume at
//     that turn (a peer recovered inside grace, or a prior drop unblocked it).
//     Otherwise nothing (still Running).
//   - Newly blocked, or blocked on a different turn than before: enter Waiting,
//     (re)start the grace clock — Began, full grace Remaining.
//   - Still blocked on the same turn, within grace: keep Waiting, report the
//     shrinking Remaining for the countdown.
//   - Still blocked, grace elapsed, at least one laggard: Drop the laggards and
//     Resume at the blocked turn (the host removes them and re-aggregates over the
//     survivors). Fail-closed: grace elapsed but NO laggard named → stay Waiting,
//     never emit a no-op drop/resume (a missing==∅ block is a caller-contract
//     violation, not a reason to advance into unaggregated ticks).
func (c *StallController) Observe(blocked bool, waitingTurn uint64, missing []uint8, now time.Duration) StallDecision {
	if !blocked {
		if c.phase == PhaseWaiting {
			resumed := c.waiting
			c.phase = PhaseRunning
			return StallDecision{Phase: PhaseRunning, Resumed: true, ResumeTurn: resumed}
		}
		return StallDecision{Phase: PhaseRunning}
	}

	// Blocked. A fresh block (or a block that moved to a later turn) restarts the
	// grace clock — each stalled turn gets its own full grace window.
	if c.phase != PhaseWaiting || c.waiting != waitingTurn {
		c.phase = PhaseWaiting
		c.waiting = waitingTurn
		c.since = now
		return StallDecision{Phase: PhaseWaiting, Began: true, Remaining: c.grace}
	}

	elapsed := now - c.since
	if elapsed < c.grace {
		return StallDecision{Phase: PhaseWaiting, Remaining: c.grace - elapsed}
	}

	// Grace elapsed. Drop the laggards and resume — unless none is named (defensive:
	// never advance past an unaggregated turn on an empty drop).
	if len(missing) == 0 {
		return StallDecision{Phase: PhaseWaiting, Remaining: 0}
	}
	dropped := append([]uint8(nil), missing...)
	c.phase = PhaseRunning
	return StallDecision{Phase: PhaseRunning, Resumed: true, Dropped: dropped, ResumeTurn: waitingTurn}
}

// Phase reports the current stall phase (for the host's status surface).
func (c *StallController) Phase() StallPhase { return c.phase }

// WaitingTurn reports the turn currently stalled on; valid only while Phase is
// PhaseWaiting.
func (c *StallController) WaitingTurn() uint64 { return c.waiting }

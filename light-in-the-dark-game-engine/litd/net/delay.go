package net

// delay.go: the adaptive input-delay buffer (#66, D-2026-06-11-26). Local
// commands issued during turn T are scheduled into turn T+delay, giving the turn
// T+delay aggregate time to round-trip to every peer before that turn executes.
// delay starts at 2 turns and adapts to measured RTT.
//
// THE DETERMINISM RULE (load-bearing): delay is NEVER inferred locally from a
// peer's own RTT — that would diverge peer to peer and desync lockstep. Instead
// the HOST announces every change as a (effectiveTurn, delay) record carried in
// the turn stream, and each peer applies the identical announcement stream to a
// DelaySchedule. So DelayAt(turn) is a pure function of the announcements, equal
// on every machine. The host's RTT measurement only DECIDES when to announce;
// the schedule itself is shared data, not a local guess.

import (
	"fmt"
	"time"
)

const (
	// DefaultDelay is the initial input delay in turns (D-2026-06-11-26).
	DefaultDelay = 2
	// MinDelay / MaxDelay clamp the delay (turns).
	MinDelay = 1
	MaxDelay = 8
)

// delayChange is one piecewise-constant segment: from effectiveTurn onward
// (until the next change) the delay is `delay`.
type delayChange struct {
	effectiveTurn uint64
	delay         int
}

// DelaySchedule is the shared, deterministic turn→delay function. Every peer
// holds one and applies the same host announcements, so all agree on the delay
// (and thus the local-command schedule) at every turn. Not safe for concurrent
// use; drive from the turn loop.
type DelaySchedule struct {
	changes []delayChange // sorted ascending by effectiveTurn; [0] = {0, DefaultDelay}
}

// NewDelaySchedule starts at DefaultDelay from turn 0.
func NewDelaySchedule() *DelaySchedule {
	return &DelaySchedule{changes: []delayChange{{effectiveTurn: 0, delay: DefaultDelay}}}
}

// Announce records a host-issued delay change taking effect at effectiveTurn.
// It is fail-closed: delay must be in [MinDelay,MaxDelay] and effectiveTurn must
// be strictly greater than the last announced change (monotonic — a change can
// only be scheduled into the future, never rewriting the past). Every peer calls
// this with the identical announcement, preserving a common schedule.
func (s *DelaySchedule) Announce(effectiveTurn uint64, delay int) error {
	if delay < MinDelay || delay > MaxDelay {
		return fmt.Errorf("net: delay %d out of [%d,%d]", delay, MinDelay, MaxDelay)
	}
	last := s.changes[len(s.changes)-1]
	if effectiveTurn <= last.effectiveTurn {
		return fmt.Errorf("net: delay change effectiveTurn %d not after last %d (monotonic)", effectiveTurn, last.effectiveTurn)
	}
	if delay == last.delay {
		return fmt.Errorf("net: delay change to %d is a no-op (already %d)", delay, last.delay)
	}
	s.changes = append(s.changes, delayChange{effectiveTurn, delay})
	return nil
}

// DelayAt returns the effective delay at turn — the most recent change whose
// effectiveTurn ≤ turn.
func (s *DelaySchedule) DelayAt(turn uint64) int {
	d := s.changes[0].delay
	for _, c := range s.changes {
		if c.effectiveTurn <= turn {
			d = c.delay
		} else {
			break
		}
	}
	return d
}

// ScheduleFor returns the turn a command issued during currentTurn executes on:
// currentTurn + DelayAt(currentTurn). Computed once at enqueue; a later
// announcement (effectiveTurn in the future) never reassigns it.
func (s *DelaySchedule) ScheduleFor(currentTurn uint64) uint64 {
	return currentTurn + uint64(s.DelayAt(currentTurn))
}

// Current is the delay of the latest segment (the value future commands use
// absent a further change).
func (s *DelaySchedule) Current() int { return s.changes[len(s.changes)-1].delay }

// DelayController is the HOST-only policy that decides when to announce a delay
// change, from per-round RTT. The schedule it mutates is shared; this object is
// not. Hysteresis (sustained streaks + a deadband) avoids flapping on jitter.
type DelayController struct {
	turnPeriod time.Duration
	raiseAfter int // consecutive high-RTT rounds before raising
	lowerAfter int // consecutive low-RTT rounds before lowering
	schedule   *DelaySchedule
	highStreak int
	lowStreak  int
}

// NewDelayController builds a host controller. turnPeriod is the wall-clock
// duration of one turn (turnLenTicks × tick period). raiseAfter/lowerAfter are
// the sustained-round thresholds (≥1). The controller drives `schedule`.
func NewDelayController(turnPeriod time.Duration, raiseAfter, lowerAfter int, schedule *DelaySchedule) (*DelayController, error) {
	if turnPeriod <= 0 {
		return nil, fmt.Errorf("net: turnPeriod must be positive")
	}
	if raiseAfter < 1 || lowerAfter < 1 {
		return nil, fmt.Errorf("net: raiseAfter/lowerAfter must be ≥1")
	}
	if schedule == nil {
		return nil, fmt.Errorf("net: nil schedule")
	}
	return &DelayController{turnPeriod: turnPeriod, raiseAfter: raiseAfter, lowerAfter: lowerAfter, schedule: schedule}, nil
}

// Observe feeds the worst (max) per-client RTT measured during currentTurn. If
// sustained high RTT (> turnPeriod) crosses raiseAfter, it raises delay; if
// sustained headroom (< turnPeriod/2) crosses lowerAfter, it lowers. On a change
// it announces at a safe future boundary (currentTurn + current delay + 1, so
// every peer receives the announcement before scheduling for that turn), applies
// it to the shared schedule, and returns (effectiveTurn, newDelay, true). The
// caller broadcasts that announcement to all clients verbatim.
func (c *DelayController) Observe(maxRTT time.Duration, currentTurn uint64) (effectiveTurn uint64, newDelay int, changed bool) {
	switch {
	case maxRTT > c.turnPeriod:
		c.highStreak++
		c.lowStreak = 0
	case maxRTT < c.turnPeriod/2:
		c.lowStreak++
		c.highStreak = 0
	default:
		// deadband: neither pressure nor headroom — decay both streaks
		c.highStreak = 0
		c.lowStreak = 0
	}

	cur := c.schedule.Current()
	eff := currentTurn + uint64(cur) + 1

	if c.highStreak >= c.raiseAfter && cur < MaxDelay {
		c.highStreak = 0
		if err := c.schedule.Announce(eff, cur+1); err == nil {
			return eff, cur + 1, true
		}
	}
	if c.lowStreak >= c.lowerAfter && cur > MinDelay {
		c.lowStreak = 0
		if err := c.schedule.Announce(eff, cur-1); err == nil {
			return eff, cur - 1, true
		}
	}
	return 0, 0, false
}

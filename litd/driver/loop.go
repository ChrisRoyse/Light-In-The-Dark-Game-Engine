// Package driver owns the wall-clock side of the engine: the 20 Hz
// fixed-timestep accumulator (tick-and-scheduler.md §1) plus game
// speed and pause (§1.1). Wall-clock readings live HERE and only here
// — litd/sim never sees real time, a float delta, or the speed
// setting; a replay recorded at fast speed replays identically at
// slow because tick content is speed-independent.
package driver

import "time"

const (
	// TickDuration is exactly 50 ms of game time per tick (R-SIM-1).
	TickDuration = 50 * time.Millisecond
	// MaxFrame clamps a stalled frame so a long hitch produces
	// game-time slowdown, not an unbounded catch-up burst (the
	// spiral-of-death guard, §1 default 250 ms).
	MaxFrame = 250 * time.Millisecond
)

// Stepper is the sim seam: exactly one deterministic tick per call.
type Stepper interface {
	Step()
}

// Loop is the fixed-timestep accumulator. The caller feeds it real
// frame durations (the wall-clock reading happens at the call site —
// this is the injection seam, which is why tests drive it with exact
// synthetic durations rather than mocks).
type Loop struct {
	sim         Stepper
	accumulator time.Duration
	speed       float64
	paused      bool
	totalSteps  uint64
}

// New returns a loop at normal speed, unpaused.
func New(sim Stepper) *Loop {
	if sim == nil {
		panic("driver: nil Stepper")
	}
	return &Loop{sim: sim, speed: 1.0}
}

// Frame advances the loop by one real frame: clamp, scale by speed,
// accumulate, step while a full tick is banked. Returns the number of
// Steps taken and the interpolation alpha in [0,1) — alpha is
// RENDER-side only (§2); nothing in the sim ever sees it.
//
// A negative duration (clock anomaly) contributes nothing — the
// accumulator only moves forward.
func (l *Loop) Frame(realFrameTime time.Duration) (steps int, alpha float64) {
	if !l.paused {
		dt := realFrameTime
		if dt < 0 {
			dt = 0
		}
		if dt > MaxFrame {
			dt = MaxFrame
		}
		// speed scales how much real time maps to one tick (§1.1):
		// fast = 1.25 game-seconds per real second.
		l.accumulator += time.Duration(float64(dt) * l.speed)
		for l.accumulator >= TickDuration {
			l.sim.Step()
			l.accumulator -= TickDuration
			steps++
			l.totalSteps++
		}
	}
	return steps, l.Alpha()
}

// Alpha returns accumulator/tick in [0,1). While paused it stays
// frozen — render keeps drawing the same interpolated pose.
func (l *Loop) Alpha() float64 { return float64(l.accumulator) / float64(TickDuration) }

// SetSpeed sets the real-time→game-time scale (1.0 normal, WC3 fast
// 1.25, slow 0.8). Non-positive or non-finite speeds are refused.
func (l *Loop) SetSpeed(speed float64) bool {
	if !(speed > 0) || speed > 100 { // also rejects NaN
		return false
	}
	l.speed = speed
	return true
}

// Speed returns the current scale.
func (l *Loop) Speed() float64 { return l.speed }

// SetPaused stops/starts feeding the accumulator. Pause is a driver
// concept: the sim does not step and never learns it was paused —
// real time elapsed during the pause is simply never banked, so
// resume produces no catch-up burst.
func (l *Loop) SetPaused(p bool) { l.paused = p }

// Paused reports the pause state.
func (l *Loop) Paused() bool { return l.paused }

// TotalSteps returns the lifetime tick count driven by this loop.
func (l *Loop) TotalSteps() uint64 { return l.totalSteps }

package driver

import (
	"testing"
	"time"
)

type countingSim struct{ steps int }

func (c *countingSim) Step() { c.steps++ }

// Edge 1: a 1,000 ms stall clamps to 250 ms → exactly 5 Steps, never
// 20 — the spiral-of-death guard.
func TestLoopStallClamps(t *testing.T) {
	sim := &countingSim{}
	l := New(sim)
	t.Logf("before stall: accumulator=%v", time.Duration(l.Alpha()*float64(TickDuration)))
	steps, alpha := l.Frame(1000 * time.Millisecond)
	t.Logf("frame(1000ms): clamped to %v -> steps=%d (want 5, NOT 20), alpha=%.3f, accumulator after=%v",
		MaxFrame, steps, alpha, time.Duration(alpha*float64(TickDuration)))
	if steps != 5 || sim.steps != 5 {
		t.Fatalf("stall must clamp to 5 steps, got %d", steps)
	}
	if alpha != 0 {
		t.Fatalf("250ms is a whole number of ticks; alpha must be 0, got %v", alpha)
	}
}

// Edge 2: 16.6 ms frames (60 FPS) — roughly 3 frames per Step, alpha
// rises monotonically within each tick window and wraps at the step.
func TestLoopSixtyFPSAlphaMonotonic(t *testing.T) {
	sim := &countingSim{}
	l := New(sim)
	frame := 16600 * time.Microsecond
	prevAlpha := -1.0
	for i := 0; i < 12; i++ {
		steps, alpha := l.Frame(frame)
		t.Logf("frame %2d: steps=%d alpha=%.4f", i, steps, alpha)
		if steps == 0 && alpha <= prevAlpha {
			t.Fatalf("alpha must rise between steps: %v after %v", alpha, prevAlpha)
		}
		if steps > 0 && alpha >= prevAlpha {
			t.Fatalf("alpha must wrap after a step: %v after %v", alpha, prevAlpha)
		}
		if alpha < 0 || alpha >= 1 {
			t.Fatalf("alpha out of [0,1): %v", alpha)
		}
		prevAlpha = alpha
	}
	// 12 × 16.6 = 199.2ms → 3 full ticks banked
	if sim.steps != 3 {
		t.Fatalf("199.2ms must produce 3 steps, got %d", sim.steps)
	}
}

// Edge 3: pause for 10 s of real frames → zero Steps and frozen
// alpha; resume produces NO catch-up burst.
func TestLoopPauseNoCatchUp(t *testing.T) {
	sim := &countingSim{}
	l := New(sim)
	l.Frame(30 * time.Millisecond) // bank a partial tick
	frozen := l.Alpha()

	l.SetPaused(true)
	pausedSteps := 0
	for i := 0; i < 600; i++ { // 10 s of 60 FPS frames while paused
		s, a := l.Frame(16600 * time.Microsecond)
		pausedSteps += s
		if a != frozen {
			t.Fatalf("alpha must stay frozen during pause: %v != %v", a, frozen)
		}
	}
	l.SetPaused(false)
	resumeSteps, _ := l.Frame(16600 * time.Microsecond)
	t.Logf("paused 10s (600 frames): steps during pause=%d, alpha frozen at %.3f; first resume frame steps=%d (no burst)",
		pausedSteps, frozen, resumeSteps)
	if pausedSteps != 0 {
		t.Fatalf("paused loop stepped %d times", pausedSteps)
	}
	if resumeSteps > 1 {
		t.Fatalf("resume burst: %d steps in one frame", resumeSteps)
	}
}

// Edge 4: fast speed (1.25×) — 1 s of real time = exactly 25 ticks.
func TestLoopFastSpeed(t *testing.T) {
	sim := &countingSim{}
	l := New(sim)
	if !l.SetSpeed(1.25) {
		t.Fatal("SetSpeed(1.25) refused")
	}
	for i := 0; i < 100; i++ { // 100 × 10ms = exactly 1s real
		l.Frame(10 * time.Millisecond)
	}
	t.Logf("1s real at speed 1.25 -> %d ticks (want 25); at 1.0 it would be 20", sim.steps)
	if sim.steps != 25 {
		t.Fatalf("fast speed must give 25 ticks/s, got %d", sim.steps)
	}
}

// Fail-closed inputs: negative frame contributes nothing; bad speeds
// refused; nil sim panics.
func TestLoopFailClosed(t *testing.T) {
	sim := &countingSim{}
	l := New(sim)
	steps, alpha := l.Frame(-5 * time.Second)
	if steps != 0 || alpha != 0 {
		t.Fatalf("negative frame must contribute nothing: steps=%d alpha=%v", steps, alpha)
	}
	for _, bad := range []float64{0, -1, 101} {
		if l.SetSpeed(bad) {
			t.Fatalf("SetSpeed(%v) must be refused", bad)
		}
	}
	if l.Speed() != 1.0 {
		t.Fatalf("refused speeds must not stick: %v", l.Speed())
	}
	defer func() {
		if recover() == nil {
			t.Fatal("New(nil) must panic")
		}
	}()
	New(nil)
}

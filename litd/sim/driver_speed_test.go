package sim

// #307 driver game-speed + pause FSV — proven against the REAL sim and
// its state hash, not a counting mock. The driver (litd/driver) scales
// wall-clock→tick accumulation only; tick CONTENT is speed-independent,
// so the same command stream driven at any speed, or interrupted by any
// pause, lands on a byte-identical state hash after the same tick count.
//
// SoT: World.HashState(...).Top after K ticks + the driver's TotalSteps
// counter. litd/sim imports litd/driver here (test-only); driver does
// not import sim (it steps through a Stepper interface), so no cycle.

import (
	"testing"
	"time"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/driver"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/statehash"
)

// speedScenario builds a deterministic world whose state genuinely
// evolves every tick (two crossing movers + an auto-acquiring fight),
// so the hash comparison is not vacuous.
func speedScenario(t *testing.T) *World {
	t.Helper()
	w := NewWorld(Caps{})
	if err := w.BindDamageMatrix(atkMatrix); err != nil {
		t.Fatal(err)
	}
	w.SetAcquireInterval(1)
	m1 := atkUnit(t, w, 0, xy(500, 500), 6*fixed.One)
	w.IssueOrder(m1, Order{Kind: OrderMove, Point: xy(2000, 500)}, false)
	m2 := atkUnit(t, w, 1, xy(2000, 800), 6*fixed.One)
	w.IssueOrder(m2, Order{Kind: OrderMove, Point: xy(500, 800)}, false)
	a := atkUnit(t, w, 0, xy(1000, 1000), 0)
	arm(t, w, a, 0, 0)
	w.Combats.AcquisitionRange[w.Combats.Row(a)] = 600 * fixed.One
	atkUnit(t, w, 1, xy(1050, 1000), 0) // victim inside weapon range
	return w
}

func hashTop(w *World) uint64 {
	var s statehash.Snapshot
	w.HashState(NewHashRegistry(), &s)
	return s.Top
}

// driveTo feeds realDt-sized frames until the driver has banked exactly
// `target` ticks. realDt is chosen so a single frame banks at most one
// tick (dt·speed < TickDuration), so the counter never overshoots.
func driveTo(l *driver.Loop, target uint64, realDt time.Duration) {
	for l.TotalSteps() < target {
		l.Frame(realDt)
	}
}

// FSV (1): identical command stream at 0.8× / 1.0× / 1.25× → identical
// final state hash after the same tick count. Speed must not leak into
// tick content.
func TestDriverSpeedIndependentReplay(t *testing.T) {
	const K = 60
	run := func(speed float64, realDt time.Duration) uint64 {
		w := speedScenario(t)
		l := driver.New(w)
		if !l.SetSpeed(speed) {
			t.Fatalf("SetSpeed(%v) refused", speed)
		}
		driveTo(l, K, realDt)
		if l.TotalSteps() != K {
			t.Fatalf("speed %v overshot: TotalSteps=%d", speed, l.TotalSteps())
		}
		return hashTop(w)
	}
	// realDt·speed < 50ms for each so at most one tick per frame.
	h08 := run(0.8, 10*time.Millisecond)   // 8ms banked/frame
	h10 := run(1.0, 10*time.Millisecond)   // 10ms
	h125 := run(1.25, 10*time.Millisecond) // 12.5ms
	base := hashTop(speedScenario(t))      // initial (0 ticks) — must differ
	t.Logf("0.8x=%016x  1.0x=%016x  1.25x=%016x  (initial=%016x)", h08, h10, h125, base)
	if h08 != h10 || h10 != h125 {
		t.Fatalf("speed leaked into tick content: 0.8=%016x 1.0=%016x 1.25=%016x", h08, h10, h125)
	}
	if h10 == base {
		t.Fatal("scenario is static — the hash never moved, test is vacuous")
	}
}

// FSV (2): a 10 s pause mid-run, then resume, lands on the SAME final
// hash as an uninterrupted run of the same tick count.
func TestDriverPauseResumeHashEqualsUnpaused(t *testing.T) {
	const K = 60
	// uninterrupted reference
	wRef := speedScenario(t)
	lRef := driver.New(wRef)
	driveTo(lRef, K, 10*time.Millisecond)
	ref := hashTop(wRef)

	// paused run: half, then 10s of paused frames, then the rest
	wP := speedScenario(t)
	lP := driver.New(wP)
	driveTo(lP, K/2, 10*time.Millisecond)
	midHash := hashTop(wP)
	lP.SetPaused(true)
	for i := 0; i < 600; i++ { // 10 s of 60 FPS frames while paused
		if s, _ := lP.Frame(16600 * time.Microsecond); s != 0 {
			t.Fatalf("paused driver stepped: %d", s)
		}
	}
	if hashTop(wP) != midHash {
		t.Fatal("sim state changed during pause")
	}
	if lP.TotalSteps() != K/2 {
		t.Fatalf("pause banked ticks: TotalSteps=%d", lP.TotalSteps())
	}
	lP.SetPaused(false)
	driveTo(lP, K, 10*time.Millisecond)
	got := hashTop(wP)
	t.Logf("unpaused=%016x  paused-then-resumed=%016x  (mid@%d=%016x)", ref, got, K/2, midHash)
	if got != ref {
		t.Fatalf("pause perturbed the result: %016x != %016x", got, ref)
	}
}

// FSV (3): a speed change mid-match never skips or duplicates a tick —
// the lifetime counter advances by exactly the steps each frame reports
// and is strictly monotonic across the switch.
func TestDriverSpeedChangeMonotonicCounter(t *testing.T) {
	w := speedScenario(t)
	l := driver.New(w)
	realDt := 10 * time.Millisecond
	prev := uint64(0)
	switched := false
	trace := make([]uint64, 0, 8)
	for i := 0; i < 400 && l.TotalSteps() < 80; i++ {
		if !switched && l.TotalSteps() >= 30 {
			if !l.SetSpeed(1.25) { // switch slow→fast mid-match
				t.Fatal("mid-match SetSpeed refused")
			}
			switched = true
		}
		steps, _ := l.Frame(realDt)
		cur := l.TotalSteps()
		if cur != prev+uint64(steps) {
			t.Fatalf("counter desync: prev=%d +steps=%d != cur=%d", prev, steps, cur)
		}
		if cur < prev {
			t.Fatalf("counter went backwards: %d -> %d", prev, cur)
		}
		if switched && len(trace) < 6 {
			trace = append(trace, cur)
		}
		prev = cur
	}
	t.Logf("counter samples around/after the 1.0→1.25 switch: %v (final=%d)", trace, l.TotalSteps())
	if !switched {
		t.Fatal("never reached the speed-switch threshold")
	}
	if l.TotalSteps() < 80 {
		t.Fatalf("did not advance past 80 ticks: %d", l.TotalSteps())
	}
}

// The driver's speed/pause are NOT part of the determinism surface: two
// worlds with identical ticks but driven at different speeds/pause hash
// identically because HashState never reads driver state. (Covered by
// (1)/(2); this asserts the property directly on the hash of one world
// driven two ways.)
func TestDriverStateExcludedFromHash(t *testing.T) {
	const K = 40
	a := speedScenario(t)
	la := driver.New(a)
	la.SetSpeed(0.8)
	driveTo(la, K, 12*time.Millisecond)

	b := speedScenario(t)
	lb := driver.New(b)
	lb.SetSpeed(1.25)
	lb.SetPaused(true)
	lb.Frame(time.Second) // paused: no effect
	lb.SetPaused(false)
	driveTo(lb, K, 7*time.Millisecond)

	t.Logf("a(0.8x)=%016x  b(1.25x,paused-then-run)=%016x", hashTop(a), hashTop(b))
	if hashTop(a) != hashTop(b) {
		t.Fatal("driver speed/pause leaked into the state hash")
	}
}

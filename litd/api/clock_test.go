package litd

import (
	"fmt"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
)

func clockLine(g *Game) string {
	if g == nil || g.w == nil {
		return "nil-game"
	}
	return fmt.Sprintf("tick=%d apiTOD=%.4f rawTOD=%.4f apiScale=%.4f rawScale=%.4f suspended=%v elapsed=%.4f",
		g.w.Tick(),
		g.TimeOfDay(), toFloat(g.w.TimeOfDay()),
		g.TimeOfDayScale(), toFloat(g.w.TimeOfDayScale()),
		g.w.TimeOfDaySuspended(), g.ElapsedTime())
}

func stepN(w *sim.World, n int) {
	for i := 0; i < n; i++ {
		w.Step()
	}
}

func TestClockScaleDoesNotChangeTickRate(t *testing.T) {
	w := sim.NewWorld(sim.Caps{})
	g := newGame(w)
	if TimeDawn != 6.0 || TimeDusk != 18.0 {
		t.Fatalf("public constants wrong: dawn=%v dusk=%v", TimeDawn, TimeDusk)
	}

	g.SetTimeOfDay(0)
	before := clockLine(g)
	g.SetTimeOfDayScale(2)
	stepN(w, 100)
	after := clockLine(g)
	t.Logf("FSV api clock scale BEFORE: %s", before)
	t.Logf("FSV api clock scale AFTER:  %s", after)

	if w.Tick() != 100 {
		t.Fatalf("tick rate changed: tick=%d want 100", w.Tick())
	}
	if got, want := g.TimeOfDay(), 0.5; got != want {
		t.Fatalf("TimeOfDay after 100 ticks @2x = %.8f, want %.8f", got, want)
	}
	if raw := toFloat(w.TimeOfDay()); raw != 0.5 {
		t.Fatalf("raw world ToD = %.8f, want 0.5", raw)
	}
	if got := g.TimeOfDayScale(); got != 2 {
		t.Fatalf("scale = %.8f, want 2", got)
	}
}

func TestClockWrapAndSuspend(t *testing.T) {
	w := sim.NewWorld(sim.Caps{})
	g := newGame(w)

	g.SetTimeOfDay(-1)
	neg := clockLine(g)
	g.SetTimeOfDay(25)
	over := clockLine(g)
	g.SuspendTimeOfDay(true)
	frozenBefore := clockLine(g)
	stepN(w, 25)
	frozenAfter := clockLine(g)
	g.SuspendTimeOfDay(false)
	stepN(w, 1)
	resumed := clockLine(g)

	t.Logf("FSV api clock SetTimeOfDay(-1): %s", neg)
	t.Logf("FSV api clock SetTimeOfDay(25): %s", over)
	t.Logf("FSV api clock frozen BEFORE:    %s", frozenBefore)
	t.Logf("FSV api clock frozen AFTER:     %s", frozenAfter)
	t.Logf("FSV api clock resumed AFTER:    %s", resumed)

	if got := toFloat(w.TimeOfDay()); over == neg || got != g.TimeOfDay() {
		t.Fatalf("test setup/readback failed: neg=%s over=%s raw=%v api=%v", neg, over, got, g.TimeOfDay())
	}
	g.SetTimeOfDay(-1)
	if got := g.TimeOfDay(); got != 23 {
		t.Fatalf("SetTimeOfDay(-1) = %.8f, want 23", got)
	}
	g.SetTimeOfDay(25)
	if got := g.TimeOfDay(); got != 1 {
		t.Fatalf("SetTimeOfDay(25) = %.8f, want 1", got)
	}
	g.SuspendTimeOfDay(true)
	before := g.TimeOfDay()
	stepN(w, 25)
	if got := g.TimeOfDay(); got != before {
		t.Fatalf("suspended ToD advanced: %.8f -> %.8f", before, got)
	}
	if !g.TimeOfDaySuspended() || !w.TimeOfDaySuspended() {
		t.Fatalf("suspended flags not set: api=%v raw=%v", g.TimeOfDaySuspended(), w.TimeOfDaySuspended())
	}
	g.SuspendTimeOfDay(false)
	if g.TimeOfDaySuspended() || w.TimeOfDaySuspended() {
		t.Fatalf("suspended flags not cleared: api=%v raw=%v", g.TimeOfDaySuspended(), w.TimeOfDaySuspended())
	}
}

func TestClockNilGameAndElapsedTime(t *testing.T) {
	var nilGame *Game
	empty := &Game{}
	t.Logf("FSV api clock nil Game: TimeOfDay=%v Scale=%v Suspended=%v Elapsed=%v",
		nilGame.TimeOfDay(), nilGame.TimeOfDayScale(), nilGame.TimeOfDaySuspended(), nilGame.ElapsedTime())
	t.Logf("FSV api clock empty Game: TimeOfDay=%v Scale=%v Suspended=%v Elapsed=%v",
		empty.TimeOfDay(), empty.TimeOfDayScale(), empty.TimeOfDaySuspended(), empty.ElapsedTime())

	nilGame.SetTimeOfDay(7)
	nilGame.SetTimeOfDayScale(2)
	nilGame.SuspendTimeOfDay(true)
	empty.SetTimeOfDay(7)
	empty.SetTimeOfDayScale(2)
	empty.SuspendTimeOfDay(true)
	if nilGame.TimeOfDay() != 0 || nilGame.TimeOfDayScale() != 0 || nilGame.TimeOfDaySuspended() || nilGame.ElapsedTime() != 0 {
		t.Fatal("nil Game getters must return zero values")
	}
	if empty.TimeOfDay() != 0 || empty.TimeOfDayScale() != 0 || empty.TimeOfDaySuspended() || empty.ElapsedTime() != 0 {
		t.Fatal("empty Game getters must return zero values")
	}

	w := sim.NewWorld(sim.Caps{})
	g := newGame(w)
	before := clockLine(g)
	stepN(w, 200)
	after := clockLine(g)
	t.Logf("FSV api clock elapsed BEFORE: %s", before)
	t.Logf("FSV api clock elapsed AFTER:  %s", after)
	if w.Tick() != 200 {
		t.Fatalf("tick=%d want 200", w.Tick())
	}
	if got := g.ElapsedTime(); got != 10.0 {
		t.Fatalf("ElapsedTime after 200 ticks = %.17g, want 10.0", got)
	}
}

func TestClockNegativeScaleNoop(t *testing.T) {
	w := sim.NewWorld(sim.Caps{})
	g := newGame(w)
	g.SetTimeOfDayScale(3)
	before := clockLine(g)
	g.SetTimeOfDayScale(-1)
	after := clockLine(g)
	t.Logf("FSV api clock negative scale BEFORE: %s", before)
	t.Logf("FSV api clock negative scale AFTER:  %s", after)
	if got := g.TimeOfDayScale(); got != 3 {
		t.Fatalf("negative scale changed scale: got %.8f want 3", got)
	}
	if raw := toFloat(w.TimeOfDayScale()); raw != 3 {
		t.Fatalf("negative scale changed raw world scale: got %.8f want 3", raw)
	}
}

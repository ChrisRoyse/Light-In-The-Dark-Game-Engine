package litd

// FSV for Game.Advance (#268/#269 host-loop primitive). SoT = the actual sim
// effect of a cooperative thread resuming across ticks: a thread that mutates a
// unit, PolledWaits, then mutates again must show the FIRST mutation immediately
// (synchronous run to first wait) and the SECOND only after Advance reaches the
// wake tick — never before, exactly then.

import (
	"testing"
	"time"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
)

func TestAdvanceResumesThreadFSV(t *testing.T) {
	g, err := NewGame(GameOptions{MaxUnits: 16, Seed: 17})
	if err != nil {
		t.Fatalf("NewGame: %v", err)
	}
	if err := g.DefineUnits([]data.Unit{
		{ID: "hfoo", Life: 100, MoveSpeedPerTick: 8 * fixed.One, TurnRatePerTick: 65535, CollisionSize: 16},
	}); err != nil {
		t.Fatalf("DefineUnits: %v", err)
	}
	u := g.CreateUnit(g.Player(1), g.UnitType("hfoo"), Vec2{X: 0, Y: 0}, Deg(0))

	// A cooperative thread: set life 10, wait 100ms (= 2 ticks), set life 20.
	g.Run(func(th *Thread) {
		u.SetLife(10)
		th.PolledWait(100 * time.Millisecond)
		u.SetLife(20)
	})

	// Run executed the thread synchronously to its first wait: life is 10, the
	// post-wait mutation has NOT happened.
	if got := u.Life(); got != 10 {
		t.Fatalf("after Run (pre-wait): Life=%v, want 10", got)
	}
	if g.SuspendedThreadCount() != 1 {
		t.Fatalf("expected 1 suspended thread, got %d", g.SuspendedThreadCount())
	}
	t.Logf("FSV pre-advance: Life=10, suspended=%d", g.SuspendedThreadCount())

	// One tick is not enough (100ms quantizes to 2 ticks) — still parked.
	g.Advance(1)
	if got := u.Life(); got != 10 {
		t.Fatalf("after Advance(1): Life=%v, want still 10 (wake is tick 2)", got)
	}

	// The second tick reaches the wake: the thread resumes and runs to completion.
	g.Advance(1)
	if got := u.Life(); got != 20 {
		t.Fatalf("after Advance(2 total): Life=%v, want 20 (thread did not resume)", got)
	}
	if g.SuspendedThreadCount() != 0 {
		t.Fatalf("thread should have finished, suspended=%d", g.SuspendedThreadCount())
	}
	t.Logf("FSV post-advance: Life=20, suspended=0 — thread resumed at the wake tick")
}

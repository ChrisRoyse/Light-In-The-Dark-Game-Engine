package net

import (
	"testing"
	"time"
)

// #71 — stall handling policy core. SoT = the StallDecision the controller emits
// under synthetic injected time. X+X=Y: with a known 30 s grace and a known clock,
// the pause→countdown→drop/resume transitions are exactly predictable. Every
// transition prints phase BEFORE and AFTER.
func TestStallControllerHappyRecoverWithinGraceFSV(t *testing.T) {
	c, err := NewStallController(DefaultGrace)
	if err != nil {
		t.Fatal(err)
	}
	const T = uint64(5)
	missing := []uint8{3}

	// BEFORE: Running. First blocked poll → enters Waiting (Began), full grace left.
	t.Logf("FSV BEFORE phase=%s", c.Phase())
	d := c.Observe(true, T, missing, 0)
	t.Logf("FSV t=0s blocked → %+v phase=%s", d, c.Phase())
	if !d.Began || d.Phase != PhaseWaiting || d.Remaining != DefaultGrace {
		t.Fatalf("entry: want Began waiting remaining=30s, got %+v", d)
	}
	if d.Resumed || len(d.Dropped) != 0 {
		t.Fatalf("entry must not drop/resume: %+v", d)
	}

	// Still blocked at t=10s: countdown shows 20 s remaining, no drop.
	d = c.Observe(true, T, missing, 10*time.Second)
	t.Logf("FSV t=10s blocked → remaining=%v dropped=%v", d.Remaining, d.Dropped)
	if d.Began || d.Phase != PhaseWaiting || d.Remaining != 20*time.Second {
		t.Fatalf("countdown: want waiting remaining=20s no-began, got %+v", d)
	}

	// Peer recovers at t=12s (inside grace): the turn aggregate arrives → not blocked.
	// SoT: Resume at turn T, NO drop.
	d = c.Observe(false, T, nil, 12*time.Second)
	t.Logf("FSV t=12s recovered → %+v phase=%s", d, c.Phase())
	if !d.Resumed || d.ResumeTurn != T || len(d.Dropped) != 0 || d.Phase != PhaseRunning {
		t.Fatalf("recover-in-grace: want Resume@%d no-drop running, got %+v", T, d)
	}

	// After resume, a clean Running poll is a no-op.
	d = c.Observe(false, 0, nil, 13*time.Second)
	if d.Resumed || d.Began || d.Phase != PhaseRunning {
		t.Fatalf("post-resume running poll must be no-op, got %+v", d)
	}
}

// Grace expiry → drop the named laggard + resume at the blocked turn.
func TestStallControllerGraceExpiryDropsFSV(t *testing.T) {
	c, _ := NewStallController(DefaultGrace)
	const T = uint64(8)
	missing := []uint8{2, 6} // two laggards

	c.Observe(true, T, missing, 0) // enter Waiting at t=0
	d := c.Observe(true, T, missing, 29*time.Second)
	if d.Resumed || len(d.Dropped) != 0 {
		t.Fatalf("t=29s (<30s grace) must not drop yet: %+v", d)
	}
	t.Logf("FSV t=29s phase=%s remaining=%v (no drop)", c.Phase(), d.Remaining)

	// t=30s: grace elapsed exactly → drop both laggards, resume at T.
	d = c.Observe(true, T, missing, 30*time.Second)
	t.Logf("FSV t=30s grace expired → dropped=%v resume@%d phase=%s", d.Dropped, d.ResumeTurn, c.Phase())
	if !d.Resumed || d.ResumeTurn != T || d.Phase != PhaseRunning {
		t.Fatalf("expiry: want Resume@%d running, got %+v", T, d)
	}
	if len(d.Dropped) != 2 || d.Dropped[0] != 2 || d.Dropped[1] != 6 {
		t.Fatalf("expiry: want dropped [2 6], got %v", d.Dropped)
	}
	// Returned slice must be a copy — mutating it must not corrupt the caller's input.
	d.Dropped[0] = 99
	if missing[0] != 2 {
		t.Fatalf("Dropped aliased the caller's missing slice: %v", missing)
	}
}

// Edge cases: re-block on a later turn restarts the grace clock; a blocked turn
// with no laggard named never emits a no-op drop (fail-closed); constructor
// rejects a non-positive grace.
func TestStallControllerEdgesFSV(t *testing.T) {
	// Edge 1 — constructor rejects grace <= 0 (would drop on the first blocked frame).
	if _, err := NewStallController(0); err == nil {
		t.Fatal("grace=0 must be rejected")
	}
	if _, err := NewStallController(-time.Second); err == nil {
		t.Fatal("negative grace must be rejected")
	}

	c, _ := NewStallController(10 * time.Second)

	// Block on turn 4 at t=0, then the gate advances and re-blocks on turn 7 at t=5s.
	// The new turn restarts the clock: at t=5s it is Began with full 10 s remaining,
	// NOT 5 s into turn 4's window — each stalled turn gets its own grace.
	c.Observe(true, 4, []uint8{1}, 0)
	d := c.Observe(true, 7, []uint8{1}, 5*time.Second)
	t.Logf("FSV re-block on turn 7 @ t=5s → began=%v remaining=%v waitingTurn=%d", d.Began, d.Remaining, c.WaitingTurn())
	if !d.Began || d.Remaining != 10*time.Second || c.WaitingTurn() != 7 {
		t.Fatalf("re-block must restart grace for turn 7 with full 10s: %+v", d)
	}
	// Now let turn 7 run past grace but name NO laggard → must stay Waiting (defensive),
	// never a phantom drop/resume.
	d = c.Observe(true, 7, nil, 20*time.Second)
	t.Logf("FSV grace-expired, missing=∅ → phase=%s resumed=%v dropped=%v", c.Phase(), d.Resumed, d.Dropped)
	if d.Resumed || len(d.Dropped) != 0 || d.Phase != PhaseWaiting {
		t.Fatalf("expired-but-no-laggard must stay Waiting (no phantom drop), got %+v", d)
	}
	// Name the laggard now → drop + resume.
	d = c.Observe(true, 7, []uint8{1}, 21*time.Second)
	if !d.Resumed || len(d.Dropped) != 1 || d.Dropped[0] != 1 {
		t.Fatalf("laggard named after expiry → drop+resume, got %+v", d)
	}
}

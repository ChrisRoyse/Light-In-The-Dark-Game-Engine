package net

// #66 FSV: the adaptive input-delay buffer. SoT = the (turn → effective-delay)
// schedule each peer derives from the host's announcement stream — identical
// across peers, growing at announced future boundaries under RTT pressure and
// shrinking to the floor on recovery, never reassigning already-scheduled turns.

import (
	"testing"
	"time"
)

const testTurnPeriod = 150 * time.Millisecond // 3 ticks × 50 ms

type announcement struct {
	eff   uint64
	delay int
}

// drive runs the controller over an RTT trace (one sample per turn) and returns
// the announcements it emitted. host schedule is mutated in place.
func drive(t *testing.T, ctrl *DelayController, rtts []time.Duration) []announcement {
	t.Helper()
	var out []announcement
	for turn, rtt := range rtts {
		eff, nd, changed := ctrl.Observe(rtt, uint64(turn))
		if changed {
			if eff <= uint64(turn) {
				t.Fatalf("announcement effectiveTurn %d not in the future of observe-turn %d", eff, turn)
			}
			out = append(out, announcement{eff, nd})
			t.Logf("  observe turn=%d rtt=%v → announce delay=%d effectiveTurn=%d", turn, rtt, nd, eff)
		}
	}
	return out
}

// replay applies announcements to a fresh peer schedule (what every client does).
func replay(t *testing.T, anns []announcement) *DelaySchedule {
	t.Helper()
	s := NewDelaySchedule()
	for _, a := range anns {
		if err := s.Announce(a.eff, a.delay); err != nil {
			t.Fatalf("replay Announce(%d,%d): %v", a.eff, a.delay, err)
		}
	}
	return s
}

func TestDelaySpikeGrowsAtBoundary(t *testing.T) {
	host := NewDelaySchedule()
	ctrl, err := NewDelayController(testTurnPeriod, 2, 3, host)
	if err != nil {
		t.Fatalf("NewDelayController: %v", err)
	}
	// Sustained 300 ms RTT (> 150 ms turn period) for several turns.
	rtts := make([]time.Duration, 8)
	for i := range rtts {
		rtts[i] = 300 * time.Millisecond
	}
	t.Log("FSV spike trace: 8× 300ms (> 150ms turn period)")
	anns := drive(t, ctrl, rtts)
	if len(anns) == 0 {
		t.Fatal("sustained high RTT produced no delay increase")
	}
	// Each announcement: delay jumps from old→new EXACTLY at effectiveTurn.
	for _, a := range anns {
		if a.eff == 0 {
			continue
		}
		before := host.DelayAt(a.eff - 1)
		at := host.DelayAt(a.eff)
		if at != a.delay || before >= at {
			t.Fatalf("at boundary %d: DelayAt(eff-1)=%d DelayAt(eff)=%d announced=%d (must step up exactly here)", a.eff, before, at, a.delay)
		}
	}
	// Delay grew above the initial 2.
	last := anns[len(anns)-1]
	if last.delay <= DefaultDelay {
		t.Fatalf("delay did not grow under pressure: ended at %d", last.delay)
	}
	// A second peer fed the same announcements derives the identical schedule.
	peer := replay(t, anns)
	for turn := uint64(0); turn <= last.eff+5; turn++ {
		if host.DelayAt(turn) != peer.DelayAt(turn) {
			t.Fatalf("schedule mismatch at turn %d: host=%d peer=%d", turn, host.DelayAt(turn), peer.DelayAt(turn))
		}
	}
	t.Logf("FSV spike: grew %d→%d across %d announced boundaries; peer schedule identical over [0,%d]", DefaultDelay, last.delay, len(anns), last.eff+5)
}

func TestDelayRecoveryShrinksToFloor(t *testing.T) {
	host := NewDelaySchedule()
	ctrl, _ := NewDelayController(testTurnPeriod, 2, 2, host)
	// First push delay UP with high RTT, then recover with low RTT.
	trace := []time.Duration{}
	for i := 0; i < 6; i++ {
		trace = append(trace, 300*time.Millisecond) // raise
	}
	for i := 0; i < 30; i++ {
		trace = append(trace, 20*time.Millisecond) // < 75ms headroom → lower
	}
	anns := drive(t, ctrl, trace)
	// Final delay must be the floor.
	finalTurn := uint64(len(trace)) + 20
	if got := host.DelayAt(finalTurn); got != MinDelay {
		t.Fatalf("after sustained recovery, delay=%d at turn %d, want floor %d", got, finalTurn, MinDelay)
	}
	// Never below the floor at any announced boundary.
	for _, a := range anns {
		if a.delay < MinDelay || a.delay > MaxDelay {
			t.Fatalf("announced delay %d out of [%d,%d]", a.delay, MinDelay, MaxDelay)
		}
	}
	peer := replay(t, anns)
	if peer.DelayAt(finalTurn) != MinDelay {
		t.Fatalf("peer recovery schedule disagrees: %d", peer.DelayAt(finalTurn))
	}
	t.Logf("FSV recovery: delay returned to floor %d (clamped, never below); peer agrees", MinDelay)
}

// TestDelayNoRetroactiveReorder — a delay change announced for a FUTURE turn
// never changes the turn already-issued commands were scheduled into.
func TestDelayNoRetroactiveReorder(t *testing.T) {
	s := NewDelaySchedule() // delay 2
	// Commands issued during turns 10 and 11 schedule to T+2.
	a10 := s.ScheduleFor(10)
	a11 := s.ScheduleFor(11)
	t.Logf("FSV before announce: cmd@10→turn%d, cmd@11→turn%d (delay %d)", a10, a11, s.DelayAt(10))
	if a10 != 12 || a11 != 13 {
		t.Fatalf("initial schedule wrong: a10=%d a11=%d", a10, a11)
	}
	// Host announces delay 4 effective at turn 20 (a future boundary).
	if err := s.Announce(20, 4); err != nil {
		t.Fatalf("Announce: %v", err)
	}
	// The already-scheduled turns are unchanged — DelayAt for turns <20 is still 2.
	if got10, got11 := s.ScheduleFor(10), s.ScheduleFor(11); got10 != a10 || got11 != a11 {
		t.Fatalf("announcement reordered prior schedule: 10→%d (was %d), 11→%d (was %d)", got10, a10, got11, a11)
	}
	// Commands issued at/after turn 20 use the new delay.
	if got := s.ScheduleFor(20); got != 24 {
		t.Fatalf("cmd@20 → turn %d, want 24 (delay 4)", got)
	}
	t.Logf("FSV after announce(turn20,delay4): cmd@10→turn%d, cmd@11→turn%d UNCHANGED; cmd@20→turn%d", s.ScheduleFor(10), s.ScheduleFor(11), s.ScheduleFor(20))
}

// TestDelayScheduleClampAndMonotonic — fail-closed guards on Announce.
func TestDelayScheduleClampAndMonotonic(t *testing.T) {
	s := NewDelaySchedule()
	if err := s.Announce(5, MaxDelay+1); err == nil {
		t.Fatal("over-max delay accepted")
	}
	if err := s.Announce(5, 0); err == nil {
		t.Fatal("below-min delay accepted")
	}
	if err := s.Announce(5, 3); err != nil {
		t.Fatalf("valid announce rejected: %v", err)
	}
	if err := s.Announce(5, 4); err == nil {
		t.Fatal("non-monotonic effectiveTurn accepted (same turn)")
	}
	if err := s.Announce(4, 4); err == nil {
		t.Fatal("backward effectiveTurn accepted")
	}
	t.Log("FSV guards: clamp [1,8] + strictly-increasing effectiveTurn enforced")
}

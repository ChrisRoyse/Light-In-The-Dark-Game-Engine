package net

// #84 observer-slot FSV. Two Sources of Truth:
//  1. the ObserverSet's own state (count / cursor / Pending range) under a known
//     join/leave/deliver sequence — X+X=Y on turn indices;
//  2. the REAL lockstep gate (RoundGate): with observers churning alongside it,
//     every round must still close on the host alone (Status==GateAggregated,
//     Roster==[HostPlayer]) — an observer must never leak into the waiting set
//     nor stall the host. That is #84's central invariant, checked against the
//     actual gate, not a model of it.

import (
	"bytes"
	"fmt"
	"testing"
	"time"
)

func TestObserverSetLifecycleFSV(t *testing.T) {
	s := NewObserverSet()
	if s.Count() != 0 {
		t.Fatalf("fresh set count=%d, want 0", s.Count())
	}

	// Join mid-match at turn 50 (zero-delay: cursor starts at the join turn).
	a := s.Join("alice", 50)
	b := s.Join("bob", 50)
	if s.Count() != 2 || a == b {
		t.Fatalf("count=%d a=%d b=%d, want 2 distinct ids", s.Count(), a, b)
	}
	list := s.Observers()
	if len(list) != 2 || list[0].ID != a || list[0].Cursor() != 50 || list[0].JoinTurn != 50 {
		t.Fatalf("observers=%+v, want alice first cursor/join=50", list)
	}
	t.Logf("FSV join: alice=%d bob=%d both cursor=50 (joined at current turn)", a, b)

	// Pending = [cursor..latest] inclusive. BEFORE delivery: 50,51,52,53.
	pend := s.Pending(a, 53)
	if len(pend) != 4 || pend[0] != 50 || pend[3] != 53 {
		t.Fatalf("pending=%v, want [50 51 52 53]", pend)
	}
	// Deliver up to 51 → cursor 52 → pending shrinks to 52,53.
	if c := s.Deliver(a, 51); c != 52 {
		t.Fatalf("after Deliver(51) cursor=%d, want 52", c)
	}
	pend = s.Pending(a, 53)
	if len(pend) != 2 || pend[0] != 52 || pend[1] != 53 {
		t.Fatalf("pending after deliver=%v, want [52 53]", pend)
	}
	// Caught up: deliver to latest → empty pending (no fabricated turns).
	s.Deliver(a, 53)
	if p := s.Pending(a, 53); p != nil {
		t.Fatalf("caught-up pending=%v, want empty", p)
	}
	t.Log("FSV deliver: cursor advances contiguously; caught-up observer has empty Pending")

	// Edge — cursor only moves FORWARD: a backward Deliver is a no-op.
	s.Deliver(a, 60) // cursor 61
	if c := s.Deliver(a, 5); c != 61 {
		t.Fatalf("backward Deliver moved cursor to %d, want 61 (monotonic)", c)
	}

	// Edge — unknown id: Deliver→0, Pending→empty, Leave→false. No panic.
	if c := s.Deliver(ObserverID(999), 10); c != 0 {
		t.Fatalf("Deliver(unknown)=%d, want 0", c)
	}
	if p := s.Pending(ObserverID(999), 10); p != nil {
		t.Fatalf("Pending(unknown)=%v, want nil", p)
	}
	if s.Leave(ObserverID(999)) {
		t.Fatal("Leave(unknown) returned true")
	}
	t.Log("FSV edges: backward-deliver no-op; unknown id fails closed (no panic)")

	// Leave frees the slot; the surviving observer and join order are intact.
	if !s.Leave(a) || s.Count() != 1 || s.Has(a) {
		t.Fatalf("after Leave(alice): count=%d hasAlice=%v", s.Count(), s.Has(a))
	}
	if got := s.Observers(); len(got) != 1 || got[0].ID != b {
		t.Fatalf("survivors=%+v, want [bob=%d]", got, b)
	}
	t.Log("FSV leave: slot freed, survivor intact, join order preserved")
}

func TestObserverNeverGatesHostFSV(t *testing.T) {
	ctrl, err := NewStallController(30 * time.Second)
	if err != nil {
		t.Fatal(err)
	}
	gate, err := NewRoundGate(2, ctrl, 100*time.Millisecond)
	if err != nil {
		t.Fatal(err)
	}
	defer gate.Close()
	obs := NewObserverSet()

	const turns = 200
	var reached uint64
	var laggards []ObserverID
	for turn := uint64(1); turn <= turns; turn++ {
		host := [][]byte{gateRecord(t, uint32(turn*10), HostPlayer, 0)}
		step, err := gate.Step(turn, host, 0)
		if err != nil {
			t.Fatalf("turn %d Step: %v", turn, err)
		}
		// INVARIANT (SoT = the real gate's step): the round closed on the host
		// alone — observers are not in the roster and did not stall it.
		if step.Status != GateAggregated {
			t.Fatalf("turn %d status=%v, want aggregated — observers must never stall the gate", turn, step.Status)
		}
		if !bytes.Equal(asBytes(step.Roster), []byte{HostPlayer}) {
			t.Fatalf("turn %d roster=%v, want [%d] — an observer leaked into the lockstep set", turn, step.Roster, HostPlayer)
		}
		reached = turn

		// Observers churn alongside the gate; the laggards are NEVER delivered to.
		if turn == 50 {
			for i := 0; i < 3; i++ {
				laggards = append(laggards, obs.Join(fmt.Sprintf("watcher-%d", i), turn))
			}
		}
		if turn == 120 {
			obs.Leave(laggards[0]) // a mid-match departure must not perturb timing
		}
	}

	// SoT: the host advanced to the final turn regardless of observers and lag.
	if reached != turns {
		t.Fatalf("host advanced to %d, want %d", reached, turns)
	}
	if obs.Count() != 2 {
		t.Fatalf("observer count=%d, want 2 (one left mid-match)", obs.Count())
	}
	// Surviving laggards fell arbitrarily far behind with NO back-pressure:
	// joined at 50, never delivered → cursor still 50 → lag = 200-50+1 = 151.
	wantLag := uint64(turns - 50 + 1)
	if lag := obs.MaxLag(turns); lag != wantLag {
		t.Fatalf("max observer lag=%d, want %d (host advanced while laggard stood still)", lag, wantLag)
	}
	pend := obs.Pending(laggards[1], turns)
	if len(pend) != int(wantLag) || pend[0] != 50 || pend[len(pend)-1] != turns {
		t.Fatalf("laggard pending=%d turns, want %d spanning [50..%d]", len(pend), wantLag, turns)
	}
	t.Logf("FSV #84: gate closed all %d turns with roster always [%d]; %d observers lagging by %d turns never stalled the host",
		turns, HostPlayer, obs.Count(), wantLag)
}

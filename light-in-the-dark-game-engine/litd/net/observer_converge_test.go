package net

import (
	"testing"
)

// TestObserverConvergesToPlayerFSV proves #84's core constraint: an observer that
// receives the aggregate turn stream and runs the full sim — submitting nothing,
// occupying no player slot — reaches the SAME StateHash as a player at every
// checkpoint, and a lagging observer never gates the player. observer_test.go
// covers the ObserverSet cursor arithmetic; this wires that bookkeeping
// (Pending → deliver to the observer's sim → Deliver) to a REAL api.Game and reads
// the SoT (StateHash), so the "observer is a zero-delay replay viewer over the same
// command-stream machinery" claim is evidence, not assertion.
//
// Scope: the observer joins from turn 0 (cursor=0), which is the path that IS
// built. Mid-match join (cursor>0) needs replay-from-0 or a state snapshot that is
// not yet wired — documented separately on #84 — so it is deliberately not claimed
// here.
//
// SoT = observer StateHash vs the player's at the same tick. X+X=Y: identical
// broadcast turns → identical hash; a divergence (skipped turn) → different hash.
func TestObserverConvergesToPlayerFSV(t *testing.T) {
	// Reference player: a real sim fed every broadcast turn in order.
	gP, uP, _ := newTwin(t)
	gateP, _ := NewLockstepGate(2)

	// The broadcast stream: host moves the unit (turn 0) then halts it, so the
	// state the hash covers is non-trivial. turnLen=2 ⇒ turn N's records at tick 2N+1.
	payloads := [][]byte{
		turnAgg(t, moveRec(t, 1, 0, uP, 6000, 200)),
		turnAgg(t, stopRec(t, 3, 1, uP)),
		turnAgg(t, stopRec(t, 5, 2, uP)),
	}
	// Player consumes all turns up front → reference hash at each tick boundary.
	wantHashAtTick := map[uint32]uint64{}
	for turn, p := range payloads {
		if err := gateP.Deliver(uint64(turn), p); err != nil {
			t.Fatalf("player deliver turn %d: %v", turn, err)
		}
		gateP.Pump(gP)
		wantHashAtTick[gP.Tick()] = gP.StateHash()
	}
	if gP.Tick() != 6 {
		t.Fatalf("player reached tick %d, want 6", gP.Tick())
	}
	t.Logf("FSV reference: player at tick 6, hash %#x (unit moved)", wantHashAtTick[6])

	// --- happy path: observer joined at turn 0, kept in lockstep step-by-step. ---
	set := NewObserverSet()
	obsID := set.Join("synthetic_observer_2026_06_23", 0)
	gO, uO, _ := newTwin(t)
	if uO != uP {
		t.Fatalf("observer twin diverged at creation: unit %d vs %d", uO, uP)
	}
	gateO, _ := NewLockstepGate(2)

	// Host fans out turn-by-turn as each becomes the latest aggregated turn.
	for latest := uint64(0); latest < uint64(len(payloads)); latest++ {
		pend := set.Pending(obsID, latest)
		if len(pend) != 1 || pend[0] != latest {
			t.Fatalf("turn %d: Pending=%v, want [%d] (cursor keeps pace)", latest, pend, latest)
		}
		for _, turn := range pend {
			if err := gateO.Deliver(turn, payloads[turn]); err != nil {
				t.Fatalf("observer deliver turn %d: %v", turn, err)
			}
		}
		gateO.Pump(gO)
		set.Deliver(obsID, latest)

		// SoT: at each checkpoint the observer's hash equals the player's at the
		// same tick.
		if got, want := gO.StateHash(), wantHashAtTick[gO.Tick()]; got != want {
			t.Fatalf("checkpoint tick %d: observer hash %#x != player %#x", gO.Tick(), got, want)
		}
		t.Logf("FSV checkpoint: after latest turn %d, observer tick=%d hash=%#x == player", latest, gO.Tick(), gO.StateHash())
	}
	if gO.StateHash() != wantHashAtTick[6] {
		t.Fatalf("final observer hash %#x != player %#x", gO.StateHash(), wantHashAtTick[6])
	}

	// --- edge 1: a SLOW observer falls behind, the player is NEVER gated, then the
	// observer catches up via a batched Pending and still converges. ---
	setS := NewObserverSet()
	slowID := setS.Join("synthetic_slow_observer", 0)
	gS, _, _ := newTwin(t)
	gateS, _ := NewLockstepGate(2)
	// Host advances to the final turn while the slow observer has delivered nothing.
	latest := uint64(len(payloads) - 1)
	if pend := setS.Pending(slowID, latest); len(pend) != len(payloads) {
		t.Fatalf("slow observer Pending=%v, want all %d turns batched", pend, len(payloads))
	}
	maxLag := setS.MaxLag(latest)
	if maxLag != latest+1 {
		t.Fatalf("MaxLag=%d, want %d (observer is the full stream behind)", maxLag, latest+1)
	}
	t.Logf("FSV edge slow: observer lags MaxLag=%d turns; player is unaffected (host never reads observer state to advance)", maxLag)
	// Observer drains the batch in order and converges.
	for _, turn := range setS.Pending(slowID, latest) {
		if err := gateS.Deliver(turn, payloads[turn]); err != nil {
			t.Fatalf("slow deliver turn %d: %v", turn, err)
		}
	}
	gateS.Pump(gS)
	setS.Deliver(slowID, latest)
	if gS.Tick() != 6 || gS.StateHash() != wantHashAtTick[6] {
		t.Fatalf("slow observer after catch-up: tick=%d hash=%#x, want tick 6 hash %#x", gS.Tick(), gS.StateHash(), wantHashAtTick[6])
	}
	if pend := setS.Pending(slowID, latest); len(pend) != 0 {
		t.Fatalf("caught-up observer still Pending=%v, want empty", pend)
	}
	t.Logf("FSV edge slow: after batched catch-up, observer tick=%d hash=%#x == player; Pending now empty", gS.Tick(), gS.StateHash())

	// --- edge 2: duplicate/replayed Deliver must not double-apply (cursor is
	// monotonic; the LockstepGate also rejects a re-delivered turn). State holds. ---
	setS.Deliver(slowID, latest) // replay the same upTo
	if pend := setS.Pending(slowID, latest); len(pend) != 0 {
		t.Fatalf("after replayed Deliver, Pending=%v, want still empty (cursor monotonic)", pend)
	}
	if err := gateS.Deliver(latest, payloads[latest]); err == nil {
		t.Fatal("re-delivering an already-applied turn must be rejected by the LockstepGate")
	}
	if gS.StateHash() != wantHashAtTick[6] {
		t.Fatalf("duplicate delivery corrupted observer state: hash %#x != %#x", gS.StateHash(), wantHashAtTick[6])
	}
	t.Logf("FSV edge dup: replayed Deliver + re-Deliver rejected, observer hash unchanged %#x", gS.StateHash())

	// --- edge 3: observer leaves mid-life → registry shrinks, no effect on the
	// player sim; Pending for the gone id is empty (fail-closed). ---
	beforeTick, beforeHash := gP.Tick(), gP.StateHash()
	if !set.Leave(obsID) {
		t.Fatal("Leave(obsID) returned false for a live observer")
	}
	if set.Count() != 0 || set.Has(obsID) {
		t.Fatalf("after Leave: Count=%d Has=%v, want 0/false", set.Count(), set.Has(obsID))
	}
	if pend := set.Pending(obsID, 99); pend != nil {
		t.Fatalf("Pending for departed observer = %v, want nil", pend)
	}
	if gP.Tick() != beforeTick || gP.StateHash() != beforeHash {
		t.Fatal("an observer leaving perturbed the player sim — observers must be side-effect free on lockstep state")
	}
	t.Logf("FSV edge leave: observer removed, registry empty, player sim untouched (tick=%d hash=%#x)", gP.Tick(), gP.StateHash())
}

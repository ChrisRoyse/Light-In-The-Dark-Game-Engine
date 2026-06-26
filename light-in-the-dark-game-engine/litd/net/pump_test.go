package net

// #71 peer-pump FSV. SoT = the (payload, pumpStatus) Poll returns for a stream we
// drive byte-exact over an in-memory net.Pipe (X+X=Y: write turn "turn-7" => Poll
// yields exactly "turn-7"). Covers the three lifecycle outcomes (pending / ready /
// closed) and the load-bearing invariant: a Poll that times out leaves ONE read
// outstanding, so a later delivery is picked up without a second concurrent
// RecvTurn (which the Session forbids).

import (
	"net"
	"testing"
	"time"
)

// pipePair builds two Sessions wired to opposite ends of a synchronous in-memory
// pipe. Writing a turn on far is read by near's pump.
func pipePair() (near, far *Session) {
	c0, c1 := net.Pipe()
	near = &Session{stream: c0, remote: "near", closeFn: c0.Close}
	far = &Session{stream: c1, remote: "far", closeFn: c1.Close}
	return near, far
}

func TestPeerPumpLifecycleFSV(t *testing.T) {
	near, far := pipePair()
	pump := newPeerPump(near)
	defer pump.Close()
	defer far.Close()

	// BEFORE: nothing written. SoT: Poll times out => PumpPending, read outstanding.
	if _, st := pump.Poll(50 * time.Millisecond); st != PumpPending {
		t.Fatalf("empty stream: status=%v, want pending", st)
	}
	if !pump.pending {
		t.Fatal("after a pending Poll the read must stay outstanding")
	}
	t.Logf("FSV pending: status=pending, pending-flag=%v (no turn written)", pump.pending)

	// ACTION: far peer sends a turn. The pump's already-outstanding read consumes
	// it — no second RecvTurn is issued. net.Pipe Write blocks until read, so send
	// from a goroutine.
	want := []byte("turn-7-synthetic")
	go func() { _ = far.SendTurn(want) }()

	// AFTER: next Poll yields exactly the bytes sent.
	got, st := pump.Poll(2 * time.Second)
	if st != PumpReady {
		t.Fatalf("after send: status=%v, want ready", st)
	}
	if string(got) != string(want) {
		t.Fatalf("payload=%q, want %q", got, want)
	}
	if pump.pending {
		t.Fatal("a consumed read must clear the outstanding flag")
	}
	t.Logf("FSV ready: status=ready payload=%q (byte-exact round-trip)", got)

	// A SECOND turn on the same pump starts a fresh read and round-trips too.
	want2 := []byte("turn-8")
	go func() { _ = far.SendTurn(want2) }()
	got2, st2 := pump.Poll(2 * time.Second)
	if st2 != PumpReady || string(got2) != string(want2) {
		t.Fatalf("second turn: status=%v payload=%q, want ready %q", st2, got2, want2)
	}
	t.Logf("FSV second turn: status=ready payload=%q", got2)
}

func TestPeerPumpCloseLatchesFSV(t *testing.T) {
	near, far := pipePair()
	pump := newPeerPump(near)
	defer pump.Close()

	// ACTION: peer's stream closes before any turn. SoT: Poll => PumpClosed.
	_ = far.Close()
	if _, st := pump.Poll(2 * time.Second); st != PumpClosed {
		t.Fatalf("closed peer: status=%v, want closed", st)
	}
	if !pump.Closed() {
		t.Fatal("Closed() must report true after a read error")
	}
	t.Logf("FSV closed: status=closed, latched=%v", pump.Closed())

	// Edge: the closed status is LATCHED — a closed peer never spuriously reports a
	// turn on a later Poll (fail-closed).
	for i := 0; i < 3; i++ {
		if _, st := pump.Poll(10 * time.Millisecond); st != PumpClosed {
			t.Fatalf("re-poll %d after close: status=%v, want closed", i, st)
		}
	}
	t.Log("FSV latch: 3 re-polls after close all stayed closed")
}

// Edge: a peer that goes quiet AFTER delivering one turn reports pending on the
// next round (the lockstep gate would then consult the StallController), not a
// stale repeat of the prior turn and not a false close.
func TestPeerPumpQuietAfterTurnFSV(t *testing.T) {
	near, far := pipePair()
	pump := newPeerPump(near)
	defer pump.Close()
	defer far.Close()

	want := []byte("only-turn")
	go func() { _ = far.SendTurn(want) }()
	if got, st := pump.Poll(2 * time.Second); st != PumpReady || string(got) != string(want) {
		t.Fatalf("first turn: status=%v payload=%q", st, got)
	}

	// Peer now stalls. SoT: next Poll => PumpPending (not Ready-repeat, not Closed).
	if got, st := pump.Poll(50 * time.Millisecond); st != PumpPending || got != nil {
		t.Fatalf("quiet round: status=%v payload=%q, want pending nil", st, got)
	}
	t.Log("FSV quiet-after-turn: status=pending (no stale repeat, no false close)")
}

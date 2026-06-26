package net

import (
	"bytes"
	"testing"
)

// TestQUICFullMatchEqualHashFSV is the strongest headless proxy for #85 short of two
// physical machines: a host and a client exchange the real per-turn aggregate stream
// over a REAL loopback QUIC connection (dialPair → 127.0.0.1, self-signed TLS,
// quic-go) and both run independent deterministic sims that must end on an identical
// StateHash. TestSessionTurnRoundTrip proves the bytes survive the wire; this proves
// those bytes drive two endpoints' sims to the same state — lockstep determinism over
// the ACTUAL transport, not in-process like cmd/desyncfsv.
//
// What this is NOT: two physical machines on a LAN with screenshots — that is #85's
// operator-gated exit run, which cannot be fabricated. What it IS: real sockets, real
// QUIC framing, real turn serialization — every layer of the netcode stack except the
// physical network between two hosts.
//
// SoT = host sim StateHash vs client sim StateHash, read each turn. X+X=Y: the same
// aggregates over the wire → the same hash; a divergent final turn → a different hash
// (the negative control).
func TestQUICFullMatchEqualHashFSV(t *testing.T) {
	host, client, cleanup := dialPair(t)
	defer cleanup()

	gH, uH, _ := newTwin(t)
	gC, uC, _ := newTwin(t)
	if uH != uC {
		t.Fatalf("twins diverged at creation: unit %d vs %d", uH, uC)
	}
	gateH, _ := NewLockstepGate(2)
	gateC, _ := NewLockstepGate(2)

	// Host drives a unit into motion (turn 0) then halts it, so the hashed state is
	// non-trivial. turnLen=2 ⇒ turn N's records sit at tick 2N+1.
	payloads := [][]byte{
		turnAgg(t, moveRec(t, 1, 0, uH, 7000, 300)),
		turnAgg(t, stopRec(t, 3, 1, uH)),
		turnAgg(t, stopRec(t, 5, 2, uH)),
	}

	for turn, p := range payloads {
		// Host applies the aggregate locally AND broadcasts it over real QUIC.
		if err := gateH.Deliver(uint64(turn), p); err != nil {
			t.Fatalf("host deliver turn %d: %v", turn, err)
		}
		gateH.Pump(gH)
		if err := host.SendTurn(p); err != nil {
			t.Fatalf("host SendTurn turn %d: %v", turn, err)
		}

		// Client receives it off the wire and applies it.
		got, err := client.RecvTurn()
		if err != nil {
			t.Fatalf("client RecvTurn turn %d: %v", turn, err)
		}
		if !bytes.Equal(got, p) {
			t.Fatalf("turn %d corrupted in transit: sent %x, got %x", turn, p, got)
		}
		if err := gateC.Deliver(uint64(turn), got); err != nil {
			t.Fatalf("client deliver turn %d: %v", turn, err)
		}
		gateC.Pump(gC)

		// SoT each turn: the two endpoints stay tick- and hash-locked over the wire.
		if gH.Tick() != gC.Tick() {
			t.Fatalf("turn %d: ticks diverged over QUIC H=%d C=%d", turn, gH.Tick(), gC.Tick())
		}
		if gH.StateHash() != gC.StateHash() {
			t.Fatalf("turn %d: StateHash diverged over QUIC H=%#x C=%#x", turn, gH.StateHash(), gC.StateHash())
		}
		t.Logf("FSV turn %d over real QUIC: both endpoints at tick %d, StateHash %#x", turn, gH.Tick(), gH.StateHash())
	}
	if gH.Tick() != 6 {
		t.Fatalf("match ended at tick %d, want 6", gH.Tick())
	}
	final := gH.StateHash()
	t.Logf("FSV #85 proxy: host + client reached tick 6 over REAL loopback QUIC with IDENTICAL StateHash %#x", final)

	// Negative control: a client that applied a DIFFERENT final turn ends on a
	// different hash — the equality is the transport+determinism, not a constant.
	gX, uX, _ := newTwin(t)
	gateX, _ := NewLockstepGate(2)
	if err := gateX.Deliver(0, payloads[0]); err != nil {
		t.Fatal(err)
	}
	gateX.Pump(gX)
	if err := gateX.Deliver(1, payloads[1]); err != nil {
		t.Fatal(err)
	}
	gateX.Pump(gX)
	if err := gateX.Deliver(2, turnAgg(t, moveRec(t, 5, 2, uX, 1, 1))); err != nil { // a different turn 2
		t.Fatal(err)
	}
	gateX.Pump(gX)
	if gX.StateHash() == final {
		t.Fatalf("control with a divergent final turn matched the match hash %#x — the equality has no teeth", final)
	}
	t.Logf("FSV control: a divergent final turn yields %#x != match %#x — equality has teeth", gX.StateHash(), final)
}

package net

// #71 RoundGate FSV. SoT = the GateStep a Step returns (status / waiting / dropped /
// roster) AND the byte-exact aggregate payload, cross-checked against a reference
// TurnBuffer built over the survivors. X+X=Y: known host+peer records => the gate's
// broadcast payload must be byte-identical to the reference aggregate of exactly
// those players. Drives peers over in-memory net.Pipes; the grace clock is injected.

import (
	"bytes"
	"testing"
	"time"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
)

func gateRecord(t *testing.T, tick uint32, player uint8, seq uint16) []byte {
	t.Helper()
	r := sim.CommandRecord{
		Version: sim.CommandVersion,
		Tick:    tick,
		Player:  player,
		Seq:     seq,
		Opcode:  sim.OpStop,
	}
	b, ok := sim.AppendEncode(nil, &r)
	if !ok {
		t.Fatalf("AppendEncode failed tick=%d player=%d seq=%d", tick, player, seq)
	}
	return b
}

// refAggregate builds the canonical broadcast payload for the given player->records
// over a fresh TurnBuffer — the independent SoT the gate must match byte-for-byte.
func refAggregate(t *testing.T, turnLen int, turn uint64, subs map[uint8][][]byte) []byte {
	t.Helper()
	players := make([]uint8, 0, len(subs))
	for p := range subs {
		players = append(players, p)
	}
	tb, err := NewTurnBuffer(turnLen, players)
	if err != nil {
		t.Fatalf("ref buffer: %v", err)
	}
	for p, recs := range subs {
		if err := tb.Submit(turn, p, recs); err != nil {
			t.Fatalf("ref submit %d: %v", p, err)
		}
	}
	payload, err := tb.Aggregate(turn)
	if err != nil {
		t.Fatalf("ref aggregate: %v", err)
	}
	return payload
}

// sendPeerTurn writes a peer's turn (encoded records) on the far end of its pipe.
// net.Pipe is synchronous, so send from a goroutine.
func sendPeerTurn(t *testing.T, far *Session, records [][]byte) {
	t.Helper()
	payload, err := EncodeTurn(records)
	if err != nil {
		t.Fatalf("EncodeTurn: %v", err)
	}
	go func() { _ = far.SendTurn(payload) }()
}

func TestRoundGateHappyAllSubmitFSV(t *testing.T) {
	ctrl, err := NewStallController(30 * time.Second)
	if err != nil {
		t.Fatal(err)
	}
	gate, err := NewRoundGate(2, ctrl, 500*time.Millisecond)
	if err != nil {
		t.Fatal(err)
	}
	defer gate.Close()

	nearA, farA := pipePair()
	nearB, farB := pipePair()
	if err := gate.AddPeer(1, nearA); err != nil {
		t.Fatal(err)
	}
	if err := gate.AddPeer(2, nearB); err != nil {
		t.Fatal(err)
	}

	const turn = uint64(5)
	host := [][]byte{gateRecord(t, 10, HostPlayer, 0)}
	recA := [][]byte{gateRecord(t, 10, 1, 0)}
	recB := [][]byte{gateRecord(t, 10, 2, 0)}

	// BEFORE: both peers deliver within grace.
	sendPeerTurn(t, farA, recA)
	sendPeerTurn(t, farB, recB)

	step, err := gate.Step(turn, host, 0)
	if err != nil {
		t.Fatalf("Step: %v", err)
	}
	// SoT 1: status + roster + no drops.
	if step.Status != GateAggregated {
		t.Fatalf("status=%v, want aggregated", step.Status)
	}
	if !bytes.Equal(asBytes(step.Roster), []byte{0, 1, 2}) || len(step.Dropped) != 0 {
		t.Fatalf("roster=%v dropped=%v, want [0 1 2] / none", step.Roster, step.Dropped)
	}
	// SoT 2: payload byte-identical to the reference aggregate over all three.
	want := refAggregate(t, 2, turn, map[uint8][][]byte{0: host, 1: recA, 2: recB})
	if !bytes.Equal(step.Payload, want) {
		t.Fatalf("payload mismatch:\n got %x\nwant %x", step.Payload, want)
	}
	t.Logf("FSV happy: status=aggregated roster=%v payload=%d B (byte-exact vs reference)", step.Roster, len(step.Payload))
}

func TestRoundGateStallGraceDropFSV(t *testing.T) {
	ctrl, err := NewStallController(30 * time.Second)
	if err != nil {
		t.Fatal(err)
	}
	gate, err := NewRoundGate(2, ctrl, 150*time.Millisecond)
	if err != nil {
		t.Fatal(err)
	}
	defer gate.Close()

	nearA, farA := pipePair()
	nearB, _ := pipePair() // B never sends — the laggard
	if err := gate.AddPeer(1, nearA); err != nil {
		t.Fatal(err)
	}
	if err := gate.AddPeer(2, nearB); err != nil {
		t.Fatal(err)
	}

	const turn = uint64(5)
	host := [][]byte{gateRecord(t, 10, HostPlayer, 0)}
	recA := [][]byte{gateRecord(t, 10, 1, 0)}
	sendPeerTurn(t, farA, recA)

	// Step 1 @ t=0: A in, B lagging. SoT: GateWaiting names B, full 30 s grace.
	s1, err := gate.Step(turn, host, 0)
	if err != nil {
		t.Fatalf("step1: %v", err)
	}
	if s1.Status != GateWaiting || !bytes.Equal(asBytes(s1.Waiting), []byte{2}) {
		t.Fatalf("step1 status=%v waiting=%v, want waiting [2]", s1.Status, s1.Waiting)
	}
	if s1.Remaining != 30*time.Second {
		t.Fatalf("step1 remaining=%v, want 30s", s1.Remaining)
	}
	t.Logf("FSV stall t=0: status=waiting waiting=%v remaining=%v", s1.Waiting, s1.Remaining)

	// Step 2 @ t=10s: still lagging, grace counts down.
	s2, err := gate.Step(turn, host, 10*time.Second)
	if err != nil {
		t.Fatalf("step2: %v", err)
	}
	if s2.Status != GateWaiting || s2.Remaining != 20*time.Second {
		t.Fatalf("step2 status=%v remaining=%v, want waiting 20s", s2.Status, s2.Remaining)
	}
	t.Logf("FSV stall t=10s: status=waiting remaining=%v (countdown)", s2.Remaining)

	// Step 3 @ t=35s: grace elapsed => drop B, aggregate host+A only.
	s3, err := gate.Step(turn, host, 35*time.Second)
	if err != nil {
		t.Fatalf("step3: %v", err)
	}
	if s3.Status != GateAggregated || !bytes.Equal(asBytes(s3.Dropped), []byte{2}) {
		t.Fatalf("step3 status=%v dropped=%v, want aggregated dropped [2]", s3.Status, s3.Dropped)
	}
	if !bytes.Equal(asBytes(s3.Roster), []byte{0, 1}) {
		t.Fatalf("step3 roster=%v, want [0 1]", s3.Roster)
	}
	// SoT: payload == reference aggregate over the SURVIVORS only (host + A).
	want := refAggregate(t, 2, turn, map[uint8][][]byte{0: host, 1: recA})
	if !bytes.Equal(s3.Payload, want) {
		t.Fatalf("survivor payload mismatch:\n got %x\nwant %x", s3.Payload, want)
	}
	// B's pump is gone.
	if !bytes.Equal(asBytes(gate.Peers()), []byte{1}) {
		t.Fatalf("after drop Peers()=%v, want [1]", gate.Peers())
	}
	t.Logf("FSV stall t=35s: status=aggregated dropped=[2] roster=[0 1] survivor-payload byte-exact")
}

// Edge: a peer whose stream CLEANLY closes is a departure, not a stall — dropped
// immediately on the first Step, no GateWaiting overlay.
func TestRoundGateCleanDepartureNotStallFSV(t *testing.T) {
	ctrl, err := NewStallController(30 * time.Second)
	if err != nil {
		t.Fatal(err)
	}
	gate, err := NewRoundGate(2, ctrl, 500*time.Millisecond)
	if err != nil {
		t.Fatal(err)
	}
	defer gate.Close()

	nearA, farA := pipePair()
	nearB, farB := pipePair()
	if err := gate.AddPeer(1, nearA); err != nil {
		t.Fatal(err)
	}
	if err := gate.AddPeer(2, nearB); err != nil {
		t.Fatal(err)
	}

	const turn = uint64(7)
	host := [][]byte{gateRecord(t, 12, HostPlayer, 0)}
	recA := [][]byte{gateRecord(t, 12, 1, 0)}
	sendPeerTurn(t, farA, recA)
	_ = farB.Close() // B departs cleanly before sending

	step, err := gate.Step(turn, host, 0)
	if err != nil {
		t.Fatalf("Step: %v", err)
	}
	// SoT: aggregated on the FIRST step (no grace wait), B dropped as a departure.
	if step.Status != GateAggregated {
		t.Fatalf("status=%v, want aggregated (clean departure must not stall)", step.Status)
	}
	if !bytes.Equal(asBytes(step.Dropped), []byte{2}) || !bytes.Equal(asBytes(step.Roster), []byte{0, 1}) {
		t.Fatalf("dropped=%v roster=%v, want [2] / [0 1]", step.Dropped, step.Roster)
	}
	want := refAggregate(t, 2, turn, map[uint8][][]byte{0: host, 1: recA})
	if !bytes.Equal(step.Payload, want) {
		t.Fatalf("payload mismatch after departure:\n got %x\nwant %x", step.Payload, want)
	}
	t.Logf("FSV clean-departure: status=aggregated (first step) dropped=[2] roster=[0 1], no stall overlay")
}

func asBytes(ids []uint8) []byte { return append([]byte(nil), ids...) }

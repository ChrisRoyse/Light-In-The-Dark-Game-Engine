package net

// #65 FSV: the command-turn pipeline. SoT = the aggregate turn bytes each client
// receives — identical across clients and sorted by (tick, playerID, seq).
// Records are minted with the REAL sim encoder (litd/sim — no import cycle, sim
// never imports net) so the test knows the exact keys it fed and checks they come
// out sorted (X+X=Y discipline). Edges: empty heartbeat contribution, late
// submission rejected, player spoof rejected, multi-client byte-identity.

import (
	"bytes"
	"context"
	"encoding/hex"
	"testing"
	"time"

	sim "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
)

// mkRecord encodes a real, minimal (OpStop, no units) CommandRecord with the
// given header key, via the production sim encoder.
func mkRecord(t *testing.T, tick uint32, player uint8, seq uint16) []byte {
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
		t.Fatalf("sim.AppendEncode failed for tick=%d player=%d seq=%d", tick, player, seq)
	}
	// Cross-check: the pipeline's header parser agrees with the real encoder.
	gt, gp, gs, ok := recordKey(b)
	if !ok || gt != tick || gp != player || gs != seq {
		t.Fatalf("recordKey mismatch vs sim encoder: got (%d,%d,%d) ok=%v want (%d,%d,%d)", gt, gp, gs, ok, tick, player, seq)
	}
	return b
}

type key struct {
	tick   uint32
	player uint8
	seq    uint16
}

func keysOf(t *testing.T, payload []byte) []key {
	t.Helper()
	recs, err := DecodeTurn(payload)
	if err != nil {
		t.Fatalf("DecodeTurn: %v", err)
	}
	ks := make([]key, len(recs))
	for i, r := range recs {
		tk, p, s, ok := recordKey(r)
		if !ok {
			t.Fatalf("record %d malformed in aggregate", i)
		}
		ks[i] = key{tk, p, s}
	}
	return ks
}

func TestTurnCodecRoundTrip(t *testing.T) {
	recs := [][]byte{
		mkRecord(t, 10, 0, 1),
		mkRecord(t, 11, 2, 3),
	}
	payload, err := EncodeTurn(recs)
	if err != nil {
		t.Fatalf("EncodeTurn: %v", err)
	}
	back, err := DecodeTurn(payload)
	if err != nil {
		t.Fatalf("DecodeTurn: %v", err)
	}
	if len(back) != len(recs) {
		t.Fatalf("round-trip count %d != %d", len(back), len(recs))
	}
	for i := range recs {
		if !bytes.Equal(back[i], recs[i]) {
			t.Fatalf("record %d not byte-identical after round-trip", i)
		}
	}
	// Truncation is refused (fail-closed).
	if _, err := DecodeTurn(payload[:len(payload)-3]); err == nil {
		t.Fatal("DecodeTurn accepted a truncated payload")
	}
	t.Logf("FSV codec: %d records round-trip byte-identical; truncation refused", len(recs))
}

// TestTurnAggregateSortedDeterministic — 3 players submit interleaved records;
// the aggregate is (tick,player,seq)-sorted and identical regardless of the
// order players submitted (the lockstep guarantee).
func TestTurnAggregateSortedDeterministic(t *testing.T) {
	players := []uint8{0, 1, 2}

	// Player 0: two records on ticks 0 and 2. Player 1: tick 1, two seqs.
	// Player 2: tick 0. Deliberately out of global order.
	p0 := [][]byte{mkRecord(t, 0, 0, 0), mkRecord(t, 2, 0, 0)}
	p1 := [][]byte{mkRecord(t, 1, 1, 1), mkRecord(t, 1, 1, 0)}
	p2 := [][]byte{mkRecord(t, 0, 2, 0)}

	// Expected global order by (tick,player,seq):
	want := []key{
		{0, 0, 0}, // tick 0, player 0
		{0, 2, 0}, // tick 0, player 2
		{1, 1, 0}, // tick 1, player 1, seq 0
		{1, 1, 1}, // tick 1, player 1, seq 1
		{2, 0, 0}, // tick 2, player 0
	}

	// Submit in order 0,1,2.
	bufA, _ := NewTurnBuffer(2, players)
	mustSubmit(t, bufA, 0, 0, p0)
	mustSubmit(t, bufA, 0, 1, p1)
	mustSubmit(t, bufA, 0, 2, p2)
	if !bufA.Ready(0) {
		t.Fatal("bufA turn 0 should be ready")
	}
	aggA, err := bufA.Aggregate(0)
	if err != nil {
		t.Fatalf("Aggregate A: %v", err)
	}

	// Submit in order 2,0,1 — must yield identical bytes.
	bufB, _ := NewTurnBuffer(2, players)
	mustSubmit(t, bufB, 0, 2, p2)
	mustSubmit(t, bufB, 0, 0, p0)
	mustSubmit(t, bufB, 0, 1, p1)
	aggB, err := bufB.Aggregate(0)
	if err != nil {
		t.Fatalf("Aggregate B: %v", err)
	}

	if !bytes.Equal(aggA, aggB) {
		t.Fatalf("aggregate not submit-order-independent:\n A=%s\n B=%s", hex.EncodeToString(aggA), hex.EncodeToString(aggB))
	}
	got := keysOf(t, aggA)
	if len(got) != len(want) {
		t.Fatalf("aggregate has %d records, want %d", len(got), len(want))
	}
	for i := range want {
		if got[i] != want[i] {
			t.Fatalf("order[%d] = %+v, want %+v\n full: %+v", i, got[i], want[i], got)
		}
	}
	t.Logf("FSV aggregate sorted + order-independent: %+v\n  hex=%s", got, hex.EncodeToString(aggA))
}

// TestTurnEmptyContributionHeartbeat — a player with zero commands still submits
// an empty contribution and the turn aggregates/broadcasts on time.
func TestTurnEmptyContributionHeartbeat(t *testing.T) {
	players := []uint8{0, 1, 2}
	buf, _ := NewTurnBuffer(3, players)
	mustSubmit(t, buf, 5, 0, [][]byte{mkRecord(t, 10, 0, 0)})
	mustSubmit(t, buf, 5, 1, nil) // empty heartbeat
	if buf.Ready(5) {
		t.Fatal("turn 5 ready with only 2/3 players")
	}
	mustSubmit(t, buf, 5, 2, [][]byte{}) // empty heartbeat
	if !buf.Ready(5) {
		t.Fatalf("turn 5 not ready after all 3 submitted (count=%d)", buf.SubmittedCount(5))
	}
	agg, err := buf.Aggregate(5)
	if err != nil {
		t.Fatalf("Aggregate: %v", err)
	}
	got := keysOf(t, agg)
	if len(got) != 1 || got[0] != (key{10, 0, 0}) {
		t.Fatalf("aggregate composition %+v, want [{10 0 0}] (only player 0 contributed)", got)
	}
	t.Logf("FSV heartbeat: 2 empty + 1 record → turn ready & aggregated with %d record(s)", len(got))
}

// TestTurnLateSubmissionRejected — once a turn is broadcast, a late submission
// for it is rejected (not merged into a later turn).
func TestTurnLateSubmissionRejected(t *testing.T) {
	players := []uint8{0, 1}
	buf, _ := NewTurnBuffer(2, players)
	mustSubmit(t, buf, 0, 0, [][]byte{mkRecord(t, 0, 0, 0)})
	mustSubmit(t, buf, 0, 1, [][]byte{mkRecord(t, 0, 1, 0)})
	t.Logf("FSV before broadcast: turn 0 submitted=%d/%d", buf.SubmittedCount(0), len(players))
	if _, err := buf.Aggregate(0); err != nil {
		t.Fatalf("Aggregate: %v", err)
	}
	// AFTER broadcast: a late submission for turn 0 must be refused.
	err := buf.Submit(0, 0, [][]byte{mkRecord(t, 0, 0, 1)})
	if err == nil {
		t.Fatal("late submission for already-broadcast turn 0 was accepted")
	}
	t.Logf("FSV after broadcast: late submit rejected → %v", err)
	// And re-aggregating the same turn is refused too.
	if _, err := buf.Aggregate(0); err == nil {
		t.Fatal("re-aggregating a broadcast turn was accepted")
	}
}

// TestTurnSpoofRejected — a player cannot submit a record whose header claims a
// different player id.
func TestTurnSpoofRejected(t *testing.T) {
	buf, _ := NewTurnBuffer(2, []uint8{0, 1})
	// Player 1 tries to submit a record stamped player 0.
	err := buf.Submit(0, 1, [][]byte{mkRecord(t, 0, 0, 0)})
	if err == nil {
		t.Fatal("spoofed player-id record was accepted")
	}
	t.Logf("FSV spoof rejected: %v", err)
}

// TestTurnBroadcastIdenticalAcrossClients — the aggregate is delivered
// byte-identically to 3 real QUIC clients.
func TestTurnBroadcastIdenticalAcrossClients(t *testing.T) {
	const n = 3
	clients, hosts, cleanup := nSessionPairs(t, n)
	defer cleanup()

	players := []uint8{0, 1, 2}
	buf, _ := NewTurnBuffer(2, players)
	mustSubmit(t, buf, 0, 0, [][]byte{mkRecord(t, 0, 0, 0)})
	mustSubmit(t, buf, 0, 1, [][]byte{mkRecord(t, 1, 1, 0)})
	mustSubmit(t, buf, 0, 2, nil)
	agg, err := buf.Aggregate(0)
	if err != nil {
		t.Fatalf("Aggregate: %v", err)
	}

	if err := Broadcast(agg, hosts); err != nil {
		t.Fatalf("Broadcast: %v", err)
	}
	for i, c := range clients {
		got, err := c.RecvTurn()
		if err != nil {
			t.Fatalf("client %d RecvTurn: %v", i, err)
		}
		if !bytes.Equal(got, agg) {
			t.Fatalf("client %d received %s, want %s", i, hex.EncodeToString(got), hex.EncodeToString(agg))
		}
		t.Logf("FSV client %d received identical aggregate (%d B): %s", i, len(got), hex.EncodeToString(got))
	}
}

func mustSubmit(t *testing.T, b *TurnBuffer, turn uint64, player uint8, recs [][]byte) {
	t.Helper()
	if err := b.Submit(turn, player, recs); err != nil {
		t.Fatalf("Submit(turn=%d player=%d): %v", turn, player, err)
	}
}

// nSessionPairs sets up n authenticated client↔host session pairs on one listener.
func nSessionPairs(t *testing.T, n int) (clients, hosts []*Session, cleanup func()) {
	t.Helper()
	serverTLS, clientTLS, err := SelfSignedTLS()
	if err != nil {
		t.Fatalf("SelfSignedTLS: %v", err)
	}
	ln, err := Listen("127.0.0.1:0", serverTLS)
	if err != nil {
		t.Fatalf("Listen: %v", err)
	}
	tok, _ := NewSessionToken()
	ctx, cancel := context.WithTimeout(context.Background(), 15*time.Second)

	hostCh := make(chan *Session, n)
	errc := make(chan error, n)
	go func() {
		for i := 0; i < n; i++ {
			s, err := ln.AcceptAuthenticated(ctx, tok)
			if err != nil {
				errc <- err
				return
			}
			hostCh <- s
		}
	}()
	for i := 0; i < n; i++ {
		c, err := DialAuthenticated(ctx, ln.Addr(), tok, clientTLS)
		if err != nil {
			cancel()
			ln.Close()
			t.Fatalf("dial %d: %v", i, err)
		}
		clients = append(clients, c)
	}
	for i := 0; i < n; i++ {
		select {
		case s := <-hostCh:
			hosts = append(hosts, s)
		case e := <-errc:
			cancel()
			ln.Close()
			t.Fatalf("accept: %v", e)
		}
	}
	cleanup = func() {
		for _, c := range clients {
			c.Close()
		}
		for _, h := range hosts {
			h.Close()
		}
		ln.Close()
		cancel()
	}
	return clients, hosts, cleanup
}

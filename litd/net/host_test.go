package net

// #62 FSV: in-process LAN host loop. SoT = the host event log + Roster()
// transitions over a REAL loopback QUIC listener with in-process clients. No mock
// transport. Edge cases: full-session refusal, mid-session client departure, and
// host-only (0-remote) local play.

import (
	"context"
	"testing"
	"time"

	sim "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
)

const (
	hostBuild = "litd-0.1.0+abc"
	hostSeed  = uint64(42)
)

// hostListener stands up a loopback QUIC listener + an accept goroutine that
// admits every connection into h.
func hostListener(t *testing.T, h *Host) *Listener {
	t.Helper()
	serverTLS, _, err := SelfSignedTLS()
	if err != nil {
		t.Fatalf("SelfSignedTLS: %v", err)
	}
	ln, err := Listen("127.0.0.1:0", serverTLS)
	if err != nil {
		t.Fatalf("Listen: %v", err)
	}
	ctx, cancel := context.WithCancel(context.Background())
	t.Cleanup(func() { cancel(); ln.Close() })
	go func() {
		for {
			s, aerr := ln.Accept(ctx)
			if aerr != nil {
				return
			}
			if _, derr := h.Admit(s, s.RemoteAddr()); derr != nil {
				// Refused: the verdict was written to the client. Don't abruptly
				// tear the conn down (that would race the client's read of the
				// reason) — drain until the client reads the verdict and hangs
				// up, then reap. Models a host that reaps refused conns on idle,
				// not instantly.
				go func(s *Session) {
					_, _ = s.RecvFrame()
					_ = s.Close()
				}(s)
			}
		}
	}()
	return ln
}

// joinClient dials the listener and runs the join handshake with the given
// build/seed. Returns the session and the join error (nil on accept).
func joinClient(t *testing.T, addr string, build string, seed uint64) (*Session, error) {
	t.Helper()
	_, clientTLS, err := SelfSignedTLS()
	if err != nil {
		t.Fatalf("SelfSignedTLS: %v", err)
	}
	ctx, cancel := context.WithTimeout(context.Background(), 8*time.Second)
	defer cancel()
	s, err := Dial(ctx, addr, clientTLS)
	if err != nil {
		return nil, err
	}
	if jerr := s.ClientJoin(build, seed); jerr != nil {
		return s, jerr
	}
	return s, nil
}

func waitPlayerCount(t *testing.T, h *Host, want int) {
	t.Helper()
	deadline := time.Now().Add(5 * time.Second)
	for time.Now().Before(deadline) {
		if h.PlayerCount() == want {
			return
		}
		time.Sleep(5 * time.Millisecond)
	}
	t.Fatalf("player count never reached %d (stuck at %d)", want, h.PlayerCount())
}

func playerRec(t *testing.T, tick uint32, player uint8, seq uint16) []byte {
	t.Helper()
	r := sim.CommandRecord{Version: sim.CommandVersion, Tick: tick, Player: player, Seq: seq, Opcode: sim.OpStop, UnitCount: 1}
	r.Units[0] = sim.EntityID(1)
	b, ok := sim.AppendEncode(nil, &r)
	if !ok {
		t.Fatalf("encode rec player=%d", player)
	}
	return b
}

func eq(a, b []uint8) bool {
	if len(a) != len(b) {
		return false
	}
	for i := range a {
		if a[i] != b[i] {
			return false
		}
	}
	return true
}

// TestHostFullSessionRefused — a capacity-4 host admits 3 remotes (4 players),
// then refuses the 4th with a distinguished reason; the roster is unchanged.
func TestHostFullSessionRefused(t *testing.T) {
	h, err := NewHost(HostOptions{BuildHash: hostBuild, Seed: hostSeed, Capacity: 4, TurnLen: 2})
	if err != nil {
		t.Fatalf("NewHost: %v", err)
	}
	ln := hostListener(t, h)

	var sessions []*Session
	for i := 1; i <= 3; i++ {
		s, jerr := joinClient(t, ln.Addr(), hostBuild, hostSeed)
		if jerr != nil {
			t.Fatalf("client %d join: %v", i, jerr)
		}
		sessions = append(sessions, s)
		waitPlayerCount(t, h, i+1) // host + i clients committed
	}
	before := h.Roster()
	if !eq(before, []uint8{0, 1, 2, 3}) {
		t.Fatalf("roster before = %v, want [0 1 2 3]", before)
	}
	t.Logf("FSV roster full: %v (%d/4 players)", before, h.PlayerCount())

	// 4th remote → 5th player → refused.
	s4, jerr := joinClient(t, ln.Addr(), hostBuild, hostSeed)
	if s4 != nil {
		defer s4.Close()
	}
	if jerr == nil {
		t.Fatal("8th-player-style join into a full session was accepted")
	}
	if got := jerr.Error(); !contains(got, "session-full") {
		t.Fatalf("refusal reason = %q, want it to name session-full", got)
	}
	after := h.Roster()
	if !eq(after, before) {
		t.Fatalf("roster changed after refusal: %v → %v", before, after)
	}
	t.Logf("FSV refusal: 4th remote refused [%v]; roster stays %v", jerr, after)
	for _, e := range h.Events() {
		t.Logf("  event: %s", e)
	}
	for _, s := range sessions {
		s.Close()
	}
}

// TestHostClientDeparture — a client disconnecting mid-session shrinks the roster
// and the loop continues aggregating for the rest.
func TestHostClientDeparture(t *testing.T) {
	h, err := NewHost(HostOptions{BuildHash: hostBuild, Seed: hostSeed, Capacity: 4, TurnLen: 2})
	if err != nil {
		t.Fatalf("NewHost: %v", err)
	}
	ln := hostListener(t, h)

	clients := map[uint8]*Session{}
	for i := 1; i <= 3; i++ {
		s, jerr := joinClient(t, ln.Addr(), hostBuild, hostSeed)
		if jerr != nil {
			t.Fatalf("client %d join: %v", i, jerr)
		}
		waitPlayerCount(t, h, i+1)
		clients[uint8(i)] = s
	}
	t.Logf("FSV before departure: roster %v", h.Roster())

	// Round 0 — all four players present, everyone submits a heartbeat.
	for _, s := range clients {
		if err := s.SendTurn(mustEncodeTurn(t, nil)); err != nil {
			t.Fatalf("client SendTurn r0: %v", err)
		}
	}
	_, roster0, err := h.CollectRound(0, nil)
	if err != nil {
		t.Fatalf("CollectRound 0: %v", err)
	}
	if !eq(roster0, []uint8{0, 1, 2, 3}) {
		t.Fatalf("round-0 roster %v, want [0 1 2 3]", roster0)
	}

	// Client 2 vanishes; the others submit round 1.
	clients[2].Close()
	delete(clients, 2)
	for _, s := range clients {
		if err := s.SendTurn(mustEncodeTurn(t, nil)); err != nil {
			t.Fatalf("client SendTurn r1: %v", err)
		}
	}
	payload, roster1, err := h.CollectRound(1, nil)
	if err != nil {
		t.Fatalf("CollectRound 1 after departure: %v", err)
	}
	if !eq(roster1, []uint8{0, 1, 3}) {
		t.Fatalf("round-1 roster %v, want [0 1 3] (player 2 departed)", roster1)
	}
	if payload == nil {
		t.Fatal("round 1 produced no aggregate after departure")
	}
	t.Logf("FSV departure: roster shrank to %v; round 1 still aggregated %d B", roster1, len(payload))
	for _, e := range h.Events() {
		t.Logf("  event: %s", e)
	}
	for _, s := range clients {
		s.Close()
	}
}

// TestHostOnlyLocalPlay — a 0-remote host loop aggregates the host player's own
// turns for local play (no network round-trip).
func TestHostOnlyLocalPlay(t *testing.T) {
	h, err := NewHost(HostOptions{BuildHash: hostBuild, Seed: hostSeed, Capacity: 2, TurnLen: 2})
	if err != nil {
		t.Fatalf("NewHost: %v", err)
	}
	if h.PlayerCount() != 1 {
		t.Fatalf("host-only player count = %d, want 1", h.PlayerCount())
	}
	rec := playerRec(t, 1, HostPlayer, 0)
	payload, roster, err := h.CollectRound(0, [][]byte{rec})
	if err != nil {
		t.Fatalf("host-only CollectRound: %v", err)
	}
	if !eq(roster, []uint8{0}) {
		t.Fatalf("host-only roster %v, want [0]", roster)
	}
	out, err := DecodeTurn(payload)
	if err != nil {
		t.Fatalf("DecodeTurn: %v", err)
	}
	if len(out) != 1 {
		t.Fatalf("host-only aggregate has %d records, want 1 (the host's own turn)", len(out))
	}
	_, p, _, ok := recordKey(out[0])
	if !ok || p != HostPlayer {
		t.Fatalf("aggregated record player=%d ok=%v, want host player %d", p, ok, HostPlayer)
	}
	t.Logf("FSV host-only: 0 remotes, host turn aggregated locally → 1 record, player %d; roster %v", p, roster)
}

func mustEncodeTurn(t *testing.T, recs [][]byte) []byte {
	t.Helper()
	b, err := EncodeTurn(recs)
	if err != nil {
		t.Fatalf("EncodeTurn: %v", err)
	}
	return b
}

func contains(s, sub string) bool {
	for i := 0; i+len(sub) <= len(s); i++ {
		if s[i:i+len(sub)] == sub {
			return true
		}
	}
	return false
}

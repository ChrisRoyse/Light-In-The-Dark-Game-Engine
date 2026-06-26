package net

// #74 FSV: the build-hash + seed join guard. SoT = the join-handshake transcript
// on host and client — accept, build-mismatch (both hashes shown), seed-mismatch
// (both seeds shown), malformed (host loop survives). Real loopback QUIC, run
// after PSK auth, before any turn data. No mock.

import (
	"bytes"
	"context"
	"crypto/tls"
	"encoding/binary"
	"strings"
	"testing"
	"time"
)

const (
	testBuild = "litd-0.1.0+deadbeef"
	testSeed  = uint64(0x0BADC0DE)
)

// joinHarness brings up an authenticated loopback listener reusable across
// multiple connects (so the malformed test can prove the host survives).
func joinHarness(t *testing.T) (ln *Listener, token []byte, clientTLS *tls.Config) {
	t.Helper()
	serverTLS, ctls, err := SelfSignedTLS()
	if err != nil {
		t.Fatalf("SelfSignedTLS: %v", err)
	}
	l, err := Listen("127.0.0.1:0", serverTLS)
	if err != nil {
		t.Fatalf("Listen: %v", err)
	}
	tok, err := NewSessionToken()
	if err != nil {
		t.Fatalf("token: %v", err)
	}
	t.Cleanup(func() { l.Close() })
	return l, tok, ctls
}

// connect makes one authenticated client↔host Session pair on ln.
func connect(t *testing.T, ln *Listener, token []byte, clientTLS *tls.Config) (client, host *Session) {
	t.Helper()
	ctx, cancel := context.WithTimeout(context.Background(), 8*time.Second)
	t.Cleanup(cancel)
	ch := make(chan *Session, 1)
	errc := make(chan error, 1)
	go func() {
		s, err := ln.AcceptAuthenticated(ctx, token)
		if err != nil {
			errc <- err
			return
		}
		ch <- s
	}()
	c, err := DialAuthenticated(ctx, ln.Addr(), token, clientTLS)
	if err != nil {
		t.Fatalf("DialAuthenticated: %v", err)
	}
	select {
	case host = <-ch:
	case e := <-errc:
		t.Fatalf("AcceptAuthenticated: %v", e)
	}
	t.Cleanup(func() { c.Close(); host.Close() })
	return c, host
}

// runJoin runs HostJoinGuard (host build/seed) concurrently with ClientJoin
// (client build/seed) and returns both errors.
func runJoin(c, h *Session, hostBuild string, hostSeed uint64, cliBuild string, cliSeed uint64) (clientErr, hostErr error) {
	done := make(chan struct{})
	go func() {
		hostErr = h.HostJoinGuard(hostBuild, hostSeed)
		close(done)
	}()
	clientErr = c.ClientJoin(cliBuild, cliSeed)
	<-done
	return clientErr, hostErr
}

func TestJoinGuardAccept(t *testing.T) {
	ln, tok, ctls := joinHarness(t)
	c, h := connect(t, ln, tok, ctls)

	cerr, herr := runJoin(c, h, testBuild, testSeed, testBuild, testSeed)
	if cerr != nil || herr != nil {
		t.Fatalf("matching join should accept; client=%v host=%v", cerr, herr)
	}
	t.Logf("FSV accept: client OK, host OK (build=%q seed=%#x)", testBuild, testSeed)

	// Join precedes turns: the channel now carries a turn.
	turn := []byte{0xAB, 0xCD}
	if err := c.SendTurn(turn); err != nil {
		t.Fatalf("post-join SendTurn: %v", err)
	}
	got, err := h.RecvTurn()
	if err != nil || string(got) != string(turn) {
		t.Fatalf("post-join recv: got=%x err=%v want %x", got, err, turn)
	}
	t.Logf("FSV accept: post-join turn %x delivered", got)
}

func TestJoinGuardBuildMismatch(t *testing.T) {
	ln, tok, ctls := joinHarness(t)
	c, h := connect(t, ln, tok, ctls)

	const cliBuild = "litd-0.2.0+cafe"
	cerr, herr := runJoin(c, h, testBuild, testSeed, cliBuild, testSeed)
	if cerr == nil || herr == nil {
		t.Fatalf("build mismatch must refuse; client=%v host=%v", cerr, herr)
	}
	// Reason distinguishes build-mismatch and shows BOTH hashes verbatim.
	for _, want := range []string{"build-mismatch", testBuild, cliBuild} {
		if !strings.Contains(cerr.Error(), want) {
			t.Fatalf("client refusal %q missing %q", cerr, want)
		}
	}
	if !strings.Contains(herr.Error(), "build-mismatch") {
		t.Fatalf("host err %q missing build-mismatch", herr)
	}
	t.Logf("FSV build-mismatch:\n  client sees: %v\n  host  sees: %v", cerr, herr)
}

func TestJoinGuardSeedMismatch(t *testing.T) {
	ln, tok, ctls := joinHarness(t)
	c, h := connect(t, ln, tok, ctls)

	const hostSeed = uint64(1111)
	const cliSeed = uint64(2222)
	cerr, herr := runJoin(c, h, testBuild, hostSeed, testBuild, cliSeed)
	if cerr == nil || herr == nil {
		t.Fatalf("seed mismatch must refuse; client=%v host=%v", cerr, herr)
	}
	for _, want := range []string{"seed-mismatch", "1111", "2222"} {
		if !strings.Contains(cerr.Error(), want) {
			t.Fatalf("client refusal %q missing %q", cerr, want)
		}
	}
	t.Logf("FSV seed-mismatch:\n  client sees: %v\n  host  sees: %v", cerr, herr)
}

// TestJoinGuardMalformedSurvives — a truncated/garbage join request is refused
// safely and the host listener keeps serving other peers (SoT: host accepts a
// fresh valid join afterward).
func TestJoinGuardMalformedSurvives(t *testing.T) {
	ln, tok, ctls := joinHarness(t)
	c, h := connect(t, ln, tok, ctls)

	// Host state BEFORE: listener live.
	t.Logf("FSV before malformed: listener live at %s", ln.Addr())

	done := make(chan error, 1)
	go func() { done <- h.HostJoinGuard(testBuild, testSeed) }()
	// Client sends a length prefix claiming 65535 build bytes (> 128 cap) then
	// closes — never a valid request.
	if _, err := c.stream.Write([]byte{0xFF, 0xFF}); err != nil {
		t.Fatalf("write garbage: %v", err)
	}
	c.Close()
	herr := <-done
	if herr == nil {
		t.Fatal("malformed join request was not refused")
	}
	t.Logf("FSV malformed: host refused safely: %v", herr)

	// Host state AFTER: a fresh, valid join still works on the same listener.
	c2, h2 := connect(t, ln, tok, ctls)
	cerr, herr2 := runJoin(c2, h2, testBuild, testSeed, testBuild, testSeed)
	if cerr != nil || herr2 != nil {
		t.Fatalf("host did not survive malformed join; fresh join client=%v host=%v", cerr, herr2)
	}
	t.Logf("FSV after malformed: fresh valid join ACCEPTED — host loop unaffected")
}

// TestJoinCodecLenCapsRefused — deterministic (no network) proof that the
// fail-closed alloc bounds reject an over-long build hash / response without
// allocating it. SoT = the codec error, not a giant buffer.
func TestJoinCodecLenCapsRefused(t *testing.T) {
	// Build-hash length 65535 (> maxBuildWire 128) → refused at the length check.
	var over [2]byte
	binary.BigEndian.PutUint16(over[:], 0xFFFF)
	if _, err := readJoinRequest(bytes.NewReader(over[:])); err == nil ||
		!strings.Contains(err.Error(), "exceeds") {
		t.Fatalf("oversized build length not refused: %v", err)
	} else {
		t.Logf("FSV codec: build-hash len 65535 refused → %v", err)
	}
	// Response message length over maxJoinMsgWire → refused.
	msgHdr := []byte{joinBuildMismatch, 0xFF, 0xFF}
	if _, _, err := readJoinResponse(bytes.NewReader(msgHdr)); err == nil ||
		!strings.Contains(err.Error(), "exceeds") {
		t.Fatalf("oversized response length not refused: %v", err)
	} else {
		t.Logf("FSV codec: response msg len 65535 refused → %v", err)
	}
	// A well-formed request round-trips through the codec exactly.
	var buf bytes.Buffer
	if err := writeJoinRequest(&buf, testBuild, testSeed); err != nil {
		t.Fatalf("writeJoinRequest: %v", err)
	}
	got, err := readJoinRequest(&buf)
	if err != nil || got.build != testBuild || got.seed != testSeed {
		t.Fatalf("codec round-trip: got=%+v err=%v want build=%q seed=%#x", got, err, testBuild, testSeed)
	}
	t.Logf("FSV codec round-trip: build=%q seed=%#x intact", got.build, got.seed)
}

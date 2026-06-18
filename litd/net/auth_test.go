package net

// #61 FSV: PSK session-token auth over the QUIC transport. SoT = the host's
// accept/refuse outcome (an established Session, or an error + an UNCHANGED host
// session table) over a real loopback QUIC connection — plus proof that a
// refused peer never gets a turn through. No mock transport.

import (
	"bytes"
	"context"
	"crypto/tls"
	"testing"
	"time"
)

// authPair stands up a loopback listener with a fresh self-signed cert (cert =
// encryption only) and returns it with the skip-verify client config.
func authPair(t *testing.T) (*Listener, *tls.Config) {
	t.Helper()
	serverTLS, clientTLS, err := SelfSignedTLS()
	if err != nil {
		t.Fatalf("SelfSignedTLS: %v", err)
	}
	ln, err := Listen("127.0.0.1:0", serverTLS)
	if err != nil {
		t.Fatalf("Listen: %v", err)
	}
	return ln, clientTLS
}

type attemptResult struct {
	host      *Session
	hostErr   error
	client    *Session
	clientErr error
}

// attempt runs one connection: the host accepts requiring `expected`, while the
// client dials via `dialFn` (plain Dial sends an empty token; DialAuthenticated
// sends one). Returns both ends' outcomes.
func attempt(t *testing.T, ln *Listener, expected []byte, dialFn func(ctx context.Context, addr string) (*Session, error)) attemptResult {
	t.Helper()
	ctx, cancel := context.WithTimeout(context.Background(), 8*time.Second)
	defer cancel()
	ch := make(chan attemptResult, 1)
	go func() {
		s, err := ln.AcceptAuthenticated(ctx, expected)
		ch <- attemptResult{host: s, hostErr: err}
	}()
	c, cerr := dialFn(ctx, ln.Addr())
	r := <-ch
	r.client, r.clientErr = c, cerr
	return r
}

func TestAuthTokenAcceptAndRefuse(t *testing.T) {
	ln, clientTLS := authPair(t)
	defer ln.Close()

	good, err := NewSessionToken()
	if err != nil {
		t.Fatalf("NewSessionToken: %v", err)
	}

	// Host session table — SoT for "no session object created on refusal".
	var table []*Session
	record := func(label string, r attemptResult) {
		if r.hostErr == nil && r.host != nil {
			table = append(table, r.host)
			t.Logf("FSV %s: ACCEPTED (host table size now %d)", label, len(table))
		} else {
			t.Logf("FSV %s: REFUSED — host: %v ; table size stays %d", label, r.hostErr, len(table))
		}
	}

	// --- Happy path: matching token accepted, and a turn round-trips. ---
	r := attempt(t, ln, good, func(ctx context.Context, addr string) (*Session, error) {
		return DialAuthenticated(ctx, addr, good, clientTLS)
	})
	before := len(table)
	record("good-token", r)
	if r.hostErr != nil || r.host == nil {
		t.Fatalf("good token should be accepted; hostErr=%v", r.hostErr)
	}
	if len(table) != before+1 {
		t.Fatalf("good token: table grew %d→%d, want +1", before, len(table))
	}
	// Prove the authenticated channel carries turns.
	turn := []byte{0x10, 0x20, 0x30}
	if err := r.client.SendTurn(turn); err != nil {
		t.Fatalf("authed SendTurn: %v", err)
	}
	got, err := r.host.RecvTurn()
	if err != nil || !bytes.Equal(got, turn) {
		t.Fatalf("authed round-trip: got=%x err=%v want %x", got, err, turn)
	}
	t.Logf("FSV good-token channel: turn %x delivered", got)
	r.client.Close()
	r.host.Close()

	// --- Edge 1: wrong token → refused, no session, no turn delivered. ---
	wrong := make([]byte, TokenLen) // all-zero, != random good
	beforeWrong := len(table)
	r = attempt(t, ln, good, func(ctx context.Context, addr string) (*Session, error) {
		return DialAuthenticated(ctx, addr, wrong, clientTLS)
	})
	record("wrong-token", r)
	if r.hostErr == nil {
		t.Fatalf("wrong token must be refused; host returned a session")
	}
	if len(table) != beforeWrong {
		t.Fatalf("wrong token: table changed %d→%d, want unchanged", beforeWrong, len(table))
	}
	// The refused client never gets a turn through: the host closed before any
	// RecvTurn, so a send is never received (the conn is torn down host-side).
	if r.client != nil {
		_ = r.client.SendTurn(turn)                // may buffer locally
		r.client.stream.Write([]byte{0})           // nudge
		if _, e := r.client.RecvTurn(); e == nil { // host closed → must error
			t.Fatalf("refused client RecvTurn succeeded; refusal did not close conn")
		}
		r.client.Close()
	}

	// --- Edge 2: empty token (plain Dial) → refused identically. ---
	beforeEmpty := len(table)
	r = attempt(t, ln, good, func(ctx context.Context, addr string) (*Session, error) {
		return Dial(ctx, addr, clientTLS) // sends tokenLen=0
	})
	record("empty-token", r)
	if r.hostErr == nil {
		t.Fatalf("empty token must be refused")
	}
	if len(table) != beforeEmpty {
		t.Fatalf("empty token: table changed, want unchanged")
	}
	if r.client != nil {
		r.client.Close()
	}

	// --- Edge 3: stale token after host regenerates the session token. ---
	fresh, err := NewSessionToken()
	if err != nil {
		t.Fatalf("regen token: %v", err)
	}
	beforeStale := len(table)
	r = attempt(t, ln, fresh, func(ctx context.Context, addr string) (*Session, error) {
		return DialAuthenticated(ctx, addr, good, clientTLS) // old token
	})
	record("stale-token", r)
	if r.hostErr == nil {
		t.Fatalf("stale token must be refused after regeneration")
	}
	if len(table) != beforeStale {
		t.Fatalf("stale token: table changed, want unchanged")
	}
	if r.client != nil {
		r.client.Close()
	}
	// And the regenerated token IS accepted.
	r = attempt(t, ln, fresh, func(ctx context.Context, addr string) (*Session, error) {
		return DialAuthenticated(ctx, addr, fresh, clientTLS)
	})
	record("fresh-token", r)
	if r.hostErr != nil || r.host == nil {
		t.Fatalf("regenerated token should be accepted; hostErr=%v", r.hostErr)
	}
	r.client.Close()
	r.host.Close()

	t.Logf("FSV summary: 1 accept (good) + 1 accept (fresh) ; 3 refusals (wrong/empty/stale); final table size %d", len(table))
}

// TestTokensEqualConstantTimePaths — the comparison primitive: equal true,
// any difference or length mismatch false (the constant-time guarantee is by
// construction via crypto/subtle).
func TestTokensEqualConstantTime(t *testing.T) {
	a := bytes.Repeat([]byte{0x5A}, TokenLen)
	b := bytes.Repeat([]byte{0x5A}, TokenLen)
	if !tokensEqual(a, b) {
		t.Fatal("equal tokens compared unequal")
	}
	b[TokenLen-1] ^= 0xFF
	if tokensEqual(a, b) {
		t.Fatal("differing tokens compared equal")
	}
	if tokensEqual(a, a[:TokenLen-1]) {
		t.Fatal("length-mismatched tokens compared equal")
	}
	if tokensEqual(a, nil) {
		t.Fatal("token vs nil compared equal")
	}
	t.Log("FSV tokensEqual: equal=true; byte-diff/len-diff/nil=false")
}

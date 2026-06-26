package net

// #60 FSV: a real loopback QUIC Session carries command turns byte-identically,
// in send order. SoT = the bytes RecvTurn actually returns (hex-dumped and
// compared to what SendTurn was given) over a genuine quic-go connection — no
// mock transport. Edge cases: peer close (error, never a partial turn), ordering
// of back-to-back turns, oversized turn rejected with the session still usable.

import (
	"bytes"
	"context"
	"crypto/ecdsa"
	"crypto/elliptic"
	"crypto/rand"
	"crypto/tls"
	"crypto/x509"
	"crypto/x509/pkix"
	"encoding/hex"
	"math/big"
	"net"
	"testing"
	"time"
)

// selfSignedTLS makes an ephemeral cert and a (server, client) config pair. The
// client PINS the server cert via RootCAs — no InsecureSkipVerify, so this is
// not a fail-open. PSK/identity hardening is #61.
func selfSignedTLS(t *testing.T) (server, client *tls.Config) {
	t.Helper()
	priv, err := ecdsa.GenerateKey(elliptic.P256(), rand.Reader)
	if err != nil {
		t.Fatalf("genkey: %v", err)
	}
	tmpl := x509.Certificate{
		SerialNumber:          big.NewInt(1),
		Subject:               pkix.Name{CommonName: "litd-net-test"},
		NotBefore:             time.Now().Add(-time.Hour),
		NotAfter:              time.Now().Add(time.Hour),
		KeyUsage:              x509.KeyUsageDigitalSignature | x509.KeyUsageCertSign,
		ExtKeyUsage:           []x509.ExtKeyUsage{x509.ExtKeyUsageServerAuth},
		IPAddresses:           []net.IP{net.IPv4(127, 0, 0, 1), net.IPv6loopback},
		DNSNames:              []string{"localhost"},
		IsCA:                  true,
		BasicConstraintsValid: true,
	}
	der, err := x509.CreateCertificate(rand.Reader, &tmpl, &tmpl, &priv.PublicKey, priv)
	if err != nil {
		t.Fatalf("createcert: %v", err)
	}
	leaf, err := x509.ParseCertificate(der)
	if err != nil {
		t.Fatalf("parsecert: %v", err)
	}
	pool := x509.NewCertPool()
	pool.AddCert(leaf)
	server = &tls.Config{
		Certificates: []tls.Certificate{{Certificate: [][]byte{der}, PrivateKey: priv, Leaf: leaf}},
		NextProtos:   []string{alpnProto},
	}
	client = &tls.Config{
		RootCAs:    pool,
		ServerName: "127.0.0.1",
		NextProtos: []string{alpnProto},
	}
	return server, client
}

// dialPair brings up a loopback listener and a connected dialer↔accepter Session
// pair, returning (dialerSession, accepterSession, cleanup).
func dialPair(t *testing.T) (dialer, accepter *Session, cleanup func()) {
	t.Helper()
	serverTLS, clientTLS := selfSignedTLS(t)
	ln, err := Listen("127.0.0.1:0", serverTLS)
	if err != nil {
		t.Fatalf("Listen: %v", err)
	}
	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)

	type res struct {
		s   *Session
		err error
	}
	ch := make(chan res, 1)
	go func() {
		s, err := ln.Accept(ctx)
		ch <- res{s, err}
	}()

	d, err := Dial(ctx, ln.Addr(), clientTLS)
	if err != nil {
		cancel()
		ln.Close()
		t.Fatalf("Dial: %v", err)
	}
	r := <-ch
	if r.err != nil {
		cancel()
		ln.Close()
		t.Fatalf("Accept: %v", r.err)
	}
	cleanup = func() {
		d.Close()
		r.s.Close()
		ln.Close()
		cancel()
	}
	return d, r.s, cleanup
}

// TestSessionTurnRoundTrip — 3 turns sent back-to-back arrive byte-identical and
// in order. SoT: the hex of each received turn equals the hex sent.
func TestSessionTurnRoundTrip(t *testing.T) {
	d, a, cleanup := dialPair(t)
	defer cleanup()

	turns := [][]byte{
		{0x01, 0x00, 0x00, 0x00, 0x10, 0x00, 0x07, 0xAA}, // synthetic command-record-ish bytes
		{0x01, 0x01, 0x00, 0x00, 0x10, 0x02, 0x09},
		{},                                     // empty turn (valid)
		bytes.Repeat([]byte{0xBE, 0xEF}, 1000), // larger turn, well under cap
	}
	for i, tn := range turns {
		if err := d.SendTurn(tn); err != nil {
			t.Fatalf("SendTurn[%d]: %v", i, err)
		}
	}
	for i, want := range turns {
		got, err := a.RecvTurn()
		if err != nil {
			t.Fatalf("RecvTurn[%d]: %v", i, err)
		}
		if !bytes.Equal(got, want) {
			t.Fatalf("turn[%d] MISMATCH:\n sent=%s\n recv=%s", i, hex.EncodeToString(want), hex.EncodeToString(got))
		}
		if i < 3 {
			t.Logf("FSV turn[%d] ok: sent=%s recv=%s (%d B)", i, hex.EncodeToString(want), hex.EncodeToString(got), len(got))
		} else {
			t.Logf("FSV turn[%d] ok: %d B byte-identical (head=%s)", i, len(got), hex.EncodeToString(got[:8]))
		}
	}
}

// TestSessionPeerCloseNoPartialTurn — when the peer closes, the next RecvTurn
// returns an error and never a partial/garbage turn (SoT: error returned, no
// bytes). State printed before (one good turn) and after (closed → error).
func TestSessionPeerCloseNoPartialTurn(t *testing.T) {
	d, a, cleanup := dialPair(t)
	defer cleanup()

	// BEFORE: one good turn proves the channel is live.
	want := []byte{0xDE, 0xAD, 0xBE, 0xEF}
	if err := d.SendTurn(want); err != nil {
		t.Fatalf("SendTurn: %v", err)
	}
	got, err := a.RecvTurn()
	if err != nil || !bytes.Equal(got, want) {
		t.Fatalf("pre-close recv: got=%x err=%v, want %x", got, err, want)
	}
	t.Logf("FSV before close: recv %x (live)", got)

	// ACTION: dialer closes the session.
	if err := d.Close(); err != nil {
		t.Logf("close returned: %v (non-fatal)", err)
	}

	// AFTER: accepter's next RecvTurn must error, returning no turn bytes.
	got2, err2 := a.RecvTurn()
	if err2 == nil {
		t.Fatalf("RecvTurn after peer close returned no error; got %d bytes %x", len(got2), got2)
	}
	if got2 != nil {
		t.Fatalf("RecvTurn after peer close returned a partial turn: %x", got2)
	}
	t.Logf("FSV after close: RecvTurn → err=%v, turn=nil (no partial surfaced)", err2)
}

// TestSessionOversizedRejected — SendTurn over the cap is rejected without
// writing, and the session still delivers the next valid turn. SoT: the error,
// then a successful round-trip after.
func TestSessionOversizedRejected(t *testing.T) {
	d, a, cleanup := dialPair(t)
	defer cleanup()

	oversized := make([]byte, MaxTurnBytes+1)
	err := d.SendTurn(oversized)
	if err == nil {
		t.Fatalf("SendTurn(%d bytes) accepted; want rejection over %d cap", len(oversized), MaxTurnBytes)
	}
	t.Logf("FSV oversized (%d B): rejected → %v", len(oversized), err)

	// Session still usable: a valid turn round-trips.
	want := []byte{0x42, 0x43, 0x44}
	if err := d.SendTurn(want); err != nil {
		t.Fatalf("post-reject SendTurn: %v", err)
	}
	got, err := a.RecvTurn()
	if err != nil || !bytes.Equal(got, want) {
		t.Fatalf("post-reject recv: got=%x err=%v, want %x", got, err, want)
	}
	t.Logf("FSV after reject: session still usable, recv %x", got)

	// At the cap exactly is allowed.
	atCap := bytes.Repeat([]byte{0x5A}, MaxTurnBytes)
	if err := d.SendTurn(atCap); err != nil {
		t.Fatalf("SendTurn at cap (%d B): %v", MaxTurnBytes, err)
	}
	got2, err := a.RecvTurn()
	if err != nil || !bytes.Equal(got2, atCap) {
		t.Fatalf("at-cap recv: len=%d err=%v, want %d", len(got2), err, MaxTurnBytes)
	}
	t.Logf("FSV at-cap (%d B): accepted + round-tripped", len(got2))
}

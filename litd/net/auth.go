package net

// auth.go: per-session transport security for the QUIC transport (#61,
// D-2026-06-11-26; D-2026-06-11-21 permissive-deps-only — this uses only the Go
// stdlib).
//
// THE MODEL (deliberate, and why InsecureSkipVerify below is NOT a fail-open):
// each session uses a fresh in-memory self-signed cert whose ONLY job is to
// encrypt the QUIC transport. Peer identity is NOT the cert chain — it is a
// pre-shared session TOKEN the host issues at session-create and the client
// presents in the handshake. The token is compared constant-time, and a
// mismatch closes the connection BEFORE any turn data flows (see
// Listener.AcceptAuthenticated in quic.go). So authentication is real and
// fail-closed; the cert is just the TLS the QUIC layer requires. Pinning a
// self-signed cert chain would add nothing — the token is the secret.

import (
	"crypto/ecdsa"
	"crypto/elliptic"
	"crypto/rand"
	"crypto/subtle"
	"crypto/tls"
	"crypto/x509"
	"crypto/x509/pkix"
	"fmt"
	"math/big"
	"time"
)

// TokenLen is the session-token size in bytes (256-bit — far beyond brute force
// over a single match's lifetime).
const TokenLen = 32

// NewSessionToken returns a cryptographically-random session token. The host
// generates one per session at create time and hands it to a joining client out
// of band — via the lobby join (#80) or the M9 hub flow. Treat it as a secret.
func NewSessionToken() ([]byte, error) {
	t := make([]byte, TokenLen)
	if _, err := rand.Read(t); err != nil {
		return nil, fmt.Errorf("net: session-token generation: %w", err)
	}
	return t, nil
}

// tokensEqual compares two tokens in constant time (no early-exit length/byte
// timing leak). Different lengths compare false. Empty vs non-empty is false, so
// a client that sends no token is refused exactly like a wrong one.
func tokensEqual(a, b []byte) bool {
	return subtle.ConstantTimeCompare(a, b) == 1
}

// SelfSignedTLS builds a per-session (server, client) TLS config pair. The
// server presents a fresh in-memory self-signed P-256 cert; the client sets
// InsecureSkipVerify because — per the model above — the cert only encrypts and
// the PSK session token authenticates. Generate one pair per session.
//
//nolint:gosec // InsecureSkipVerify is intentional: auth is the PSK token, not
// the cert chain (#61 / D-2026-06-11-26). The handshake refuses a bad token
// before any data, so this is not a fail-open.
func SelfSignedTLS() (server, client *tls.Config, err error) {
	priv, err := ecdsa.GenerateKey(elliptic.P256(), rand.Reader)
	if err != nil {
		return nil, nil, fmt.Errorf("net: session key generation: %w", err)
	}
	tmpl := x509.Certificate{
		SerialNumber: big.NewInt(1),
		Subject:      pkix.Name{CommonName: "litd-net session"},
		NotBefore:    time.Now().Add(-time.Hour),
		NotAfter:     time.Now().Add(24 * time.Hour),
		KeyUsage:     x509.KeyUsageDigitalSignature,
		ExtKeyUsage:  []x509.ExtKeyUsage{x509.ExtKeyUsageServerAuth},
	}
	der, err := x509.CreateCertificate(rand.Reader, &tmpl, &tmpl, &priv.PublicKey, priv)
	if err != nil {
		return nil, nil, fmt.Errorf("net: self-signed cert: %w", err)
	}
	server = &tls.Config{
		Certificates: []tls.Certificate{{Certificate: [][]byte{der}, PrivateKey: priv}},
		NextProtos:   []string{alpnProto},
		MinVersion:   tls.VersionTLS13, // QUIC mandates TLS 1.3
	}
	client = &tls.Config{
		InsecureSkipVerify: true, // see model + nolint note above: token is the auth
		NextProtos:         []string{alpnProto},
		MinVersion:         tls.VersionTLS13,
	}
	return server, client, nil
}

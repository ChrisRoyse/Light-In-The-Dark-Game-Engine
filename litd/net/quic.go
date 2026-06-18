package net

// quic.go contains the only quic-go-aware code in litd/net (D-2026-06-11-26: the
// M7 transport is quic-go, star topology). Everything quic-go is confined here;
// the public Session (session.go) holds only stdlib io interfaces, so no quic-go
// type appears in an exported signature (R-API / import-graph constraint).
//
// A Session rides ONE QUIC bidirectional reliable stream (D-26: turns + state
// hashes go on the reliable stream; RFC 9221 datagrams exist but are unused for
// turns). On connect the dialer opens the stream and writes a one-byte protocol
// version; the accepter reads + checks it, so both ends hold a live, agreed
// stream before their Session is returned. Cert/PSK hardening is #61 — this
// layer requires a caller-supplied *tls.Config and fails closed without one.

import (
	"context"
	"crypto/tls"
	"encoding/binary"
	"fmt"
	"io"

	quic "github.com/quic-go/quic-go"
)

// alpnProto is the QUIC ALPN; dialer and listener must agree or the TLS
// handshake fails. protocolVersion is the first stream byte, a cheap guard that
// both ends speak this framing (build-hash/seed join guard is #74).
const (
	alpnProto            = "litd-net/1"
	protocolVersion byte = 1
	// maxTokenWire bounds the handshake token so a garbled/hostile peer cannot
	// force a large allocation before auth (fail-closed, §2.4).
	maxTokenWire = 256
	// authRejectCode is the QUIC application close code for a rejected PSK token.
	authRejectCode quic.ApplicationErrorCode = 1
)

// writeHandshake sends the protocol version then a length-prefixed session token
// (token may be empty for the unauthenticated transport path). It also flushes
// the first STREAM frame so the accepter's AcceptStream returns.
func writeHandshake(stream *quic.Stream, token []byte) error {
	if len(token) > maxTokenWire {
		return fmt.Errorf("net: session token too long: %d > %d", len(token), maxTokenWire)
	}
	buf := make([]byte, 3+len(token))
	buf[0] = protocolVersion
	binary.BigEndian.PutUint16(buf[1:3], uint16(len(token)))
	copy(buf[3:], token)
	if _, err := stream.Write(buf); err != nil {
		return fmt.Errorf("net: handshake write: %w", err)
	}
	return nil
}

// readHandshake reads + validates the version byte and returns the peer's
// length-prefixed session token (empty if the peer sent none). A wrong version
// or an over-long token length is a loud, fail-closed error.
func readHandshake(stream *quic.Stream) ([]byte, error) {
	var head [3]byte
	if _, err := io.ReadFull(stream, head[:]); err != nil {
		return nil, fmt.Errorf("net: handshake read: %w", err)
	}
	if head[0] != protocolVersion {
		return nil, fmt.Errorf("net: protocol version mismatch: peer=%d want=%d", head[0], protocolVersion)
	}
	n := binary.BigEndian.Uint16(head[1:3])
	if int(n) > maxTokenWire {
		return nil, fmt.Errorf("net: handshake token length %d exceeds %d cap", n, maxTokenWire)
	}
	tok := make([]byte, n)
	if _, err := io.ReadFull(stream, tok); err != nil {
		return nil, fmt.Errorf("net: handshake token read: %w", err)
	}
	return tok, nil
}

// withALPN clones conf and ensures the litd-net ALPN is present. A nil config is
// a loud error: QUIC mandates TLS, and silently fabricating one would be a
// fail-open (§2.4). Hardening the contents (pinned cert, PSK) is #61.
func withALPN(conf *tls.Config) (*tls.Config, error) {
	if conf == nil {
		return nil, fmt.Errorf("net: nil tls.Config — QUIC requires TLS (see #61 for cert/PSK)")
	}
	c := conf.Clone()
	if len(c.NextProtos) == 0 {
		c.NextProtos = []string{alpnProto}
	}
	return c, nil
}

// Listener accepts incoming QUIC sessions on a UDP address. It wraps the quic-go
// listener so callers never see a quic-go type.
type Listener struct {
	ql *quic.Listener
}

// Listen binds a QUIC listener on addr (host:port; ":0" picks a free port — read
// it back with Addr). tlsConf is required.
func Listen(addr string, tlsConf *tls.Config) (*Listener, error) {
	tc, err := withALPN(tlsConf)
	if err != nil {
		return nil, err
	}
	ql, err := quic.ListenAddr(addr, tc, nil)
	if err != nil {
		return nil, fmt.Errorf("net: listen %s: %w", addr, err)
	}
	return &Listener{ql: ql}, nil
}

// Addr is the bound local address (resolve the actual port after ":0").
func (l *Listener) Addr() string { return l.ql.Addr().String() }

// Close stops accepting and tears down the listener.
func (l *Listener) Close() error { return l.ql.Close() }

// Accept blocks until a dialer connects and its stream handshake completes,
// then returns the established Session. It does NOT authenticate — any peer
// speaking the protocol is accepted (use AcceptAuthenticated for PSK auth, #61).
// ctx bounds the wait.
func (l *Listener) Accept(ctx context.Context) (*Session, error) {
	return l.accept(ctx, nil)
}

// AcceptAuthenticated is like Accept but requires the dialer's handshake to
// carry a session token byte-equal (constant-time) to expected. A wrong, empty,
// or stale token closes the connection with authRejectCode and returns an error
// — BEFORE any turn data can flow and WITHOUT creating a Session (#61). expected
// must be non-empty.
func (l *Listener) AcceptAuthenticated(ctx context.Context, expected []byte) (*Session, error) {
	if len(expected) == 0 {
		return nil, fmt.Errorf("net: AcceptAuthenticated requires a non-empty expected token")
	}
	return l.accept(ctx, expected)
}

// accept does the shared connect+handshake. When expected != nil, the peer's
// token is verified constant-time and a mismatch is refused before a Session
// exists.
func (l *Listener) accept(ctx context.Context, expected []byte) (*Session, error) {
	conn, err := l.ql.Accept(ctx)
	if err != nil {
		return nil, fmt.Errorf("net: accept conn: %w", err)
	}
	stream, err := conn.AcceptStream(ctx)
	if err != nil {
		conn.CloseWithError(0, "accept stream failed")
		return nil, fmt.Errorf("net: accept stream: %w", err)
	}
	tok, err := readHandshake(stream)
	if err != nil {
		conn.CloseWithError(0, "handshake failed")
		return nil, err
	}
	if expected != nil && !tokensEqual(tok, expected) {
		// Refuse before any turn data flows; no Session is created.
		conn.CloseWithError(authRejectCode, "auth rejected")
		return nil, fmt.Errorf("net: auth rejected: session-token mismatch from %s", conn.RemoteAddr())
	}
	return newSession(conn, stream), nil
}

// Dial connects to a QUIC listener at addr and returns an established Session,
// sending no auth token (the unauthenticated transport path). ctx bounds the
// connect + handshake.
func Dial(ctx context.Context, addr string, tlsConf *tls.Config) (*Session, error) {
	return dial(ctx, addr, nil, tlsConf)
}

// DialAuthenticated connects and presents token in the handshake, for a host
// using AcceptAuthenticated. token must be non-empty. Pair with SelfSignedTLS
// for the standard config (cert = encryption only; token = auth).
func DialAuthenticated(ctx context.Context, addr string, token []byte, tlsConf *tls.Config) (*Session, error) {
	if len(token) == 0 {
		return nil, fmt.Errorf("net: DialAuthenticated requires a non-empty token")
	}
	return dial(ctx, addr, token, tlsConf)
}

func dial(ctx context.Context, addr string, token []byte, tlsConf *tls.Config) (*Session, error) {
	tc, err := withALPN(tlsConf)
	if err != nil {
		return nil, err
	}
	conn, err := quic.DialAddr(ctx, addr, tc, nil)
	if err != nil {
		return nil, fmt.Errorf("net: dial %s: %w", addr, err)
	}
	stream, err := conn.OpenStreamSync(ctx)
	if err != nil {
		conn.CloseWithError(0, "open stream failed")
		return nil, fmt.Errorf("net: open stream: %w", err)
	}
	if err := writeHandshake(stream, token); err != nil {
		conn.CloseWithError(0, "handshake write failed")
		return nil, err
	}
	return newSession(conn, stream), nil
}

// newSession wires a quic conn+stream into the transport-agnostic Session. The
// close func resets the stream and closes the connection with a clean code.
func newSession(conn *quic.Conn, stream *quic.Stream) *Session {
	return &Session{
		stream: stream,
		remote: conn.RemoteAddr().String(),
		closeFn: func() error {
			_ = stream.Close()
			return conn.CloseWithError(0, "session closed")
		},
	}
}

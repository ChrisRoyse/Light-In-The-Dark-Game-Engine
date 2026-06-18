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
)

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
// then returns the established Session. ctx bounds the wait.
func (l *Listener) Accept(ctx context.Context) (*Session, error) {
	conn, err := l.ql.Accept(ctx)
	if err != nil {
		return nil, fmt.Errorf("net: accept conn: %w", err)
	}
	stream, err := conn.AcceptStream(ctx)
	if err != nil {
		conn.CloseWithError(0, "accept stream failed")
		return nil, fmt.Errorf("net: accept stream: %w", err)
	}
	// Read the dialer's protocol-version byte (fail closed on mismatch).
	var hs [1]byte
	if _, err := io.ReadFull(stream, hs[:]); err != nil {
		conn.CloseWithError(0, "handshake read failed")
		return nil, fmt.Errorf("net: handshake read: %w", err)
	}
	if hs[0] != protocolVersion {
		conn.CloseWithError(0, "protocol version mismatch")
		return nil, fmt.Errorf("net: protocol version mismatch: peer=%d want=%d", hs[0], protocolVersion)
	}
	return newSession(conn, stream), nil
}

// Dial connects to a QUIC listener at addr and returns an established Session.
// tlsConf is required (use RootCAs to pin the listener's self-signed cert; #61
// adds PSK). ctx bounds the connect + handshake.
func Dial(ctx context.Context, addr string, tlsConf *tls.Config) (*Session, error) {
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
	// Write the protocol-version byte; this also flushes the STREAM frame so the
	// accepter's AcceptStream returns.
	if _, err := stream.Write([]byte{protocolVersion}); err != nil {
		conn.CloseWithError(0, "handshake write failed")
		return nil, fmt.Errorf("net: handshake write: %w", err)
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

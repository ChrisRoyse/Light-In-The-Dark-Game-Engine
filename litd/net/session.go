// Package net is the litd multiplayer transport (M7, D-2026-06-11-26): a star
// topology over quic-go. A Session is one reliable channel between two peers
// (host↔player, or relay↔player) that carries command turns and state hashes.
//
// Layering rules (PRD §4.1): litd/sim never imports litd/net; turn bytes are
// opaque here (the command-record encoding is input.md §8 / the command-turn
// pipeline #65). No quic-go type appears in any exported signature — the
// transport details live in quic.go behind stdlib io interfaces.
package net

import (
	"encoding/binary"
	"fmt"
	"io"
)

// MaxTurnBytes caps one command turn. Turns are small (a 2–4 tick group of
// fixed-size command records, input.md §8); 64 KiB is far above any real turn
// and bounds a single read so a malicious/garbled peer cannot force an unbounded
// allocation (fail-closed, §2.4).
const MaxTurnBytes = 64 * 1024

// Session is one peer's end of a reliable QUIC stream carrying command turns.
// Turns are length-prefixed and delivered in send order. It is single-flow: one
// goroutine may SendTurn while another RecvTurns, but concurrent SendTurns (or
// concurrent RecvTurns) are not supported — turns are produced serially per peer.
type Session struct {
	stream  io.ReadWriteCloser
	remote  string
	closeFn func() error
}

// RemoteAddr is the peer's address, for logging/diagnostics.
func (s *Session) RemoteAddr() string { return s.remote }

// SendTurn writes one command turn as a length-prefixed frame (u32 little-endian
// length, then the bytes). A turn larger than MaxTurnBytes is rejected before
// anything is written, and the session stays usable for the next turn. A
// zero-length turn is a valid empty frame. A transport write error is returned
// verbatim (wrapped) — never swallowed.
func (s *Session) SendTurn(turn []byte) error {
	if len(turn) > MaxTurnBytes {
		return fmt.Errorf("net: turn too large: %d bytes > %d cap (nothing sent)", len(turn), MaxTurnBytes)
	}
	var hdr [4]byte
	binary.LittleEndian.PutUint32(hdr[:], uint32(len(turn)))
	if _, err := s.stream.Write(hdr[:]); err != nil {
		return fmt.Errorf("net: send turn header: %w", err)
	}
	if len(turn) > 0 {
		if _, err := s.stream.Write(turn); err != nil {
			return fmt.Errorf("net: send turn body: %w", err)
		}
	}
	return nil
}

// RecvTurn reads the next command turn in send order. If the peer closes or the
// stream breaks, it returns an error and NEVER a partial turn (a half-read frame
// surfaces as an error, not truncated bytes). A framed length exceeding
// MaxTurnBytes is a protocol violation and is refused (fail-closed) rather than
// allocated.
func (s *Session) RecvTurn() ([]byte, error) {
	var hdr [4]byte
	if _, err := io.ReadFull(s.stream, hdr[:]); err != nil {
		return nil, fmt.Errorf("net: recv turn header: %w", err)
	}
	n := binary.LittleEndian.Uint32(hdr[:])
	if n > MaxTurnBytes {
		return nil, fmt.Errorf("net: recv turn: framed length %d exceeds %d cap", n, MaxTurnBytes)
	}
	if n == 0 {
		return []byte{}, nil
	}
	buf := make([]byte, n)
	if _, err := io.ReadFull(s.stream, buf); err != nil {
		return nil, fmt.Errorf("net: recv turn body (%d bytes): %w", n, err)
	}
	return buf, nil
}

// Close tears down the stream and the underlying connection. Idempotent-ish: a
// second call may return a transport error but never panics.
func (s *Session) Close() error {
	if s.closeFn == nil {
		return nil
	}
	return s.closeFn()
}

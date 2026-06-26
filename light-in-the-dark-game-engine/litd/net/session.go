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

// Frame kinds multiplexed on the one reliable stream. The command-turn stream
// (KindTurn) and the out-of-band chat stream (KindChat, #316) share the wire but
// are TYPE-TAGGED so the receiver dispatches them apart — chat never enters the
// command/replay/hash path. New kinds append; the kind byte makes the protocol
// self-describing and forward-evolvable.
const (
	KindTurn byte = 0
	KindChat byte = 1
)

// Frame is one typed message read off the reliable stream.
type Frame struct {
	Kind    byte
	Payload []byte
}

// sendFrame writes [kind u8][u32 LE len][payload]. The payload is bounded by
// MaxTurnBytes (the hard wire cap); per-kind caps (e.g. MaxChatBytes) are
// enforced by the typed senders above this. Rejected before any byte is written.
func (s *Session) sendFrame(kind byte, payload []byte) error {
	if len(payload) > MaxTurnBytes {
		return fmt.Errorf("net: frame too large: %d bytes > %d cap (nothing sent)", len(payload), MaxTurnBytes)
	}
	var hdr [5]byte
	hdr[0] = kind
	binary.LittleEndian.PutUint32(hdr[1:], uint32(len(payload)))
	if _, err := s.stream.Write(hdr[:]); err != nil {
		return fmt.Errorf("net: send frame header: %w", err)
	}
	if len(payload) > 0 {
		if _, err := s.stream.Write(payload); err != nil {
			return fmt.Errorf("net: send frame body: %w", err)
		}
	}
	return nil
}

// RecvFrame reads the next typed frame in send order. The caller dispatches on
// Kind (turn → lockstep, chat → chat handler). A peer close or broken stream
// returns an error and NEVER a partial frame; a framed length over MaxTurnBytes
// is refused (fail-closed) rather than allocated.
func (s *Session) RecvFrame() (Frame, error) {
	var hdr [5]byte
	if _, err := io.ReadFull(s.stream, hdr[:]); err != nil {
		return Frame{}, fmt.Errorf("net: recv frame header: %w", err)
	}
	kind := hdr[0]
	n := binary.LittleEndian.Uint32(hdr[1:])
	if n > MaxTurnBytes {
		return Frame{}, fmt.Errorf("net: recv frame: framed length %d exceeds %d cap", n, MaxTurnBytes)
	}
	if n == 0 {
		return Frame{Kind: kind, Payload: []byte{}}, nil
	}
	buf := make([]byte, n)
	if _, err := io.ReadFull(s.stream, buf); err != nil {
		return Frame{}, fmt.Errorf("net: recv frame body (%d bytes): %w", n, err)
	}
	return Frame{Kind: kind, Payload: buf}, nil
}

// SendTurn writes one command turn as a KindTurn frame. A turn larger than
// MaxTurnBytes is rejected before anything is written, and the session stays
// usable for the next frame. A zero-length turn is a valid empty frame.
func (s *Session) SendTurn(turn []byte) error {
	if len(turn) > MaxTurnBytes {
		return fmt.Errorf("net: turn too large: %d bytes > %d cap (nothing sent)", len(turn), MaxTurnBytes)
	}
	return s.sendFrame(KindTurn, turn)
}

// RecvTurn reads the next frame and requires it to be a command turn. A chat (or
// any non-turn) frame arriving where a turn is expected is a protocol error — the
// real game loop uses RecvFrame and dispatches by kind instead.
func (s *Session) RecvTurn() ([]byte, error) {
	f, err := s.RecvFrame()
	if err != nil {
		return nil, err
	}
	if f.Kind != KindTurn {
		return nil, fmt.Errorf("net: expected turn frame, got kind %d", f.Kind)
	}
	return f.Payload, nil
}

// SendChat writes one encoded chat frame (EncodeChat) as a KindChat frame —
// out-of-band from the command stream. Bounded by MaxChatBytes at encode time.
func (s *Session) SendChat(frame []byte) error {
	return s.sendFrame(KindChat, frame)
}

// Close tears down the stream and the underlying connection. Idempotent-ish: a
// second call may return a transport error but never panics.
func (s *Session) Close() error {
	if s.closeFn == nil {
		return nil
	}
	return s.closeFn()
}

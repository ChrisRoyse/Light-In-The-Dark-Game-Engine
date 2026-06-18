package net

// chat.go: out-of-band chat (#316, D-17). Chat rides the reliable QUIC stream as
// a distinct KindChat frame (session.go) — it NEVER enters the command stream,
// the replay, or the state hash. litd/sim does not import this file (the
// import-graph check enforces net→sim stays one-directional and chat is
// presentation-side).
//
// Four channels: lobby (pre-match), all, allies, observers. Observer chat is
// information-isolated: a message from an observer never reaches a player, on any
// channel (observers have full vision; their words must not leak). Messages are
// UTF-8-validated plain text with no markup interpretation (injection-safe),
// bounded in size, and rate-limited per client.

import (
	"encoding/binary"
	"fmt"
	"time"
	"unicode/utf8"
)

// ChatChannel is the addressing scope of a message.
type ChatChannel uint8

const (
	ChanLobby     ChatChannel = iota // pre-match lobby: everyone present
	ChanAll                          // in-match: all players + observers
	ChanAllies                       // in-match: sender's team (+ observers)
	ChanObservers                    // observers only; players never receive
	chanCount
)

// MaxChatBytes caps one message body. Small by design — chat is short text, and
// a tight cap bounds allocation and screen abuse.
const MaxChatBytes = 512

// Participant identifies a chat endpoint for routing. ID is unique across all
// participants (players and observers share one id space). Team groups allies.
type Participant struct {
	ID         uint8
	Team       uint8
	IsObserver bool
}

// ChatMessage is one plain-text message. Sender is the participant ID.
type ChatMessage struct {
	Channel ChatChannel
	Sender  uint8
	Body    string
}

// EncodeChat serializes a message to a KindChat frame payload:
// [channel u8][sender u8][u16 LE body-len][body]. Fail-closed on an invalid
// channel, an over-cap body, or non-plain-text (invalid UTF-8 / control chars) —
// the message is never put on the wire.
func EncodeChat(m ChatMessage) ([]byte, error) {
	if m.Channel >= chanCount {
		return nil, fmt.Errorf("net: chat: invalid channel %d", m.Channel)
	}
	if err := validateBody(m.Body); err != nil {
		return nil, err
	}
	out := make([]byte, 0, 4+len(m.Body))
	out = append(out, byte(m.Channel), m.Sender)
	var l [2]byte
	binary.LittleEndian.PutUint16(l[:], uint16(len(m.Body)))
	out = append(out, l[:]...)
	out = append(out, m.Body...)
	return out, nil
}

// DecodeChat parses a KindChat frame payload. A bad channel, truncation, trailing
// bytes, over-cap body, or non-plain-text is a (non-fatal) error so a malformed
// chat frame is dropped without disturbing the session.
func DecodeChat(b []byte) (ChatMessage, error) {
	if len(b) < 4 {
		return ChatMessage{}, fmt.Errorf("net: chat: short frame (%d bytes)", len(b))
	}
	ch := ChatChannel(b[0])
	if ch >= chanCount {
		return ChatMessage{}, fmt.Errorf("net: chat: invalid channel %d", ch)
	}
	sender := b[1]
	n := int(binary.LittleEndian.Uint16(b[2:4]))
	if 4+n != len(b) {
		return ChatMessage{}, fmt.Errorf("net: chat: length %d does not match frame (%d bytes)", n, len(b))
	}
	body := string(b[4 : 4+n])
	if err := validateBody(body); err != nil {
		return ChatMessage{}, err
	}
	return ChatMessage{Channel: ch, Sender: sender, Body: body}, nil
}

// validateBody enforces plain-text: UTF-8, within the cap, no control characters
// (injection-safe — no markup, no newline/escape smuggling).
func validateBody(s string) error {
	if len(s) > MaxChatBytes {
		return fmt.Errorf("net: chat: body %d bytes exceeds %d cap", len(s), MaxChatBytes)
	}
	if !utf8.ValidString(s) {
		return fmt.Errorf("net: chat: body is not valid UTF-8")
	}
	for _, r := range s {
		if r < 0x20 || r == 0x7f {
			return fmt.Errorf("net: chat: body contains control character %#x", r)
		}
	}
	return nil
}

// Visible reports whether recipient should receive msg, given who sent it. This
// is the whole routing/info-isolation policy, as a pure function (no I/O) so it
// is exhaustively testable. The sender never receives its own message over the
// wire (the UI echoes locally).
func Visible(msg ChatMessage, sender, recipient Participant) bool {
	if recipient.ID == sender.ID {
		return false
	}
	switch msg.Channel {
	case ChanObservers:
		return recipient.IsObserver
	case ChanAllies:
		if recipient.IsObserver {
			return true // observers see everything
		}
		if sender.IsObserver {
			return false // an observer's words never reach players
		}
		return recipient.Team == sender.Team
	case ChanAll:
		if sender.IsObserver && !recipient.IsObserver {
			return false // observer "all" chat must not leak to players
		}
		return true
	case ChanLobby:
		return true // pre-match: everyone present
	default:
		return false
	}
}

// Recipients filters all participants down to those who should receive msg.
func Recipients(msg ChatMessage, sender Participant, all []Participant) []Participant {
	out := make([]Participant, 0, len(all))
	for _, p := range all {
		if Visible(msg, sender, p) {
			out = append(out, p)
		}
	}
	return out
}

// RateLimiter is a per-client token bucket gating chat send rate. The clock is
// caller-supplied so the policy is deterministically testable. Not safe for
// concurrent use; one per client.
type RateLimiter struct {
	capacity     float64
	refillPerSec float64
	tokens       float64
	last         time.Time
}

// NewRateLimiter builds a bucket holding up to capacity messages, refilling at
// refillPerSec per second. It starts full.
func NewRateLimiter(capacity, refillPerSec float64) *RateLimiter {
	return &RateLimiter{capacity: capacity, refillPerSec: refillPerSec, tokens: capacity}
}

// Allow consumes one token if available, returning whether the send is permitted
// at time now. Tokens refill based on elapsed time since the last call.
func (r *RateLimiter) Allow(now time.Time) bool {
	if !r.last.IsZero() {
		r.tokens += now.Sub(r.last).Seconds() * r.refillPerSec
		if r.tokens > r.capacity {
			r.tokens = r.capacity
		}
	}
	r.last = now
	if r.tokens >= 1 {
		r.tokens--
		return true
	}
	return false
}

// Scrollback is a fixed-capacity ring of received messages (the UI's chat
// history). It reuses one backing array — no per-message allocation once full
// (R-GC-1 spirit). Not safe for concurrent use.
type Scrollback struct {
	buf  []ChatMessage
	head int // index of the oldest message
	size int
}

// NewScrollback builds a ring holding the most recent capacity messages.
func NewScrollback(capacity int) *Scrollback {
	if capacity < 1 {
		capacity = 1
	}
	return &Scrollback{buf: make([]ChatMessage, capacity)}
}

// Add appends a message, evicting the oldest when full.
func (s *Scrollback) Add(m ChatMessage) {
	if s.size < len(s.buf) {
		s.buf[(s.head+s.size)%len(s.buf)] = m
		s.size++
		return
	}
	s.buf[s.head] = m // full: overwrite oldest, advance head
	s.head = (s.head + 1) % len(s.buf)
}

// Len is the number of buffered messages.
func (s *Scrollback) Len() int { return s.size }

// Messages returns buffered messages oldest→newest (a fresh slice; the ring is
// untouched).
func (s *Scrollback) Messages() []ChatMessage {
	out := make([]ChatMessage, s.size)
	for i := 0; i < s.size; i++ {
		out[i] = s.buf[(s.head+i)%len(s.buf)]
	}
	return out
}

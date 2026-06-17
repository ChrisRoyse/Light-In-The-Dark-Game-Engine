package ai

// Typed command-stack messaging — the inbound half (#273). The map-script
// domain sends commands *to* an AI player via WC3's CommandAI(player, command,
// data); the AI script reads them with the CommandsWaiting / pop / peek family.
// This is that per-player inbox: a bounded FIFO of integer-pair messages,
// drained at a fixed point in the AI phase, with WC3 pop/peek semantics
// preserved for the natives that read it (#276).
//
// No shared state crosses the boundary in the other direction either: a message
// is a fixed-size value (command + data ints, exactly WC3's pair), never a
// handle. The inbox is bounded — overflow is fail-closed (reject the newest
// command, count it, emit a loud diagnostic), never unbounded growth that could
// starve memory under a flooding map script.

import (
	"fmt"
	"io"
	"os"
)

// InboxMsg is one map-script → AI command: the integer-pair WC3 used
// (CommandAI's command + data), typed as a value with no pointers.
type InboxMsg struct {
	Command int32
	Data    int32
}

// Inbox is one AI player's bounded FIFO command queue. Implemented as a ring
// over a fixed backing array, so push/pop are O(1) and allocation-free, and the
// capacity is a hard ceiling.
type Inbox struct {
	buf     []InboxMsg
	head    int // index of the oldest message
	count   int // number of queued messages
	dropped int // messages rejected because the inbox was full
	diag    io.Writer
}

// NewInbox returns an inbox holding at most capacity pending messages.
// A capacity <= 0 is treated as 1 (a zero-capacity queue could accept nothing,
// which is never the intent).
func NewInbox(capacity int) *Inbox {
	if capacity <= 0 {
		capacity = 1
	}
	return &Inbox{buf: make([]InboxMsg, capacity), diag: os.Stderr}
}

// SetDiagnostics redirects the overflow diagnostic (default os.Stderr); nil
// silences it. The dropped counter is maintained regardless.
func (b *Inbox) SetDiagnostics(w io.Writer) { b.diag = w }

// Cap returns the inbox capacity.
func (b *Inbox) Cap() int { return len(b.buf) }

// Push enqueues m at the tail. Returns true if accepted, false if the inbox was
// full — in which case m is rejected (NOT dropping an older, already-queued
// command), the dropped counter advances, and a loud diagnostic is emitted.
//
// Rationale (documented decision): reject-newest is the fail-closed choice. The
// already-queued commands are the ones the AI is about to act on; dropping the
// oldest to make room would silently rewrite the AI's near-term plan under a
// flood. Refusing the newest preserves the committed FIFO and surfaces the flood
// loudly instead of hiding it.
func (b *Inbox) Push(m InboxMsg) bool {
	if b.count == len(b.buf) {
		b.dropped++
		if b.diag != nil {
			fmt.Fprintf(b.diag,
				"ai: INBOX FULL cap=%d — rejected command{Command:%d Data:%d} (total dropped=%d); "+
					"map script is flooding CommandAI faster than the AI drains it\n",
				len(b.buf), m.Command, m.Data, b.dropped)
		}
		return false
	}
	b.buf[(b.head+b.count)%len(b.buf)] = m
	b.count++
	return true
}

// Waiting returns the number of queued messages — WC3's CommandsWaiting.
func (b *Inbox) Waiting() int { return b.count }

// Peek returns the oldest queued message without removing it. ok is false on an
// empty inbox.
func (b *Inbox) Peek() (msg InboxMsg, ok bool) {
	if b.count == 0 {
		return InboxMsg{}, false
	}
	return b.buf[b.head], true
}

// Pop removes and returns the oldest queued message (FIFO). ok is false on an
// empty inbox.
func (b *Inbox) Pop() (msg InboxMsg, ok bool) {
	if b.count == 0 {
		return InboxMsg{}, false
	}
	msg = b.buf[b.head]
	b.head = (b.head + 1) % len(b.buf)
	b.count--
	return msg, true
}

// Dropped returns how many messages have been rejected for overflow over the
// inbox's lifetime.
func (b *Inbox) Dropped() int { return b.dropped }

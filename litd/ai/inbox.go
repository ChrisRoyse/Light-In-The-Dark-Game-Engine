package ai

// Typed command-stack messaging — the inbound half (#273; LIFO fix #378). The
// map-script domain sends commands *to* an AI player via WC3's CommandAI(player,
// command, data); the AI script reads them with the CommandsWaiting /
// GetLastCommand / PopLastCommand family (#276, inbox_natives.go). This is that
// per-player command stack.
//
// Semantics are WC3-faithful and verified against the source of truth (jassdoc;
// Hive "Intermediate AI concepts"; common.ai:135-138; PRD execution-model.md §6
// "integer-pair command stack"): a **LIFO stack**, capacity **12**. CommandAI
// pushes onto the top; GetLastCommand reads the top (most recent); PopLastCommand
// removes the top, exposing older commands. A ported WC3 AI script that pushes
// (A,B,C) and reads GetLastCommand must see C — so this is a stack, not a queue.
//
// No shared state crosses the boundary: a message is a fixed-size value (command
// + data ints, exactly WC3's pair), never a handle. The stack is bounded —
// overflow is fail-closed (reject the newest command, count it, emit a loud
// diagnostic), matching WC3's documented "the AI stops taking orders past 12"
// rather than growing unbounded or silently discarding queued commands.

import (
	"fmt"
	"io"
	"os"
)

// WC3CommandStackCap is the WC3 per-player AI command-stack capacity. Sending
// more than this many commands before the AI drains them makes the AI "stop
// taking orders" — reproduced here as a fail-closed reject-newest.
const WC3CommandStackCap = 12

// InboxMsg is one map-script → AI command: the integer-pair WC3 used
// (CommandAI's command + data), typed as a value with no pointers.
type InboxMsg struct {
	Command int32
	Data    int32
}

// Inbox is one AI player's bounded LIFO command stack. Backed by a fixed array
// with the top at index count-1, so push/pop are O(1) and allocation-free and
// the capacity is a hard ceiling.
type Inbox struct {
	buf     []InboxMsg
	count   int // number on the stack; top is buf[count-1]
	dropped int // commands rejected because the stack was full
	diag    io.Writer
}

// NewInbox returns a command stack holding at most capacity pending commands.
// A capacity <= 0 is treated as 1. Use NewCommandStack for the WC3 default of 12.
func NewInbox(capacity int) *Inbox {
	if capacity <= 0 {
		capacity = 1
	}
	return &Inbox{buf: make([]InboxMsg, capacity), diag: os.Stderr}
}

// NewCommandStack returns a WC3-capacity (12) command stack.
func NewCommandStack() *Inbox { return NewInbox(WC3CommandStackCap) }

// SetDiagnostics redirects the overflow diagnostic (default os.Stderr); nil
// silences it. The dropped counter is maintained regardless.
func (b *Inbox) SetDiagnostics(w io.Writer) { b.diag = w }

// Cap returns the stack capacity.
func (b *Inbox) Cap() int { return len(b.buf) }

// Push pushes m onto the top of the stack (WC3 CommandAI). Returns true if
// accepted, false if the stack was full — in which case m is rejected (the
// already-stacked commands are untouched), the dropped counter advances, and a
// loud diagnostic is emitted.
//
// Rationale (documented decision): reject-newest is both fail-closed and
// WC3-faithful. WC3 documents that past 12 queued commands "the AI stops taking
// orders" — i.e. new commands do not land. Dropping an older command instead
// would silently rewrite the AI's committed stack; refusing the newest preserves
// it and surfaces the flood loudly.
func (b *Inbox) Push(m InboxMsg) bool {
	if b.count == len(b.buf) {
		b.dropped++
		if b.diag != nil {
			fmt.Fprintf(b.diag,
				"ai: COMMAND STACK FULL cap=%d — rejected command{Command:%d Data:%d} (total dropped=%d); "+
					"map script is flooding CommandAI faster than the AI drains it\n",
				len(b.buf), m.Command, m.Data, b.dropped)
		}
		return false
	}
	b.buf[b.count] = m
	b.count++
	return true
}

// Waiting returns the number of commands on the stack — WC3's CommandsWaiting.
func (b *Inbox) Waiting() int { return b.count }

// Top returns the most-recently-pushed command without removing it (the top of
// the stack — what GetLastCommand/GetLastData read). ok is false on an empty
// stack.
func (b *Inbox) Top() (msg InboxMsg, ok bool) {
	if b.count == 0 {
		return InboxMsg{}, false
	}
	return b.buf[b.count-1], true
}

// Pop removes the top (most recent) command and returns it (WC3
// PopLastCommand). ok is false on an empty stack.
func (b *Inbox) Pop() (msg InboxMsg, ok bool) {
	if b.count == 0 {
		return InboxMsg{}, false
	}
	b.count--
	msg = b.buf[b.count]
	b.buf[b.count] = InboxMsg{} // clear the vacated slot — no stale leak
	return msg, true
}

// Dropped returns how many commands have been rejected for overflow over the
// stack's lifetime.
func (b *Inbox) Dropped() int { return b.dropped }

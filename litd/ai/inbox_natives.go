package ai

// Command-stack natives (#276; jass-mapping/ai-natives.md command-stack family;
// execution-model.md §6). The WC3 command-stack natives implemented over the
// typed LIFO command stack (inbox.go): the AI-side read verbs
// (CommandsWaiting / GetLastCommand / GetLastData / PopLastCommand) and the
// map-side delivery (CommandAI), with a per-player CommandBus and a documented,
// deterministic delivery tick.
//
// Native → method mapping (each common.ai command-stack native maps to exactly
// one method; integer-pair payload, with a LastPair compatibility accessor for
// ported scripts):
//
//	CommandsWaiting()   → (*Inbox).CommandsWaiting   // stack depth
//	GetLastCommand()    → (*Inbox).GetLastCommand    // top command int
//	GetLastData()       → (*Inbox).GetLastData       // top data int
//	PopLastCommand()    → (*Inbox).PopLastCommand    // remove top
//	CommandAI(p,c,d)    → (*CommandBus).CommandAI     // map-side push onto p's stack
//
// Delivery-tick contract (decided, documented, golden-tested): a command sent
// by the map script via CommandAI on tick T is visible to the AI on the SAME
// tick T. Within tick phase 2 the map-script domain runs first, then the AI
// domain (tick-and-scheduler.md §3.4), so every CommandAI push of tick T has
// already landed on the stack before the AI sub-phase reads it that same tick.
// There is no T+1 delay and no cross-tick buffering — delivery is synchronous
// within the phase, which is both simplest and what WC3 scripts assume.
//
// Empty-stack semantics (WC3-faithful, no panic): GetLastCommand / GetLastData
// on an empty stack return 0 (the zero command), and PopLastCommand on an empty
// stack is a no-op (the counter never goes negative). WC3 scripts always guard
// with CommandsWaiting() != 0 before reading, so empty access is undefined in
// practice; returning 0 / no-op is the safe canonical choice.

import (
	"encoding/binary"
	"errors"
	"fmt"
)

// CommandsWaiting returns the number of commands on the stack (WC3
// CommandsWaiting). Identical to Waiting; named for the native it implements.
func (b *Inbox) CommandsWaiting() int { return b.count }

// GetLastCommand returns the command value of the top (most-recent) command
// without removing it, or 0 if the stack is empty (WC3 GetLastCommand).
func (b *Inbox) GetLastCommand() int {
	if b.count == 0 {
		return 0
	}
	return int(b.buf[b.count-1].Command)
}

// GetLastData returns the data value of the top (most-recent) command without
// removing it, or 0 if the stack is empty (WC3 GetLastData).
func (b *Inbox) GetLastData() int {
	if b.count == 0 {
		return 0
	}
	return int(b.buf[b.count-1].Data)
}

// PopLastCommand removes the top (most-recent) command, exposing the next-older
// one. A no-op on an empty stack — the counter never goes negative (WC3
// PopLastCommand, fail-safe).
func (b *Inbox) PopLastCommand() {
	if b.count == 0 {
		return
	}
	b.count--
	b.buf[b.count] = InboxMsg{}
}

// LastPair is the integer-pair compatibility accessor for ported scripts: it
// returns the top command and data together with ok=false on an empty stack.
func (b *Inbox) LastPair() (command, data int, ok bool) {
	if b.count == 0 {
		return 0, 0, false
	}
	top := b.buf[b.count-1]
	return int(top.Command), int(top.Data), true
}

// CommandBus owns one command stack per AI player and is the map-script domain's
// door to them (CommandAI). Per-player stacks are kept dense and in registration
// order; no map is used, so serialization and lookup are deterministic.
type CommandBus struct {
	players []int
	boxes   []*Inbox
}

// NewCommandBus returns an empty bus.
func NewCommandBus() *CommandBus { return &CommandBus{} }

// Add registers a WC3-capacity (12) command stack for player and returns it.
// Panics on a duplicate player — two stacks for one player would split its
// inbound commands.
func (bus *CommandBus) Add(player int) *Inbox {
	if bus.Box(player) != nil {
		panic(fmt.Sprintf("ai: duplicate CommandBus.Add for player %d", player))
	}
	box := NewCommandStack()
	bus.players = append(bus.players, player)
	bus.boxes = append(bus.boxes, box)
	return box
}

// Box returns player's command stack, or nil if none is registered.
func (bus *CommandBus) Box(player int) *Inbox {
	for i, p := range bus.players {
		if p == player {
			return bus.boxes[i]
		}
	}
	return nil
}

// CommandAI pushes (command, data) onto player's command stack (WC3 CommandAI,
// the map-side delivery). Returns true if accepted, false if the player has no
// stack or the stack is full (fail-closed; the full-stack case is counted and
// logged by Inbox.Push). The push is visible to the AI on the same tick (see the
// delivery-tick contract above).
func (bus *CommandBus) CommandAI(player, command, data int) bool {
	box := bus.Box(player)
	if box == nil {
		return false
	}
	return box.Push(InboxMsg{Command: int32(command), Data: int32(data)})
}

// --- serialization (R-SIM-6): the inbox state is part of mid-game saves -----

var aiInboxMagic = [8]byte{'L', 'I', 'T', 'D', 'A', 'I', 'B', 'X'}

const aiInboxVersion uint16 = 1

// Save appends a single stack's canonical encoding to dst (count, dropped, then
// each command bottom-to-top so a decode rebuilds the same stack order).
func (b *Inbox) Save(dst []byte) []byte {
	dst = binary.LittleEndian.AppendUint32(dst, uint32(b.count))
	dst = binary.LittleEndian.AppendUint32(dst, uint32(b.dropped))
	for i := 0; i < b.count; i++ {
		dst = binary.LittleEndian.AppendUint32(dst, uint32(b.buf[i].Command))
		dst = binary.LittleEndian.AppendUint32(dst, uint32(b.buf[i].Data))
	}
	return dst
}

// Save appends the canonical encoding of every player's stack to dst.
func (bus *CommandBus) Save(dst []byte) []byte {
	dst = append(dst, aiInboxMagic[:]...)
	dst = binary.LittleEndian.AppendUint16(dst, aiInboxVersion)
	dst = binary.LittleEndian.AppendUint32(dst, uint32(len(bus.players)))
	for i, p := range bus.players {
		dst = binary.LittleEndian.AppendUint32(dst, uint32(p))
		dst = bus.boxes[i].Save(dst)
	}
	return dst
}

var (
	errInboxMagic   = errors.New("ai: bad command-bus save magic")
	errInboxVersion = errors.New("ai: unsupported command-bus save version")
)

// Load restores every player's stack from blob. The bus must already host the
// same set of players (registered via Add); Load matches by player id and
// rejects any mismatch. It is atomic: parsed into locals and validated fully
// before any stack is mutated, so a corrupt blob leaves the bus untouched.
func (bus *CommandBus) Load(blob []byte) error {
	if len(blob) < len(aiInboxMagic)+2+4 {
		return fmt.Errorf("ai: command-bus blob too short (%d bytes)", len(blob))
	}
	for i := range aiInboxMagic {
		if blob[i] != aiInboxMagic[i] {
			return errInboxMagic
		}
	}
	off := 8
	if v := binary.LittleEndian.Uint16(blob[off:]); v != aiInboxVersion {
		return fmt.Errorf("%w: %d (want %d)", errInboxVersion, v, aiInboxVersion)
	}
	off += 2
	count := int(binary.LittleEndian.Uint32(blob[off:]))
	off += 4
	if count != len(bus.players) {
		return fmt.Errorf("ai: command-bus save has %d players but bus hosts %d", count, len(bus.players))
	}

	type parsed struct {
		box     *Inbox
		n       int
		dropped int
		msgs    []InboxMsg
	}
	recs := make([]parsed, 0, count)
	seen := make([]bool, len(bus.players))
	for i := 0; i < count; i++ {
		if off+4+4+4 > len(blob) {
			return fmt.Errorf("ai: truncated command-bus player header at %d", off)
		}
		player := int(binary.LittleEndian.Uint32(blob[off:]))
		off += 4
		n := int(binary.LittleEndian.Uint32(blob[off:]))
		off += 4
		dropped := int(binary.LittleEndian.Uint32(blob[off:]))
		off += 4
		idx := -1
		for j, p := range bus.players {
			if p == player {
				idx = j
				break
			}
		}
		if idx < 0 {
			return fmt.Errorf("ai: command-bus save references player %d with no live stack", player)
		}
		if seen[idx] {
			return fmt.Errorf("ai: command-bus save references player %d twice", player)
		}
		seen[idx] = true
		box := bus.boxes[idx]
		if n < 0 || n > box.Cap() {
			return fmt.Errorf("ai: command-bus player %d count %d exceeds capacity %d", player, n, box.Cap())
		}
		if off+n*8 > len(blob) {
			return fmt.Errorf("ai: truncated command-bus messages for player %d", player)
		}
		msgs := make([]InboxMsg, n)
		for k := 0; k < n; k++ {
			msgs[k] = InboxMsg{
				Command: int32(binary.LittleEndian.Uint32(blob[off:])),
				Data:    int32(binary.LittleEndian.Uint32(blob[off+4:])),
			}
			off += 8
		}
		recs = append(recs, parsed{box: box, n: n, dropped: dropped, msgs: msgs})
	}
	if off != len(blob) {
		return fmt.Errorf("ai: %d trailing bytes after command-bus save", len(blob)-off)
	}

	// Validated — commit. Rebuild each stack bottom-to-top.
	for _, r := range recs {
		for k := range r.box.buf {
			r.box.buf[k] = InboxMsg{}
		}
		copy(r.box.buf, r.msgs)
		r.box.count = r.n
		r.box.dropped = r.dropped
	}
	return nil
}

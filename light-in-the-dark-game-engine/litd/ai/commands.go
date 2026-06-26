package ai

// Typed command-stack messaging — the outbound half (#273; execution-model.md
// §6 R-EXEC-3; tick-and-scheduler.md §3.4; milestones.md §9 deliverable 2). The
// AI domain's only way to act is to enqueue typed value commands onto the same
// ordered command stream player input and replays use, so an AI match replays
// from the command stream alone and AI can never desync. (The inbound half —
// the map-script → AI CommandAI inbox — is in inbox.go.)
//
// No shared state crosses the boundary: every command is a fixed-size value
// with no handles into AI-context memory and no closures. Ordering across the
// boundary is deterministic — multiple AI players enqueuing on one tick come out
// sequenced by player index then enqueue seq — and the buffer is pooled, so
// steady-state issuing allocates nothing.

import (
	"fmt"
	"hash/fnv"
)

// The typed command set: the build/train/harvest/attack-wave/guard intents an
// AI expresses. Operands (A, B) are documented per kind; they are plain ints,
// never pointers, so a command is meaningless to dereference across the boundary
// — the sim resolves ids to entities at apply time, and a stale id is a no-op
// (R-API-5), never a dangling read.
const (
	// CmdNone is the zero value — an unset command, never enqueued.
	CmdNone CommandKind = iota
	// CmdTrain: A = unitTypeID to train, B = producing structure id.
	CmdTrain
	// CmdBuild: A = structure typeID, B = packed build location.
	CmdBuild
	// CmdHarvest: A = worker id, B = resource node id.
	CmdHarvest
	// CmdAttack: A = attacking group/unit id, B = target id.
	CmdAttack
	// CmdAttackWave: A = formed group id, B = target region/point.
	CmdAttackWave
	// CmdGuard: A = unit/group id, B = guard point.
	CmdGuard
	// CmdRetreat: A = group id, B = rally point.
	CmdRetreat
	// cmdKindCount bounds the valid range — used to reject malformed kinds.
	cmdKindCount
)

// Valid reports whether k is a known command kind (excludes CmdNone and any
// out-of-range value). The stream rejects invalid kinds fail-closed.
func (k CommandKind) Valid() bool { return k > CmdNone && k < cmdKindCount }

// streamCmd is one entry in the ordered command stream: the issuing player, a
// monotonic enqueue sequence, and the typed command. The pair (player, seq) is
// the deterministic ordering key.
type streamCmd struct {
	player int32
	seq    uint32
	cmd    AICommand
}

// CommandStream is the single ordered buffer every AI player's commands land in
// — the typed descendant of WC3's per-player integer-pair command stacks,
// merged into one replay-grade stream. The sim drains it in phase 2 of the tick,
// applying each command in (player, seq) order. The backing slice is pooled
// across drains, so steady-state issuing allocates nothing.
//
// Ordering guarantee: the domain ticks players in fixed index order and seq is
// monotonic, so append order already equals (playerIndex, seq) order; the stream
// preserves that order on drain without a sort (and therefore without the
// allocation a sort would cost).
type CommandStream struct {
	buf      []streamCmd
	seq      uint32
	rejected int
}

// NewCommandStream returns a stream with capacity pre-reserved for cap commands
// per drain cycle (it still grows on demand, but a right-sized cap keeps steady
// state allocation-free).
func NewCommandStream(capacity int) *CommandStream {
	if capacity < 0 {
		capacity = 0
	}
	return &CommandStream{buf: make([]streamCmd, 0, capacity)}
}

// Commander returns the AICommander a given AI player issues through. The
// returned commander stamps every command with player authoritatively, so an AI
// context cannot spoof a command for another player. Call once per player at
// domain setup and hand it to Domain.AddPlayer.
func (s *CommandStream) Commander(player int) AICommander {
	return &playerCommander{s: s, player: int32(player)}
}

// playerCommander is one player's write handle onto the shared stream.
type playerCommander struct {
	s      *CommandStream
	player int32
}

// Issue enqueues c onto the stream, stamping it with this commander's player
// (authoritative — the AI cannot forge another player's id) and the next
// sequence number. A command with an invalid kind is rejected fail-closed
// (counted, not silently enqueued) — a malformed AI decision must not enter the
// replay stream.
func (pc *playerCommander) Issue(c AICommand) {
	if !c.Kind.Valid() {
		pc.s.rejected++
		return
	}
	c.Player = pc.player
	pc.s.seq++
	pc.s.buf = append(pc.s.buf, streamCmd{player: pc.player, seq: pc.s.seq, cmd: c})
}

// Len returns the number of commands currently buffered (not yet drained).
func (s *CommandStream) Len() int { return len(s.buf) }

// Rejected returns how many commands were rejected for an invalid kind.
func (s *CommandStream) Rejected() int { return s.rejected }

// At returns the i-th buffered command's player and payload, for inspection,
// recording, and replay (the stream is pure data — replaying is re-applying At
// in order).
func (s *CommandStream) At(i int) (player int, cmd AICommand) {
	return int(s.buf[i].player), s.buf[i].cmd
}

// Drain applies every buffered command in (player, seq) order via apply, then
// clears the buffer for reuse (no allocation, no reset of the seq counter — seq
// stays monotonic across the whole match so the replay key is stable). The sim
// passes its order-application closure as apply; a command naming a dead entity
// is the sim's no-op (R-API-5), but it stays in the stream so the replay is
// byte-identical regardless of which targets happened to be alive.
func (s *CommandStream) Drain(apply func(player int, cmd AICommand)) {
	for i := range s.buf {
		apply(int(s.buf[i].player), s.buf[i].cmd)
	}
	s.buf = s.buf[:0]
}

// Hash returns an order-sensitive FNV-1a hash of the currently buffered stream
// (player, seq, kind, operands). Two runs that produce the same hash produced
// the same command stream — the determinism / replay source of truth. Allocates
// (the hasher); for verification only, never on the hot path.
func (s *CommandStream) Hash() uint64 {
	h := fnv.New64a()
	var b [4]byte
	put := func(v uint32) {
		b[0], b[1], b[2], b[3] = byte(v), byte(v>>8), byte(v>>16), byte(v>>24)
		_, _ = h.Write(b[:])
	}
	for i := range s.buf {
		put(uint32(s.buf[i].player))
		put(s.buf[i].seq)
		put(uint32(s.buf[i].cmd.Kind))
		put(uint32(s.buf[i].cmd.A))
		put(uint32(s.buf[i].cmd.B))
	}
	return h.Sum64()
}

// String renders the buffered stream for diagnostics / FSV dumps.
func (s *CommandStream) String() string {
	out := fmt.Sprintf("CommandStream(%d cmds, %d rejected):", len(s.buf), s.rejected)
	for i := range s.buf {
		c := s.buf[i]
		out += fmt.Sprintf("\n  [%d] player=%d seq=%d kind=%d A=%d B=%d",
			i, c.player, c.seq, c.cmd.Kind, c.cmd.A, c.cmd.B)
	}
	return out
}

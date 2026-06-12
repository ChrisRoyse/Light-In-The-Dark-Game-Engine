package sim

// The deterministic command stream (input.md §8, R-INP-1.7): the
// sim's single front door. A CommandRecord is the unit of replay
// (determinism.md §6 — the replay body IS this stream) and the unit
// of M7 lockstep exchange. Encoding is fixed little-endian, no maps,
// no floats; world coordinates are quantized to fixed-point 32.32 at
// ENCODE time so client float math never leaks into the sim.
//
// Queue contract (tick-and-scheduler.md §6): the UI/driver thread
// stages records under a mutex at any time; the driver moves staged
// records into the per-tick pending queue BETWEEN ticks via
// IngestStagedCommands — the lock lives outside sim.Step(), the sim
// itself stays lock-free. Pending order is (tick, playerID, seq).
// Validation happens inside phase 1, deterministically: an invalid
// record is a deterministic no-op, never an error path — a recorded
// command must replay to the same no-op on every machine.

import (
	"sync"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
)

// CommandVersion is the encoding version. Replays refuse mismatches:
// a record carrying any other version is rejected, never guessed at.
const CommandVersion uint8 = 1

// Command opcodes — closed set, registry frozen per encoding version.
const (
	OpMove uint8 = iota
	OpAttack
	OpStop
	OpHold
	OpPatrol
	OpCastAbility
	OpTrain
	OpBuild
	OpCancel
	OpRally
	OpHarvest
	OpRepair
	OpBoard
	OpUnload
	// appended by #305 (append discipline: prior values stable, old
	// replays stay valid)
	OpGetItem
	opcodeCount
)

// CmdFlagQueued is flags bit 0: shift-queued order (input.md §7).
// Queue-pool application lands with #144; the bit is carried and
// validated today so replays recorded now stay valid.
const CmdFlagQueued uint8 = 1 << 0

// MaxCommandUnits caps the entity-ID list of one record (selection
// cap, input.md §2). Fixed array keeps CommandRecord a value type.
const MaxCommandUnits = 64

// cmdHeaderSize is the fixed header: version u8, tick u32, playerID
// u8, seq u16, opcode u8, flags u8.
const cmdHeaderSize = 10

// payloadShape declares which payload fields an opcode carries, in
// fixed encode order: units, target, point, data.
type payloadShape struct {
	units  bool // u8 count + count × u32 generation-counted IDs
	target bool // u32 EntityID (0 = none)
	point  bool // 2 × i64 fixed-point 32.32 world coords
	data   bool // u16 data-table index
}

// opShapes is the frozen per-opcode payload registry for version 1.
var opShapes = [opcodeCount]payloadShape{
	OpMove:        {units: true, point: true},
	OpAttack:      {units: true, target: true, point: true},
	OpStop:        {units: true},
	OpHold:        {units: true},
	OpPatrol:      {units: true, point: true},
	OpCastAbility: {units: true, target: true, point: true, data: true},
	OpTrain:       {units: true, data: true},
	OpBuild:       {units: true, point: true, data: true},
	OpCancel:      {units: true},
	OpRally:       {units: true, point: true},
	OpHarvest:     {units: true, target: true},
	OpRepair:      {units: true, target: true},
	OpBoard:       {units: true, target: true},
	OpUnload:      {units: true, point: true},
	OpGetItem:     {units: true, target: true},
}

// CommandRecord is one decoded command — a fixed-size value struct,
// no pointers, no heap. Tick 0 means "unassigned": ingest stamps it
// with the next unsimulated tick (single-player enqueue rule).
type CommandRecord struct {
	Version uint8
	Tick    uint32
	Player  uint8
	Seq     uint16
	Opcode  uint8
	Flags   uint8

	UnitCount uint8
	Units     [MaxCommandUnits]EntityID
	Target    EntityID
	Point     fixed.Vec2
	Data      uint16
}

// EncodedSize returns the exact encoded byte length of r, or -1 for
// an unencodable record (unknown opcode/version, oversized list).
func (r *CommandRecord) EncodedSize() int {
	if r.Version != CommandVersion || r.Opcode >= opcodeCount {
		return -1
	}
	if r.UnitCount > MaxCommandUnits {
		return -1
	}
	n := cmdHeaderSize
	sh := opShapes[r.Opcode]
	if sh.units {
		n += 1 + int(r.UnitCount)*4
	}
	if sh.target {
		n += 4
	}
	if sh.point {
		n += 16
	}
	if sh.data {
		n += 2
	}
	return n
}

// AppendEncode appends the fixed little-endian encoding of r to dst
// and returns the extended slice. With a preallocated dst the encode
// path is zero-alloc (R-GC-1 applies to the input layer). Returns
// (dst, false) unchanged for an unencodable record — fail closed.
func AppendEncode(dst []byte, r *CommandRecord) ([]byte, bool) {
	if r.EncodedSize() < 0 {
		return dst, false
	}
	dst = append(dst, r.Version,
		byte(r.Tick), byte(r.Tick>>8), byte(r.Tick>>16), byte(r.Tick>>24),
		r.Player,
		byte(r.Seq), byte(r.Seq>>8),
		r.Opcode, r.Flags)
	sh := opShapes[r.Opcode]
	if sh.units {
		dst = append(dst, r.UnitCount)
		for i := uint8(0); i < r.UnitCount; i++ {
			u := uint32(r.Units[i])
			dst = append(dst, byte(u), byte(u>>8), byte(u>>16), byte(u>>24))
		}
	}
	if sh.target {
		t := uint32(r.Target)
		dst = append(dst, byte(t), byte(t>>8), byte(t>>16), byte(t>>24))
	}
	if sh.point {
		dst = appendI64(dst, int64(r.Point.X))
		dst = appendI64(dst, int64(r.Point.Y))
	}
	if sh.data {
		dst = append(dst, byte(r.Data), byte(r.Data>>8))
	}
	return dst, true
}

func appendI64(dst []byte, v int64) []byte {
	u := uint64(v)
	return append(dst, byte(u), byte(u>>8), byte(u>>16), byte(u>>24),
		byte(u>>32), byte(u>>40), byte(u>>48), byte(u>>56))
}

// DecodeCommand decodes one record from the front of b into r,
// returning the bytes consumed. Any malformation — short buffer,
// unknown version (payload shape unknowable), unknown opcode,
// oversized unit list, truncated payload — is a deterministic
// rejection: r untouched, n = 0, ok = false. Never panics; the fuzz
// target holds DecodeCommand to that.
func DecodeCommand(b []byte, r *CommandRecord) (n int, ok bool) {
	if len(b) < cmdHeaderSize {
		return 0, false
	}
	version := b[0]
	opcode := b[8]
	if version != CommandVersion || opcode >= opcodeCount {
		return 0, false
	}
	var rec CommandRecord
	rec.Version = version
	rec.Tick = uint32(b[1]) | uint32(b[2])<<8 | uint32(b[3])<<16 | uint32(b[4])<<24
	rec.Player = b[5]
	rec.Seq = uint16(b[6]) | uint16(b[7])<<8
	rec.Opcode = opcode
	rec.Flags = b[9]
	p := cmdHeaderSize
	sh := opShapes[opcode]
	if sh.units {
		if len(b) < p+1 {
			return 0, false
		}
		count := b[p]
		p++
		if count > MaxCommandUnits || len(b) < p+int(count)*4 {
			return 0, false
		}
		rec.UnitCount = count
		for i := 0; i < int(count); i++ {
			rec.Units[i] = EntityID(uint32(b[p]) | uint32(b[p+1])<<8 |
				uint32(b[p+2])<<16 | uint32(b[p+3])<<24)
			p += 4
		}
	}
	if sh.target {
		if len(b) < p+4 {
			return 0, false
		}
		rec.Target = EntityID(uint32(b[p]) | uint32(b[p+1])<<8 |
			uint32(b[p+2])<<16 | uint32(b[p+3])<<24)
		p += 4
	}
	if sh.point {
		if len(b) < p+16 {
			return 0, false
		}
		rec.Point.X = fixed.F64(decodeI64(b[p:]))
		rec.Point.Y = fixed.F64(decodeI64(b[p+8:]))
		p += 16
	}
	if sh.data {
		if len(b) < p+2 {
			return 0, false
		}
		rec.Data = uint16(b[p]) | uint16(b[p+1])<<8
		p += 2
	}
	*r = rec
	return p, true
}

func decodeI64(b []byte) int64 {
	return int64(uint64(b[0]) | uint64(b[1])<<8 | uint64(b[2])<<16 |
		uint64(b[3])<<24 | uint64(b[4])<<32 | uint64(b[5])<<40 |
		uint64(b[6])<<48 | uint64(b[7])<<56)
}

// cmdQueueCap bounds both the staging buffer and the pending ring.
const cmdQueueCap = 1024

// CommandQueue is the staged per-tick queue. staged is the only
// piece of sim-adjacent state a foreign thread may touch, and only
// under mu; pending belongs to the sim thread exclusively.
type CommandQueue struct {
	mu     sync.Mutex
	staged []CommandRecord // fixed cap, mutex-guarded

	pending []CommandRecord // sorted (Tick, Player, Seq); sim-side
	head    int             // consumption cursor into pending

	stagedDropped  uint64 // staging buffer full
	pendingDropped uint64 // pending ring full at ingest
	lateDropped    uint64 // explicit Tick already simulated at ingest
}

func newCommandQueue() *CommandQueue {
	return &CommandQueue{
		staged:  make([]CommandRecord, 0, cmdQueueCap),
		pending: make([]CommandRecord, 0, cmdQueueCap),
	}
}

// Stage appends a record to the staging buffer. Safe to call from
// the UI/driver thread at any time. Returns false when the buffer is
// full — fail closed, counted, never silently grown.
func (q *CommandQueue) Stage(r CommandRecord) bool {
	q.mu.Lock()
	defer q.mu.Unlock()
	if len(q.staged) == cap(q.staged) {
		q.stagedDropped++
		return false
	}
	q.staged = append(q.staged, r)
	return true
}

// cmdLess orders pending records (Tick, Player, Seq) — the total
// order phase 1 consumes in.
func cmdLess(a, b *CommandRecord) bool {
	if a.Tick != b.Tick {
		return a.Tick < b.Tick
	}
	if a.Player != b.Player {
		return a.Player < b.Player
	}
	return a.Seq < b.Seq
}

// IngestStagedCommands moves staged records into the pending queue.
// The DRIVER calls this between ticks (never during one): it takes
// the staging lock, stamps unassigned ticks with the next
// unsimulated tick, and insertion-sorts into (Tick, Player, Seq)
// order. Records whose explicit tick has already been simulated are
// dropped deterministically and counted.
func (w *World) IngestStagedCommands() {
	q := w.Cmds
	q.mu.Lock()
	defer q.mu.Unlock()
	next := w.tick + 1
	for i := range q.staged {
		r := q.staged[i]
		if r.Tick == 0 {
			r.Tick = next
		} else if r.Tick <= w.tick {
			q.lateDropped++
			continue
		}
		// compact consumed prefix before checking capacity
		if q.head > 0 && len(q.pending) == cap(q.pending) {
			n := copy(q.pending, q.pending[q.head:])
			q.pending = q.pending[:n]
			q.head = 0
		}
		if len(q.pending) == cap(q.pending) {
			q.pendingDropped++
			continue
		}
		// insertion sort: scan back from the tail. Staging order is
		// the tiebreak for identical (Tick, Player, Seq) — stable.
		q.pending = q.pending[:len(q.pending)+1]
		j := len(q.pending) - 1
		for j > q.head && cmdLess(&r, &q.pending[j-1]) {
			q.pending[j] = q.pending[j-1]
			j--
		}
		q.pending[j] = r
	}
	q.staged = q.staged[:0]
}

// StageCommand stages a record for ingest. The public front door:
// scripted orders and player orders both route through here.
func (w *World) StageCommand(r CommandRecord) bool { return w.Cmds.Stage(r) }

// CmdApplied / CmdRejected / CmdDropped report the lifetime counters
// (FSV + telemetry surface).
func (w *World) CmdApplied() uint64  { return w.cmdApplied }
func (w *World) CmdRejected() uint64 { return w.cmdRejected }
func (w *World) CmdDropped() (staged, pending, late uint64) {
	return w.Cmds.stagedDropped, w.Cmds.pendingDropped, w.Cmds.lateDropped
}

// consumePendingCommands is phase 1's record drain: apply every
// pending record scheduled for the current tick, in (Player, Seq)
// order (the queue is already sorted). Sim-thread only — no lock.
func (w *World) consumePendingCommands() {
	q := w.Cmds
	for q.head < len(q.pending) && q.pending[q.head].Tick == w.tick {
		w.applyCommandRecord(&q.pending[q.head])
		q.head++
	}
	if q.head == len(q.pending) {
		q.pending = q.pending[:0]
		q.head = 0
	}
}

// applyCommandRecord is the phase-1 validation gate. Every check is
// deterministic; a failed check is a counted no-op, never an error.
//
//	version/opcode  → whole-record reject
//	actor liveness + ownership → per-unit filter; zero survivors = reject
//	order-head write → Move/Attack/Stop/Hold today; remaining verbs
//	                   validate + dispatch to OnCommandRecord, their
//	                   systems land with #144+.
func (w *World) applyCommandRecord(r *CommandRecord) {
	if r.Version != CommandVersion || r.Opcode >= opcodeCount ||
		r.UnitCount > MaxCommandUnits {
		w.cmdRejected++
		return
	}
	sh := opShapes[r.Opcode]
	valid := 0
	if sh.units {
		for i := uint8(0); i < r.UnitCount; i++ {
			id := r.Units[i]
			if !w.Ents.Alive(id) {
				continue
			}
			row := w.Owners.Row(id)
			if row == -1 || w.Owners.Player[row] != r.Player {
				continue
			}
			w.cmdActors[valid] = id
			valid++
		}
		if valid == 0 {
			w.cmdRejected++
			return
		}
	}
	// target liveness: a dead/stale target degrades to 0 (point /
	// no-target variant) rather than rejecting — WC3 semantics.
	target := r.Target
	if target != 0 && !w.Ents.Alive(target) {
		target = 0
	}
	var orderKind uint8
	writeOrder := false
	switch r.Opcode {
	case OpMove:
		orderKind, writeOrder = OrderMove, true
	case OpAttack:
		orderKind, writeOrder = OrderAttack, true
	case OpStop:
		orderKind, writeOrder = OrderStop, true
	case OpHold:
		orderKind, writeOrder = OrderHold, true
	case OpCastAbility:
		orderKind, writeOrder = OrderCastAbility, true
	case OpGetItem:
		orderKind, writeOrder = OrderPickup, true
	}
	if writeOrder {
		for i := 0; i < valid; i++ {
			or := w.Orders.Row(w.cmdActors[i])
			if or == -1 {
				continue
			}
			// a player command is an unqueued issue: queue cleared,
			// current order interrupted, new order installed (§2.3 —
			// the shift-queue flag joins the wire format with #146)
			w.issueOrderRow(or, w.cmdActors[i], Order{Kind: orderKind, Target: target, Point: r.Point, Data: r.Data}, false)
		}
	}
	w.cmdApplied++
	if w.OnCommandRecord != nil {
		w.OnCommandRecord(w.tick, r, w.cmdActors[:valid])
	}
}

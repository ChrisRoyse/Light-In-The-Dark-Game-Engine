package sim

// Replay format (#198, determinism.md §6): a replay is INPUTS, never
// state — header (format version, data-table fingerprint, map hash,
// seed, roster size), the ordered command stream, and a state-hash
// checkpoint trace (top + per-system sub-hashes every interval, so a
// divergence names its culprit system, not just "differs").
//
// Everything fails closed: bad magic, version mismatch, fingerprint
// mismatch, truncation, and trailing garbage are named load errors —
// a replay that cannot be verified exactly is refused, never run
// "best effort" into a silent desync.

import (
	"encoding/binary"
	"fmt"
	"io"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/statehash"
)

// ReplayMagic opens every replay file.
const ReplayMagic = "LITDRPL\x01"

// ReplayFormatVersion bumps on any layout change. v2 (#404) widened
// ReplayCommand from move-only to the full order vocabulary (Target +
// Data fields).
const ReplayFormatVersion uint32 = 2

// DefaultCheckpointInterval is the §6 checkpoint cadence in ticks.
const DefaultCheckpointInterval uint32 = 100

// Replay command kinds (#404). A STABLE enum — deliberately NOT the sim
// OrderKind values — so a recorded stream keeps its meaning across engine
// changes; append-only, Move stays 0 (the v1 sole kind). ToOrder maps each to
// a sim Order. Train/production is a queue command, not a unit Order, so it is
// not representable here yet (tracked on #404).
const (
	ReplayMove    uint8 = 0 // Point
	ReplayStop    uint8 = 1
	ReplayHold    uint8 = 2
	ReplayPatrol  uint8 = 3 // Point
	ReplayAttack  uint8 = 4 // Point; optional Target unit (focus-fire) else attack-move-by-point
	ReplayHarvest uint8 = 5 // Target node
	ReplayFollow  uint8 = 6 // Target unit
	ReplayBuild   uint8 = 7 // Point = site, Data = unit-type id
	ReplayMaxKind uint8 = ReplayBuild
)

// NoRosterRef is the ReplayCommand.Target value meaning "no target unit" — used
// by point/none orders and by attack-move (attack with a point, no focus unit).
const NoRosterRef = ^uint32(0)

// ReplayCommand is one recorded input: spawn-roster index addressing (stable
// across runs). Kind is a Replay* constant. Unit is the ordered unit; Target a
// second roster index for target-taking orders (NoRosterRef = none); Data a
// typed payload (ReplayBuild: unit-type id); X,Y a point in raw 32.32 bits.
type ReplayCommand struct {
	Tick   uint32
	Player uint8
	Kind   uint8
	Unit   uint32 // spawn-order index into the roster
	Target uint32 // target unit roster index, or NoRosterRef
	Data   uint16 // typed payload (build: unit-type id); 0 = none
	X, Y   int64  // fixed.F64 bits
}

// ToOrder translates a replay command into a sim Order, resolving roster
// indices to live entities via resolve (ok=false for an out-of-range or dead
// index). Returns ok=false when the command cannot be applied — a
// target-taking order whose target is missing — so the caller skips it exactly
// as it already skips a dead orderer. Decode/loadCommands reject unknown kinds
// at the boundary, so the default case is defensive only.
func (c *ReplayCommand) ToOrder(resolve func(idx uint32) (EntityID, bool)) (Order, bool) {
	pt := fixed.Vec2{X: fixed.F64(c.X), Y: fixed.F64(c.Y)}
	switch c.Kind {
	case ReplayMove:
		return Order{Kind: OrderMove, Point: pt}, true
	case ReplayStop:
		return Order{Kind: OrderStop}, true
	case ReplayHold:
		return Order{Kind: OrderHold}, true
	case ReplayPatrol:
		return Order{Kind: OrderPatrol, Point: pt}, true
	case ReplayAttack:
		ord := Order{Kind: OrderAttack, Point: pt}
		if c.Target != NoRosterRef {
			tgt, ok := resolve(c.Target)
			if !ok {
				return Order{}, false
			}
			ord.Target = tgt
		}
		return ord, true
	case ReplayHarvest:
		tgt, ok := resolve(c.Target)
		if !ok {
			return Order{}, false
		}
		return Order{Kind: OrderHarvest, Target: tgt}, true
	case ReplayFollow:
		tgt, ok := resolve(c.Target)
		if !ok {
			return Order{}, false
		}
		return Order{Kind: OrderFollow, Target: tgt}, true
	case ReplayBuild:
		return Order{Kind: OrderBuild, Point: pt, Data: c.Data}, true
	default:
		return Order{}, false
	}
}

// ReplayCheckpoint is one hash-trace entry: the top hash plus every
// per-system sub-hash in HashSystems order.
type ReplayCheckpoint struct {
	Tick uint32
	Top  uint64
	Subs []uint64
}

// Replay is one decoded (or under-construction) replay.
type Replay struct {
	Version     uint32
	Fingerprint uint64 // data-table content hash (0 = no tables bound)
	MapHash     uint64 // 0 until the map format lands (M5)
	Seed        uint64
	Roster      uint32 // built-in-layout unit count
	Interval    uint32 // checkpoint cadence in ticks
	Ticks       uint32 // total ticks of the recorded run
	Commands    []ReplayCommand
	Checkpoints []ReplayCheckpoint
}

// CheckpointFrom captures one checkpoint from a snapshot.
func CheckpointFrom(tick uint32, snap *statehash.Snapshot) ReplayCheckpoint {
	return ReplayCheckpoint{Tick: tick, Top: snap.Top, Subs: append([]uint64(nil), snap.Subs...)}
}

// Encode writes the canonical binary form (little-endian, fixed
// widths).
func (r *Replay) Encode(w io.Writer) error {
	le := binary.LittleEndian
	var scratch [8]byte
	put := func(vals ...any) error {
		for _, v := range vals {
			switch x := v.(type) {
			case uint32:
				le.PutUint32(scratch[:4], x)
				if _, err := w.Write(scratch[:4]); err != nil {
					return err
				}
			case uint64:
				le.PutUint64(scratch[:8], x)
				if _, err := w.Write(scratch[:8]); err != nil {
					return err
				}
			case uint8:
				scratch[0] = x
				if _, err := w.Write(scratch[:1]); err != nil {
					return err
				}
			case uint16:
				le.PutUint16(scratch[:2], x)
				if _, err := w.Write(scratch[:2]); err != nil {
					return err
				}
			case int64:
				le.PutUint64(scratch[:8], uint64(x))
				if _, err := w.Write(scratch[:8]); err != nil {
					return err
				}
			default:
				panic("sim: replay encode: unhandled type")
			}
		}
		return nil
	}
	if _, err := io.WriteString(w, ReplayMagic); err != nil {
		return err
	}
	if err := put(r.Version, r.Fingerprint, r.MapHash, r.Seed, r.Roster, r.Interval, r.Ticks); err != nil {
		return err
	}
	if err := put(uint32(len(r.Commands))); err != nil {
		return err
	}
	for i := range r.Commands {
		c := &r.Commands[i]
		if err := put(c.Tick, c.Player, c.Kind, c.Unit, c.Target, c.Data, c.X, c.Y); err != nil {
			return err
		}
	}
	if err := put(uint32(len(r.Checkpoints)), uint32(len(HashSystems))); err != nil {
		return err
	}
	for i := range r.Checkpoints {
		cp := &r.Checkpoints[i]
		if len(cp.Subs) != len(HashSystems) {
			return fmt.Errorf("sim: replay encode: checkpoint %d has %d subs, want %d", i, len(cp.Subs), len(HashSystems))
		}
		if err := put(cp.Tick, cp.Top); err != nil {
			return err
		}
		for _, s := range cp.Subs {
			if err := put(s); err != nil {
				return err
			}
		}
	}
	return nil
}

// DecodeReplay reads and validates one replay. Every malformation is
// a named error; trailing bytes after the trace are refused too.
func DecodeReplay(rd io.Reader) (*Replay, error) {
	le := binary.LittleEndian
	var scratch [8]byte
	fail := func(what string, err error) (*Replay, error) {
		if err == io.EOF || err == io.ErrUnexpectedEOF {
			return nil, fmt.Errorf("sim: replay: truncated while reading %s", what)
		}
		return nil, fmt.Errorf("sim: replay: %s: %w", what, err)
	}
	u32 := func(what string) (uint32, error) {
		if _, err := io.ReadFull(rd, scratch[:4]); err != nil {
			_, e := fail(what, err)
			return 0, e
		}
		return le.Uint32(scratch[:4]), nil
	}
	u64 := func(what string) (uint64, error) {
		if _, err := io.ReadFull(rd, scratch[:8]); err != nil {
			_, e := fail(what, err)
			return 0, e
		}
		return le.Uint64(scratch[:8]), nil
	}
	u8 := func(what string) (uint8, error) {
		if _, err := io.ReadFull(rd, scratch[:1]); err != nil {
			_, e := fail(what, err)
			return 0, e
		}
		return scratch[0], nil
	}
	u16 := func(what string) (uint16, error) {
		if _, err := io.ReadFull(rd, scratch[:2]); err != nil {
			_, e := fail(what, err)
			return 0, e
		}
		return le.Uint16(scratch[:2]), nil
	}

	magic := make([]byte, len(ReplayMagic))
	if _, err := io.ReadFull(rd, magic); err != nil {
		return fail("magic", err)
	}
	if string(magic) != ReplayMagic {
		return nil, fmt.Errorf("sim: replay: bad magic %q (not a .litdreplay file)", magic)
	}
	r := &Replay{}
	var err error
	if r.Version, err = u32("version"); err != nil {
		return nil, err
	}
	if r.Version != ReplayFormatVersion {
		return nil, fmt.Errorf("sim: replay: format version %d, this engine reads %d", r.Version, ReplayFormatVersion)
	}
	if r.Fingerprint, err = u64("fingerprint"); err != nil {
		return nil, err
	}
	if r.MapHash, err = u64("map hash"); err != nil {
		return nil, err
	}
	if r.Seed, err = u64("seed"); err != nil {
		return nil, err
	}
	if r.Roster, err = u32("roster"); err != nil {
		return nil, err
	}
	if r.Interval, err = u32("interval"); err != nil {
		return nil, err
	}
	if r.Interval == 0 {
		return nil, fmt.Errorf("sim: replay: checkpoint interval 0")
	}
	if r.Ticks, err = u32("ticks"); err != nil {
		return nil, err
	}
	nCmd, err := u32("command count")
	if err != nil {
		return nil, err
	}
	const maxRecords = 1 << 24 // refuse absurd counts before allocating
	if nCmd > maxRecords {
		return nil, fmt.Errorf("sim: replay: command count %d exceeds limit", nCmd)
	}
	r.Commands = make([]ReplayCommand, nCmd)
	lastTick := uint32(0)
	for i := range r.Commands {
		c := &r.Commands[i]
		if c.Tick, err = u32("command tick"); err != nil {
			return nil, err
		}
		if c.Tick < lastTick {
			return nil, fmt.Errorf("sim: replay: command %d out of order (tick %d after %d)", i, c.Tick, lastTick)
		}
		lastTick = c.Tick
		if c.Player, err = u8("command player"); err != nil {
			return nil, err
		}
		if c.Kind, err = u8("command kind"); err != nil {
			return nil, err
		}
		if c.Kind > ReplayMaxKind {
			return nil, fmt.Errorf("sim: replay: command %d has unknown kind %d (max %d)", i, c.Kind, ReplayMaxKind)
		}
		if c.Unit, err = u32("command unit"); err != nil {
			return nil, err
		}
		if c.Target, err = u32("command target"); err != nil {
			return nil, err
		}
		if c.Data, err = u16("command data"); err != nil {
			return nil, err
		}
		var x, y uint64
		if x, err = u64("command x"); err != nil {
			return nil, err
		}
		if y, err = u64("command y"); err != nil {
			return nil, err
		}
		c.X, c.Y = int64(x), int64(y)
	}
	nCp, err := u32("checkpoint count")
	if err != nil {
		return nil, err
	}
	nSubs, err := u32("sub-hash count")
	if err != nil {
		return nil, err
	}
	if int(nSubs) != len(HashSystems) {
		return nil, fmt.Errorf("sim: replay: %d sub-hashes per checkpoint, this engine has %d systems", nSubs, len(HashSystems))
	}
	if nCp > maxRecords {
		return nil, fmt.Errorf("sim: replay: checkpoint count %d exceeds limit", nCp)
	}
	r.Checkpoints = make([]ReplayCheckpoint, nCp)
	for i := range r.Checkpoints {
		cp := &r.Checkpoints[i]
		if cp.Tick, err = u32("checkpoint tick"); err != nil {
			return nil, err
		}
		if cp.Top, err = u64("checkpoint top"); err != nil {
			return nil, err
		}
		cp.Subs = make([]uint64, nSubs)
		for j := range cp.Subs {
			if cp.Subs[j], err = u64("checkpoint sub"); err != nil {
				return nil, err
			}
		}
	}
	if n, _ := rd.Read(scratch[:1]); n != 0 {
		return nil, fmt.Errorf("sim: replay: trailing bytes after checkpoint trace")
	}
	return r, nil
}

// CompareCheckpoint reports the first per-system divergence between a
// recorded checkpoint and a recomputed snapshot: ("", true) on match.
func CompareCheckpoint(cp *ReplayCheckpoint, snap *statehash.Snapshot) (culprit string, match bool) {
	if cp.Top == snap.Top {
		return "", true
	}
	for i := range cp.Subs {
		if i < len(snap.Subs) && cp.Subs[i] != snap.Subs[i] {
			return HashSystems[i], false
		}
	}
	return "top", false
}

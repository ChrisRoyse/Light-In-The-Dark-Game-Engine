package net

// turns.go: the command-turn pipeline (#65, D-2026-06-11-26). Commands are
// grouped into TURNS spanning 2–4 sim ticks (fixed per session). Each player
// submits its command records for a turn; the host aggregates every player's
// submission into ONE deterministic, (tick,playerID,seq)-sorted byte payload and
// broadcasts the identical bytes to all clients — the lockstep invariant: every
// peer consumes the exact same ordered command stream each turn (R-FSV-2).
//
// Records are opaque here: a record is the sim's fixed little-endian
// CommandRecord encoding (input.md §8), UNTOUCHED. The turn is a transport-level
// grouping that length-prefixes each record; the pipeline reads only the fixed
// 10-byte header (version u8, tick u32, player u8, seq u16, opcode u8, flags u8)
// to extract the sort key, so litd/net never imports litd/sim.
//
// Determinism note: TurnBuffer uses maps for the per-turn/per-player staging
// (host orchestration, not sim gameplay — the no-map rule is sim-scoped), but
// the AGGREGATE is explicitly sorted before encoding, so map iteration order
// never leaks into the broadcast bytes.

import (
	"encoding/binary"
	"fmt"
	"sort"
)

const (
	recHeaderSize = 10   // input.md §8 fixed CommandRecord header
	maxRecordWire = 512  // per-record byte cap (max real record ≈ 289 B); fail-closed alloc bound
	minTurnLen    = 2    // D-2026-06-11-26: turns span 2–4 ticks
	maxTurnLen    = 4
)

// recordKey extracts the (tick, player, seq) sort key from a record's fixed
// header. ok=false for a record too short to hold a header.
func recordKey(rec []byte) (tick uint32, player uint8, seq uint16, ok bool) {
	if len(rec) < recHeaderSize {
		return 0, 0, 0, false
	}
	tick = binary.LittleEndian.Uint32(rec[1:5])
	player = rec[5]
	seq = binary.LittleEndian.Uint16(rec[6:8])
	return tick, player, seq, true
}

// EncodeTurn serializes records (already in final order) into a turn payload:
// u32 count, then per record u16 len + bytes. A record over maxRecordWire is a
// fail-closed error. The result is suitable for Session.SendTurn.
func EncodeTurn(records [][]byte) ([]byte, error) {
	out := make([]byte, 4)
	binary.LittleEndian.PutUint32(out[:4], uint32(len(records)))
	for i, rec := range records {
		if len(rec) > maxRecordWire {
			return nil, fmt.Errorf("net: turn encode: record %d size %d > %d cap", i, len(rec), maxRecordWire)
		}
		var lp [2]byte
		binary.LittleEndian.PutUint16(lp[:], uint16(len(rec)))
		out = append(out, lp[:]...)
		out = append(out, rec...)
	}
	return out, nil
}

// DecodeTurn parses a turn payload back into its records (each a copy). It is
// the inverse of EncodeTurn and fails closed on truncation or an over-cap
// record length.
func DecodeTurn(payload []byte) ([][]byte, error) {
	if len(payload) < 4 {
		return nil, fmt.Errorf("net: turn decode: short header (%d bytes)", len(payload))
	}
	count := binary.LittleEndian.Uint32(payload[:4])
	off := 4
	out := make([][]byte, 0, count)
	for i := uint32(0); i < count; i++ {
		if off+2 > len(payload) {
			return nil, fmt.Errorf("net: turn decode: truncated length at record %d", i)
		}
		n := int(binary.LittleEndian.Uint16(payload[off : off+2]))
		off += 2
		if n > maxRecordWire {
			return nil, fmt.Errorf("net: turn decode: record %d length %d > %d cap", i, n, maxRecordWire)
		}
		if off+n > len(payload) {
			return nil, fmt.Errorf("net: turn decode: truncated body at record %d", i)
		}
		rec := make([]byte, n)
		copy(rec, payload[off:off+n])
		out = append(out, rec)
		off += n
	}
	if off != len(payload) {
		return nil, fmt.Errorf("net: turn decode: %d trailing bytes", len(payload)-off)
	}
	return out, nil
}

// TurnBuffer is the host-side aggregator. It collects each player's per-turn
// submissions and, once every expected player has submitted, produces the single
// sorted aggregate to broadcast. Not safe for concurrent use — drive it from the
// host's turn loop (one goroutine).
type TurnBuffer struct {
	turnLen   int
	players   []uint8
	playerSet map[uint8]bool
	subs      map[uint64]map[uint8][][]byte
	broadcast map[uint64]bool
}

// NewTurnBuffer creates a buffer for a session whose turns span turnLen ticks
// (must be in [2,4]) with the given player set (non-empty, no duplicates).
func NewTurnBuffer(turnLen int, players []uint8) (*TurnBuffer, error) {
	if turnLen < minTurnLen || turnLen > maxTurnLen {
		return nil, fmt.Errorf("net: turn length %d out of [%d,%d]", turnLen, minTurnLen, maxTurnLen)
	}
	if len(players) == 0 {
		return nil, fmt.Errorf("net: turn buffer needs at least one player")
	}
	set := make(map[uint8]bool, len(players))
	for _, p := range players {
		if set[p] {
			return nil, fmt.Errorf("net: duplicate player id %d", p)
		}
		set[p] = true
	}
	ps := append([]uint8(nil), players...)
	sort.Slice(ps, func(i, j int) bool { return ps[i] < ps[j] })
	return &TurnBuffer{
		turnLen:   turnLen,
		players:   ps,
		playerSet: set,
		subs:      make(map[uint64]map[uint8][][]byte),
		broadcast: make(map[uint64]bool),
	}, nil
}

// TurnLen is the fixed per-session turn length in ticks.
func (b *TurnBuffer) TurnLen() int { return b.turnLen }

// Submit records player's command records for turn. An empty slice is a valid
// (heartbeat) contribution — lockstep needs every player's submission each turn.
// Rejected, fail-closed: an unknown player, a duplicate submission for the same
// turn, a submission for an already-broadcast turn (late), a malformed record,
// or a record whose header player id != the submitting player (spoof).
func (b *TurnBuffer) Submit(turn uint64, player uint8, records [][]byte) error {
	if !b.playerSet[player] {
		return fmt.Errorf("net: submit: unknown player %d", player)
	}
	if b.broadcast[turn] {
		return fmt.Errorf("net: submit: turn %d already broadcast (late submission rejected)", turn)
	}
	for i, rec := range records {
		rp, _, _, ok := recHeaderPlayer(rec)
		if !ok {
			return fmt.Errorf("net: submit: turn %d player %d record %d malformed (%d bytes < %d header)", turn, player, i, len(rec), recHeaderSize)
		}
		if len(rec) > maxRecordWire {
			return fmt.Errorf("net: submit: turn %d record %d size %d > %d cap", turn, i, len(rec), maxRecordWire)
		}
		if rp != player {
			return fmt.Errorf("net: submit: turn %d player %d record %d claims player %d (spoof rejected)", turn, player, i, rp)
		}
	}
	pm := b.subs[turn]
	if pm == nil {
		pm = make(map[uint8][][]byte)
		b.subs[turn] = pm
	}
	if _, dup := pm[player]; dup {
		return fmt.Errorf("net: submit: turn %d player %d already submitted", turn, player)
	}
	cp := make([][]byte, len(records))
	for i, rec := range records {
		cp[i] = append([]byte(nil), rec...)
	}
	pm[player] = cp
	return nil
}

// recHeaderPlayer is recordKey reduced to the player id + validity.
func recHeaderPlayer(rec []byte) (player uint8, _ uint16, _ uint32, ok bool) {
	t, p, s, ok := recordKey(rec)
	return p, s, t, ok
}

// Ready reports whether every expected player has submitted for turn.
func (b *TurnBuffer) Ready(turn uint64) bool {
	return len(b.subs[turn]) == len(b.players)
}

// SubmittedCount reports how many players have submitted for turn (for
// queue-state inspection / heartbeat diagnostics).
func (b *TurnBuffer) SubmittedCount(turn uint64) int { return len(b.subs[turn]) }

// Aggregate produces the single broadcast payload for turn: all players' records
// sorted by (tick, playerID, seq), then EncodeTurn. It requires every player to
// have submitted and marks the turn broadcast (so a later Submit is rejected).
// Calling twice for the same turn is a fail-closed error.
func (b *TurnBuffer) Aggregate(turn uint64) ([]byte, error) {
	if b.broadcast[turn] {
		return nil, fmt.Errorf("net: aggregate: turn %d already broadcast", turn)
	}
	if !b.Ready(turn) {
		return nil, fmt.Errorf("net: aggregate: turn %d not ready (%d/%d players submitted)", turn, b.SubmittedCount(turn), len(b.players))
	}
	var all [][]byte
	for _, p := range b.players { // deterministic player order, then sort below
		all = append(all, b.subs[turn][p]...)
	}
	sort.SliceStable(all, func(i, j int) bool {
		ti, pi, si, _ := recordKey(all[i])
		tj, pj, sj, _ := recordKey(all[j])
		if ti != tj {
			return ti < tj
		}
		if pi != pj {
			return pi < pj
		}
		return si < sj
	})
	payload, err := EncodeTurn(all)
	if err != nil {
		return nil, err
	}
	b.broadcast[turn] = true
	delete(b.subs, turn) // staging no longer needed; frees memory
	return payload, nil
}

// Broadcast sends payload to every session (the same bytes to all — the lockstep
// guarantee). It returns the first send error, if any, after attempting all.
func Broadcast(payload []byte, sessions []*Session) error {
	var firstErr error
	for i, s := range sessions {
		if err := s.SendTurn(payload); err != nil && firstErr == nil {
			firstErr = fmt.Errorf("net: broadcast to session %d: %w", i, err)
		}
	}
	return firstErr
}

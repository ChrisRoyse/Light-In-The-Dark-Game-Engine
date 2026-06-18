package net

// lockstep.go: lockstep sim gating (#68, D-2026-06-11-5). Each peer runs the
// FULL sim; only command turns are exchanged. The invariant: the sim must not
// advance into the ticks of turn T until the aggregate for T has arrived — the
// gate blocks the tick driver otherwise. This is the host-loop wrapper around
// the deterministic core; litd/sim is untouched.
//
// The gate depends only on the SimDriver interface, so litd/net never imports
// litd/api or litd/sim (the net→sim direction stays closed; D-5 / import-graph).
// api.Game satisfies SimDriver via the netsync.go seam. The render layer keeps
// presenting (interpolating the last simulated state) while the gate blocks —
// that freeze lives in the client shell, above this gate.

import "fmt"

// SimDriver is the minimal tick-driver surface the gate needs. api.Game
// implements it. Tick() is the current tick; StageCommand stages one encoded
// record; IngestCommands moves staged→pending between ticks; Advance steps n
// whole ticks.
type SimDriver interface {
	Tick() uint32
	StageCommand(rec []byte) (bool, error)
	IngestCommands()
	Advance(ticks int)
}

// LockstepGate holds delivered turn aggregates and gates a SimDriver so it never
// executes a turn's ticks before that turn's aggregate is present. Not safe for
// concurrent use; drive it from the game loop.
type LockstepGate struct {
	turnLen int
	turns   map[uint64][][]byte // delivered turn → decoded records
	staged  map[uint64]bool     // turns whose records were staged into the driver
}

// NewLockstepGate builds a gate for a session whose turns span turnLen ticks
// (must be in [2,4], matching the turn pipeline).
func NewLockstepGate(turnLen int) (*LockstepGate, error) {
	if turnLen < minTurnLen || turnLen > maxTurnLen {
		return nil, fmt.Errorf("net: lockstep turn length %d out of [%d,%d]", turnLen, minTurnLen, maxTurnLen)
	}
	return &LockstepGate{
		turnLen: turnLen,
		turns:   make(map[uint64][][]byte),
		staged:  make(map[uint64]bool),
	}, nil
}

// Deliver decodes an aggregate payload (as produced by EncodeTurn / TurnBuffer)
// for turn and records it. Delivering the same turn twice is a fail-closed error
// (the aggregate is immutable once broadcast). Out-of-order delivery is fine —
// Pump only consumes contiguous turns.
func (g *LockstepGate) Deliver(turn uint64, payload []byte) error {
	if _, dup := g.turns[turn]; dup {
		return fmt.Errorf("net: lockstep: turn %d already delivered", turn)
	}
	recs, err := DecodeTurn(payload)
	if err != nil {
		return fmt.Errorf("net: lockstep: turn %d: %w", turn, err)
	}
	g.turns[turn] = recs
	return nil
}

// turnOfTick maps a 1-based tick to its 0-based turn. Tick t (≥1) belongs to
// turn (t-1)/turnLen: ticks 1..L → turn 0, L+1..2L → turn 1, …
func (g *LockstepGate) turnOfTick(tick uint32) uint64 {
	return uint64((tick - 1) / uint32(g.turnLen))
}

// isTurnStart reports whether tick is the first tick of its turn.
func (g *LockstepGate) isTurnStart(tick uint32) bool {
	return (tick-1)%uint32(g.turnLen) == 0
}

// Pump advances d through every tick whose covering turn's aggregate has been
// delivered, staging each turn's records at the turn's first tick, and stops at
// the boundary of the first undelivered turn. It returns the ticks advanced, the
// turn it is now waiting on, and whether it is blocked (true) or merely caught up
// to everything delivered. The sim NEVER enters a turn's ticks before that
// turn's aggregate is present.
func (g *LockstepGate) Pump(d SimDriver) (advanced int, waitingTurn uint64, blocked bool) {
	for {
		next := d.Tick() + 1
		turn := g.turnOfTick(next)
		recs, ok := g.turns[turn]
		if !ok {
			return advanced, turn, true // block at this turn's boundary
		}
		if g.isTurnStart(next) && !g.staged[turn] {
			for _, rec := range recs {
				if _, err := d.StageCommand(rec); err != nil {
					// A malformed record in a delivered aggregate is a hard fault;
					// surface by blocking rather than skipping (fail-closed).
					return advanced, turn, true
				}
			}
			g.staged[turn] = true
		}
		d.IngestCommands()
		d.Advance(1)
		advanced++
	}
}

// HighestContiguousTurn returns the highest turn T such that turns 0..T are all
// delivered (the furthest the gate could advance), and whether any turn is
// delivered at all.
func (g *LockstepGate) HighestContiguousTurn() (uint64, bool) {
	if _, ok := g.turns[0]; !ok {
		return 0, false
	}
	t := uint64(0)
	for {
		if _, ok := g.turns[t+1]; !ok {
			return t, true
		}
		t++
	}
}

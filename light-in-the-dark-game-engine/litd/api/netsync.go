package litd

// netsync.go: the thin api seam the lockstep layer (litd/net, #68) drives. It
// exposes the sim's existing command-staging + tick primitives without leaking
// sim types into exported signatures (records cross as encoded bytes, input.md
// §8). litd/net depends on an interface these satisfy, so litd/sim stays
// untouched and the net→sim direction never reverses (D-5 / import-graph check).

import (
	"fmt"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
)

// Tick is the current simulation tick (0 before the first Advance).
func (g *Game) Tick() uint32 {
	if g == nil || g.w == nil {
		return 0
	}
	return g.w.Tick()
}

// StageCommand decodes one encoded command record (input.md §8) and stages it
// for ingest on the next IngestCommands. Returns (false, nil) if the staging
// buffer is full (fail-closed, counted by the sim), and an error if the bytes
// are not a single well-formed record. The lockstep driver stages a turn's
// aggregated records before advancing into that turn's ticks.
func (g *Game) StageCommand(rec []byte) (bool, error) {
	if g == nil || g.w == nil {
		return false, fmt.Errorf("litd: StageCommand on a nil game")
	}
	var r sim.CommandRecord
	n, ok := sim.DecodeCommand(rec, &r)
	if !ok || n != len(rec) {
		return false, fmt.Errorf("litd: StageCommand: not a single well-formed record (%d bytes, decoded %d ok=%v)", len(rec), n, ok)
	}
	return g.w.StageCommand(r), nil
}

// IngestCommands moves staged records into the pending queue (stamping unassigned
// ticks, dropping already-simulated ones). The driver calls it BETWEEN ticks,
// never during one.
func (g *Game) IngestCommands() {
	if g == nil || g.w == nil {
		return
	}
	g.w.IngestStagedCommands()
}

// OnCommandRecord installs fn as the per-record consumption hook: it fires in
// phase 1 for every VALIDATED record the sim applies, with the tick it executed
// on and the record's identity (player, seq, opcode). nil clears it. This is the
// verification seam proving aggregated commands actually reach the sim at their
// scheduled tick. It does not expose sim types.
func (g *Game) OnCommandRecord(fn func(tick uint32, player uint8, seq uint16, opcode uint8)) {
	if g == nil || g.w == nil {
		return
	}
	if fn == nil {
		g.w.OnCommandRecord = nil
		return
	}
	g.w.OnCommandRecord = func(tick uint32, r *sim.CommandRecord, _ []sim.EntityID) {
		fn(tick, r.Player, r.Seq, r.Opcode)
	}
}

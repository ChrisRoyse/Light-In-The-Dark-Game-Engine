package melee

import (
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
)

// RecordingBridge wraps a melee.Bridge and records every state-changing call it
// makes into the production replay format (sim.ReplayCommand), WITHOUT changing
// behavior: each mutator appends one command stamped with the current sim tick
// (Bridge.Now()) and then delegates to the inner bridge, returning its result
// verbatim. All read methods come through the embedded Bridge unchanged, so a
// recorded match's sim state hashes identically to an unrecorded one. The
// recorded stream replays through sim.ReplayCommand.Apply with NO controller
// running, reproducing the match bit-for-bit — this promotes #398's proven
// mechanism into the real .litdreplay format (#404).
//
// Addressing: the economy kinds are player-addressed. Wave unit orders are
// addressed by entity id — a deterministic re-run reproduces identical entity
// ids, so the replay resolves index == id directly (the same leverage #398
// used). Coordinates are recorded as fixed-point bits using the SAME FromInt
// conversion the bridge applies (dpt/aiWU), so Apply rebuilds the identical
// point.
//
// Fidelity note: both OrderMoveTo and OrderAttackTo are realized by the sim
// bridges as a plain OrderMove toward the point (attack-move is a move that
// acquires on arrival — pursuit is #150/#380). So BOTH record as ReplayMove;
// recording OrderAttackTo as ReplayAttack would replay an OrderAttack and desync.
type RecordingBridge struct {
	Bridge
	out *[]sim.ReplayCommand
}

// NewRecordingBridge taps inner, appending each recorded command to *out.
func NewRecordingBridge(inner Bridge, out *[]sim.ReplayCommand) *RecordingBridge {
	return &RecordingBridge{Bridge: inner, out: out}
}

func (r *RecordingBridge) rec(c sim.ReplayCommand) { *r.out = append(*r.out, c) }

// --- ai.EconomyControl -----------------------------------------------------

func (r *RecordingBridge) AssignHarvest(player, resource, count int) int {
	r.rec(sim.ReplayCommand{
		Tick: r.Now(), Player: uint8(player), Kind: sim.ReplayHarvestAssign,
		Data: uint16(resource), Unit: uint32(count),
	})
	return r.Bridge.AssignHarvest(player, resource, count)
}

func (r *RecordingBridge) PlaceBuilding(player, typeID int, cx, cy int32) bool {
	r.rec(sim.ReplayCommand{
		Tick: r.Now(), Player: uint8(player), Kind: sim.ReplayPlaceBuilding,
		Data: uint16(typeID), X: int64(fixed.FromInt(cx)), Y: int64(fixed.FromInt(cy)),
	})
	return r.Bridge.PlaceBuilding(player, typeID, cx, cy)
}

// --- ai.ProductionControl --------------------------------------------------

func (r *RecordingBridge) TrainForPlayer(player, typeID int) (int, int) {
	r.rec(sim.ReplayCommand{
		Tick: r.Now(), Player: uint8(player), Kind: sim.ReplayTrain, Data: uint16(typeID),
	})
	return r.Bridge.TrainForPlayer(player, typeID)
}

// --- ai.WaveSource ---------------------------------------------------------

func (r *RecordingBridge) OrderMoveTo(id, x, y int32) {
	r.rec(sim.ReplayCommand{
		Tick: r.Now(), Kind: sim.ReplayMove, Unit: uint32(id), Target: sim.NoRosterRef,
		X: int64(fixed.FromInt(x)), Y: int64(fixed.FromInt(y)),
	})
	r.Bridge.OrderMoveTo(id, x, y)
}

func (r *RecordingBridge) OrderAttackTo(id, x, y int32) {
	// Realized as a move by the sim bridges — record as ReplayMove (see type doc).
	r.rec(sim.ReplayCommand{
		Tick: r.Now(), Kind: sim.ReplayMove, Unit: uint32(id), Target: sim.NoRosterRef,
		X: int64(fixed.FromInt(x)), Y: int64(fixed.FromInt(y)),
	})
	r.Bridge.OrderAttackTo(id, x, y)
}

// EntityResolver is the replay-side address resolver for a RecordingBridge
// stream: wave unit orders were recorded by entity id, so a fresh deterministic
// re-run resolves index == EntityID(idx) when that entity is alive. Pass this to
// sim.ReplayCommand.Apply when replaying a recorded AI match with no controller.
func EntityResolver(w *sim.World) func(idx uint32) (sim.EntityID, bool) {
	return func(idx uint32) (sim.EntityID, bool) {
		e := sim.EntityID(idx)
		if w.Ents.Alive(e) {
			return e, true
		}
		return 0, false
	}
}

package litd

// Unit production surface (#527). Training is a sim queue command, not a unit
// Order (litd/sim/replay.go), so it needs its own public verb rather than
// riding the order catalog. The deterministic source of truth is the sim's
// production queue, resolved in the produce pass; this method only stages the
// request and reports whether the sim accepted it.

import (
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
)

// Train enqueues production of typ at the producer unit u — a building whose
// unit type lists typ in its Trains set. It returns whether the sim accepted
// the order. A refusal (invalid producer, unknown/untrainable type, or a
// cost / food / tech / queue-capacity limit) returns false and the sim emits
// EventTrainRefused carrying the reason. No-op returning false for an invalid
// handle or a null type (fail closed). When the order is accepted the unit
// spawns after the type's train time, emitting EventUnitTrained.
// JASS: IssueTrainOrderByIdBJ
func (u Unit) Train(typ UnitType) bool {
	if !u.Valid() {
		u.g.reportInvalid("Unit.Train")
		return false
	}
	if typ.IsZero() {
		u.g.reportInvalid("Unit.Train")
		return false
	}
	return u.g.w.EnqueueTrain(u.id, typ.ref-1) == sim.TrainOK
}

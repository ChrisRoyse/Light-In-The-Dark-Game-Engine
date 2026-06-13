package litd

import "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"

// order.go is the order-issuing surface (jass-mapping/units.md). WC3 exposes
// orders through three native families keyed by target shape —
// IssueImmediateOrder/…ById (no target), IssuePointOrder/…Loc/…ById (a point),
// IssueTargetOrder/…ById (a widget) — times a string-vs-id twin for every one.
// That whole matrix collapses (D3) onto a single typed verb: Unit.Order(ord,
// target), where the order verb is a value from the catalog below and the
// target is an OrderTarget sum (none | point | unit). One verb, one target
// value — the string/id and immediate/point/target splits disappear.
//
// The catalog entries wrap the sim order kinds (litd/sim/store_order.go,
// orders_behavior.go). Order.id is the sim kind + 1, so the zero Order is the
// unset order (IsZero) and never aliases OrderStop (sim kind 0).

// Order verb catalog — the named orders issuable through Unit.Order. Each is the
// deduped landing point for its JASS order-string/id family.
//
// Target shape (enforced by intent, not by the sim at issue time): Stop and
// Hold take no target; Move, Smart, and Patrol take a point; Attack and Follow
// take a unit (Attack also accepts a point for attack-move). Casting, harvest,
// pickup, and build have their own dedicated verbs and are not issued here.
var (
	// OrderStop halts the unit and drops it to its auto-acquire stance.
	// JASS: "stop" (IssueImmediateOrder).
	OrderStop = Order{id: uint16(sim.OrderStop) + 1}
	// OrderMove walks the unit to a point. JASS: "move" (IssuePointOrder).
	OrderMove = Order{id: uint16(sim.OrderMove) + 1}
	// OrderAttack attacks a unit, or attack-moves to a point. JASS: "attack".
	OrderAttack = Order{id: uint16(sim.OrderAttack) + 1}
	// OrderSmart is the right-click order, resolved by the smart-order table.
	// JASS: "smart" (IssuePointOrder/IssueTargetOrder).
	OrderSmart = Order{id: uint16(sim.OrderSmart) + 1}
	// OrderHold holds position without pursuing. JASS: "holdposition".
	OrderHold = Order{id: uint16(sim.OrderHold) + 1}
	// OrderPatrol patrols between the current position and a point.
	// JASS: "patrol".
	OrderPatrol = Order{id: uint16(sim.OrderPatrol) + 1}
	// OrderFollow follows a unit. JASS: "smart" on a friendly target / "follow".
	OrderFollow = Order{id: uint16(sim.OrderFollow) + 1}
)

// targetKind tags the OrderTarget variant.
type targetKind uint8

const (
	targetNone  targetKind = 0
	targetPoint targetKind = 1
	targetUnit  targetKind = 2
)

// OrderTarget is the target of an order — the sum type that replaces WC3's
// immediate/point/target native split (D3). Build one with TargetNone,
// TargetPoint, or TargetUnit. The zero value is the no-target variant.
type OrderTarget struct {
	kind  targetKind
	point Vec2
	unit  Unit
}

// TargetNone is the empty target — for immediate orders (Stop, Hold).
func TargetNone() OrderTarget { return OrderTarget{} }

// TargetPoint targets a world point — for point orders (Move, Patrol,
// attack-move).
func TargetPoint(p Vec2) OrderTarget {
	return OrderTarget{kind: targetPoint, point: p}
}

// TargetUnit targets a unit — for target orders (Attack a hostile, Follow a
// friendly). A value constructor: it packages the handle into an order target
// value, it does not mutate (R-API-1).
func TargetUnit(u Unit) OrderTarget {
	return OrderTarget{kind: targetUnit, unit: u}
}

// Order issues ord to the unit with the given target, replacing any current
// order (the unqueued IssueXxxOrder semantics). Returns true if the order was
// installed, false on an invalid handle, an unset order, a dead unit target, or
// a unit with no order capability. JASS: the IssueImmediateOrder /
// IssuePointOrder / IssueTargetOrder families (and their …ById and …Loc twins)
// all collapse here (D3).
//
// Fail-closed: a target unit that is already invalid/dead makes the call a
// no-op returning false rather than issuing an order against a stale entity.
func (u Unit) Order(ord Order, target OrderTarget) bool {
	if !u.Valid() {
		u.g.reportInvalid("Unit.Order")
		return false
	}
	if ord.IsZero() {
		u.g.reportInvalid("Unit.Order (unset order)")
		return false
	}
	o := sim.Order{Kind: uint8(ord.id - 1)}
	switch target.kind {
	case targetPoint:
		o.Point = vec(target.point)
	case targetUnit:
		if !target.unit.Valid() {
			u.g.reportInvalid("Unit.Order (dead target unit)")
			return false
		}
		o.Target = target.unit.id
	}
	return u.g.w.IssueOrder(u.id, o, false)
}

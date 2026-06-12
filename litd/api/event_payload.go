package litd

// Event payloads (public-api-design.md §2 row 12, §3.4 R-API-4). The
// JASS event/eventid handles and every Get* event-context native
// collapse onto methods of one plain value type. Accessors on a payload
// of the wrong event kind return zero-value handles (R-API-5), so a
// chain off an unrelated event degrades safely instead of returning a
// foreign entity.

import (
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
)

// EventKind names a dispatchable event type (naming-and-style.md N-8:
// Event + noun + state verb, no JASS prefix). The public kinds are a
// stable ABI mapped onto the sim's internal numbering by simKindOf.
type EventKind uint16

const (
	// EventUnitDeath fires when a unit dies. Unit() is the dying unit;
	// KillingUnit() is the killer (zero on an environmental death).
	EventUnitDeath EventKind = iota + 1
	// EventUnitDamaged fires once per applied damage packet. Unit() is
	// the damaged unit, Source() the attacker, Damage() the amount.
	EventUnitDamaged
	// EventOrderIssued fires when an order becomes a unit's current
	// order. Unit() is the ordered unit, Target() the order target.
	EventOrderIssued
	// EventOrderDone fires when a unit's current order completes.
	EventOrderDone
	// EventUnitTrained fires when a production order finishes a unit.
	EventUnitTrained
	// EventResearchFinished fires when an upgrade completes.
	EventResearchFinished
	// EventHeroLevel fires when a hero gains a level.
	EventHeroLevel
	// EventItemPickedUp fires when a unit picks up an item.
	EventItemPickedUp
	// EventConstructFinished fires when a building completes
	// construction.
	EventConstructFinished
	// EventMissileImpact fires when a missile delivers its payload.
	// Missile() is the missile; Unit() is the struck unit (zero on a
	// point/AoE detonation).
	EventMissileImpact
	// EventMissileExpired fires when a missile dies without delivering.
	// Missile() is the missile.
	EventMissileExpired
	// EventVictory fires when a player first reaches ResultWon.
	// Player() is the winning player.
	EventVictory
	// EventDefeat fires when a player first reaches ResultLost or
	// ResultLeft. Player() is the defeated/left player.
	EventDefeat
)

// simKindOf maps each public event kind to its sim event kind. The map
// is read only at OnEvent registration time, never on the dispatch
// path. A kind absent here is unknown and OnEvent rejects it
// (fail-closed).
var simKindOf = map[EventKind]uint16{
	EventUnitDeath:         1,  // sim.EvUnitDeath
	EventUnitDamaged:       7,  // sim.EvUnitDamaged
	EventOrderIssued:       4,  // sim.EvOrderIssued
	EventOrderDone:         5,  // sim.EvOrderDone
	EventUnitTrained:       11, // sim.EvUnitTrained
	EventResearchFinished:  13, // sim.EvResearchFinished
	EventHeroLevel:         14, // sim.EvHeroLevel
	EventItemPickedUp:      16, // sim.EvItemPickedUp
	EventConstructFinished: 20, // sim.EvConstructFinished
	EventMissileImpact:     22, // sim.EvMissileImpact
	EventMissileExpired:    23, // sim.EvMissileExpired
	EventVictory:           sim.EvVictory,
	EventDefeat:            sim.EvDefeat,
}

// Event is the payload handed to an OnEvent handler — a plain value
// (no allocation per dispatch). It carries the firing kind, the two
// participating entities, the scalar argument, and a game back-pointer
// so its context accessors can resolve nouns.
type Event struct {
	kind EventKind
	src  sim.EntityID
	dst  sim.EntityID
	arg  int64
	g    *Game
}

// Kind returns the event kind.
func (e Event) Kind() EventKind { return e.kind }

// IsZero reports whether this is the zero-value event.
func (e Event) IsZero() bool { return e == Event{} }

// primary returns the entity the event is "about" — the subject used by
// Unit() and by ForPlayer scoping. For a damage event that is the
// damaged unit (Dst); for every other kind it is the source (Src).
func (e Event) primary() sim.EntityID {
	if e.kind == EventUnitDamaged || e.kind == EventMissileImpact {
		return e.dst
	}
	return e.src
}

// playerSlot returns the player slot carried by a player-scoped event,
// or -1 when the event is not about a player slot.
func (e Event) playerSlot() int32 {
	switch e.kind {
	case EventVictory, EventDefeat:
		if e.arg >= 0 && e.arg < sim.MaxPlayers {
			return int32(e.arg)
		}
	}
	return -1
}

// Missile returns the missile on a missile event, else the zero
// Missile.
func (e Event) Missile() Missile {
	switch e.kind {
	case EventMissileImpact, EventMissileExpired:
		return Missile{id: e.src, g: e.g}
	}
	return Missile{}
}

// Unit returns the event's primary unit (the dying unit, the damaged
// unit, the ordered unit, …), or the zero Unit on an unrelated kind.
// JASS: GetTriggerUnit.
func (e Event) Unit() Unit { return Unit{id: e.primary(), g: e.g} }

// KillingUnit returns the killer on a death event, else the zero Unit.
// JASS: GetKillingUnit.
func (e Event) KillingUnit() Unit {
	if e.kind != EventUnitDeath {
		return Unit{}
	}
	return Unit{id: e.dst, g: e.g}
}

// Source returns the attacker on a damage event, else the zero Unit.
// JASS: GetEventDamageSource.
func (e Event) Source() Unit {
	if e.kind != EventUnitDamaged {
		return Unit{}
	}
	return Unit{id: e.src, g: e.g}
}

// Target returns the order target on an order event, else the zero
// Unit. JASS: GetOrderTargetUnit.
func (e Event) Target() Unit {
	if e.kind != EventOrderIssued {
		return Unit{}
	}
	return Unit{id: e.dst, g: e.g}
}

// Damage returns the damage amount on a damage event, else 0. JASS:
// GetEventDamage.
func (e Event) Damage() float64 {
	if e.kind != EventUnitDamaged {
		return 0
	}
	return toFloat(fixed.F64(e.arg))
}

// Player returns the player carried by a player-scoped event, else the
// zero Player on an unrelated kind. JASS: GetTriggerPlayer for the
// victory/defeat collapse.
func (e Event) Player() Player {
	if e.playerSlot() < 0 {
		return Player{}
	}
	return Player{idx: e.playerSlot(), g: e.g}
}

// EventView is the read-only payload handed to a filter
// (public-api-design.md §3.4, execution-model.md §4). It is a
// pointerless value carrying only precomputed read data: no Game
// reference, no entity handle, and therefore no mutating verb or wait
// is reachable from a filter — purity is enforced by the type the
// filter is handed, not by convention.
type EventView struct {
	kind        EventKind
	damage      float64
	ownerPlayer int32 // owner slot of the primary unit, -1 if none
}

// Kind returns the event kind.
func (v EventView) Kind() EventKind { return v.kind }

// Damage returns the damage amount (0 for non-damage events).
func (v EventView) Damage() float64 { return v.damage }

// OwnerPlayer returns the player slot owning the primary unit, or -1 if
// the primary unit has no owner — enough for the common
// "is this my unit?" filter without exposing a mutable handle.
func (v EventView) OwnerPlayer() int { return int(v.ownerPlayer) }

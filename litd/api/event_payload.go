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
	// EventRegionEnter fires when a unit enters a region's cells. Unit()
	// is the entering unit; Region() is the region.
	EventRegionEnter
	// EventRegionLeave fires when a unit leaves a region's cells (or dies
	// inside it). Unit() is the leaving unit; Region() is the region.
	EventRegionLeave
	// EventOrderDropped fires when a unit's queued order is discarded
	// without completing. Unit() is the unit.
	EventOrderDropped
	// EventBuffExpired fires when a buff instance times out. Unit() is the
	// buffed unit.
	EventBuffExpired
	// EventResourceDeposited fires when a harvester delivers resources to
	// a depot. Unit() is the harvester.
	EventResourceDeposited
	// EventResourceDepleted fires when a resource node is exhausted.
	// Unit() is the node entity.
	EventResourceDepleted
	// EventTrainRefused fires when a production order is refused (cost,
	// food, or requirement gate). Unit() is the would-be trainer.
	EventTrainRefused
	// EventHeroDied fires when a hero dies (in addition to EventUnitDeath).
	// Unit() is the hero.
	EventHeroDied
	// EventItemUsed fires when a unit uses (activates) a carried item.
	// Unit() is the user.
	EventItemUsed
	// EventItemDropped fires when a unit drops an item. Unit() is the
	// dropping unit.
	EventItemDropped
	// EventConstructStarted fires when a building begins construction.
	// Unit() is the building.
	EventConstructStarted
	// EventConstructCancelled fires when construction is cancelled before
	// completing. Unit() is the building.
	EventConstructCancelled
	// EventAbilityCast fires when a unit commits a cast (enters cast point).
	// Unit() is the caster, Target() the cast target, Ability() the ability ref.
	EventAbilityCast
	// EventAbilityEffect fires at the EFFECT edge, when the ability's effect
	// composition runs. Unit() is the caster, Target() the target.
	EventAbilityEffect
	// EventAbilityChannelStart fires when a channeled ability enters its channel.
	// Unit() is the caster.
	EventAbilityChannelStart
	// EventAbilityChannelStop fires when a channel ends (into backswing). Unit()
	// is the caster.
	EventAbilityChannelStop
	// EventAbilityFinish fires when a cast completes normally (returns to ready).
	// Unit() is the caster.
	EventAbilityFinish
	// EventAbilityStopped fires when a cast is interrupted before finishing.
	// Unit() is the caster.
	EventAbilityStopped
	// EventAttackLaunch fires at a weapon's FIRE edge. Unit() is the attacker,
	// Target() the target.
	EventAttackLaunch
	// EventAttackLanded fires when a weapon-sourced packet lands on a live
	// target, immediately before that hit's EventUnitDamaged. Unit() is the
	// attacker, Target() the victim, Damage() the post-mitigation amount.
	EventAttackLanded
	// EventBuffApplied fires when a new buff instance attaches. Unit() is the
	// buffed unit, Source() the applier.
	EventBuffApplied
	// EventBuffRefreshed fires when an existing buff instance is refreshed or
	// restacked. Unit() is the buffed unit, Source() the applier.
	EventBuffRefreshed
)

// simKindOf maps each public event kind to its sim event kind. The map
// is read only at OnEvent registration time, never on the dispatch
// path. A kind absent here is unknown and OnEvent rejects it
// (fail-closed).
var simKindOf = map[EventKind]uint16{
	EventUnitDeath:           1,  // sim.EvUnitDeath
	EventUnitDamaged:         7,  // sim.EvUnitDamaged
	EventOrderIssued:         4,  // sim.EvOrderIssued
	EventOrderDone:           5,  // sim.EvOrderDone
	EventUnitTrained:         11, // sim.EvUnitTrained
	EventResearchFinished:    13, // sim.EvResearchFinished
	EventHeroLevel:           14, // sim.EvHeroLevel
	EventItemPickedUp:        16, // sim.EvItemPickedUp
	EventConstructFinished:   20, // sim.EvConstructFinished
	EventMissileImpact:       22, // sim.EvMissileImpact
	EventMissileExpired:      23, // sim.EvMissileExpired
	EventVictory:             sim.EvVictory,
	EventDefeat:              sim.EvDefeat,
	EventRegionEnter:         sim.EvRegionEnter,
	EventRegionLeave:         sim.EvRegionLeave,
	EventOrderDropped:        sim.EvOrderDropped,
	EventBuffExpired:         sim.EvBuffExpired,
	EventResourceDeposited:   sim.EvResourceDeposited,
	EventResourceDepleted:    sim.EvResourceDepleted,
	EventTrainRefused:        sim.EvTrainRefused,
	EventHeroDied:            sim.EvHeroDied,
	EventItemUsed:            sim.EvItemUsed,
	EventItemDropped:         sim.EvItemDropped,
	EventConstructStarted:    sim.EvConstructStarted,
	EventConstructCancelled:  sim.EvConstructCancelled,
	EventAbilityCast:         sim.EvAbilityCast,
	EventAbilityEffect:       sim.EvAbilityEffect,
	EventAbilityChannelStart: sim.EvAbilityChannelStart,
	EventAbilityChannelStop:  sim.EvAbilityChannelStop,
	EventAbilityFinish:       sim.EvAbilityFinish,
	EventAbilityStopped:      sim.EvAbilityStopped,
	EventAttackLaunch:        sim.EvAttackLaunch,
	EventAttackLanded:        sim.EvAttackLanded,
	EventBuffApplied:         sim.EvBuffApplied,
	EventBuffRefreshed:       sim.EvBuffRefreshed,
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
	sub  *subscription // the firing subscription (Event.Subscription)
}

// Kind returns the event kind.
func (e Event) Kind() EventKind { return e.kind }

// IsZero reports whether this is the zero-value event.
func (e Event) IsZero() bool { return e == Event{} }

// primary returns the entity the event is "about" — the subject used by
// Unit() and by ForPlayer scoping. For a damage event that is the
// damaged unit (Dst); for every other kind it is the source (Src).
func (e Event) primary() sim.EntityID {
	switch e.kind {
	case EventUnitDamaged, EventMissileImpact,
		EventBuffApplied, EventBuffRefreshed:
		// the buffed unit is the target (Dst); the applier is the Source.
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
// JASS: GetBuyingUnit, GetChangingUnit, GetConstructedStructure, GetConstructingStructure, GetDyingUnit, GetEnteringUnit, GetEnumUnit, GetFilterUnit, GetLearningUnit, GetLeavingUnit, GetLevelingUnit, GetOrderedUnit, GetResearchingUnit, GetRevivableUnit, GetSellingUnit, GetSpellAbilityUnit, GetSummoningUnit, GetTrainedUnit, GetTransportUnit, GetTriggerUnit
func (e Event) Unit() Unit { return Unit{id: e.primary(), g: e.g} }

// KillingUnit returns the killer on a death event, else the zero Unit.
// JASS: GetKillingUnit
func (e Event) KillingUnit() Unit {
	if e.kind != EventUnitDeath {
		return Unit{}
	}
	return Unit{id: e.dst, g: e.g}
}

// Source returns the attacker on a damage event, else the zero Unit.
// JASS: GetAttacker, GetEventDamageSource
func (e Event) Source() Unit {
	switch e.kind {
	case EventUnitDamaged,
		EventAttackLaunch, EventAttackLanded,
		EventBuffApplied, EventBuffRefreshed:
		return Unit{id: e.src, g: e.g}
	}
	return Unit{}
}

// Target returns the order target on an order event, else the zero
// Unit. JASS: GetOrderTargetUnit.
// JASS: GetEventTargetUnit, GetOrderTargetUnit, GetSpellTargetUnit
func (e Event) Target() Unit {
	switch e.kind {
	case EventOrderIssued,
		EventAbilityCast, EventAbilityEffect, EventAbilityChannelStart,
		EventAbilityChannelStop, EventAbilityFinish,
		EventAttackLaunch, EventAttackLanded:
		return Unit{id: e.dst, g: e.g}
	}
	return Unit{}
}

// Ability returns the ability ref on an ability-lifecycle event, else the zero
// (invalid) ref. Valid for EventAbility* kinds, where Arg carries the ref.
func (e Event) Ability() AbilityRef {
	switch e.kind {
	case EventAbilityCast, EventAbilityEffect, EventAbilityChannelStart,
		EventAbilityChannelStop, EventAbilityFinish, EventAbilityStopped:
		return AbilityRef(uint16(e.arg))
	}
	return 0
}

// Damage returns the damage amount on a damage event, else 0. JASS:
// GetEventDamage.
// JASS: GetEventDamage
func (e Event) Damage() float64 {
	switch e.kind {
	case EventUnitDamaged, EventAttackLanded:
		return toFloat(fixed.F64(e.arg))
	}
	return 0
}

// Region returns the region on a region enter/leave event, else the zero
// Region. The handle is rebuilt from the packed (id, generation) arg, so
// it is Valid only while the region still exists. JASS:
// GetTriggeringRegion.
// JASS: GetTriggeringRegion
func (e Event) Region() Region {
	switch e.kind {
	case EventRegionEnter, EventRegionLeave:
		return Region{id: uint32(e.arg), gen: uint32(e.arg >> 32), g: e.g}
	}
	return Region{}
}

// Subscription returns the registration that is currently firing this
// handler — the capability behind JASS GetTriggeringTrigger, letting a
// handler cancel itself (e.Subscription().Cancel()) or pass its own
// registration on. Zero-value Subscription outside a dispatch.
// JASS: GetTriggeringTrigger
func (e Event) Subscription() Subscription { return Subscription{s: e.sub} }

// Player returns the player carried by a player-scoped event, else the
// zero Player on an unrelated kind. JASS: GetTriggerPlayer for the
// victory/defeat collapse.
// JASS: GetTriggerPlayer
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

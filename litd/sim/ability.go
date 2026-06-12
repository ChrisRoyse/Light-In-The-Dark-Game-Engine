package sim

// Ability cast machine (#160, combat-and-orders.md §5.1, revised per
// ADR #294): abilities are DATA — a cast block (cost, clocks, range)
// plus a compiled effect composition (#296). This file implements the
// shared state machine; zero per-ability Go exists anywhere.
//
//	READY → CASTPOINT → EFFECT edge → [CHANNEL] → BACKSWING → READY
//
// All clocks absolute ticks. Mana is spent ENTERING castpoint and
// refunded if the castpoint is interrupted (cooldown untouched); once
// the EFFECT edge runs, cost and cooldown are committed — a channel
// or backswing interrupt keeps both. The cooldown clock starts at the
// EFFECT edge (ReadyAt = effectTick + cooldown).
//
// Driven in phase 5 BEFORE the attack system: a unit ordered to cast
// is casting, not autoattacking. The OrderCastAbility head is the
// authority — replacing it is the interrupt edge (stuns route through
// the same edge when #162 lands).

import (
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
)

// castStateNames renders traces (FSV SoT).
var castStateNames = [...]string{"ready", "precast", "castpoint", "channel", "backswing", "cooldown"}

// CastStateName returns the human name of a Cast* state.
func CastStateName(s uint8) string {
	if int(s) < len(castStateNames) {
		return castStateNames[s]
	}
	return "?"
}

// BindAbilityDefs installs the loaded ability rows. Refs in
// AbilityStore.AbilityID are defIndex+1 into this slice. Fail-closed
// on a set too large for the uint16 ref space.
func (w *World) BindAbilityDefs(defs []data.Ability) bool {
	if len(defs) >= 1<<16-1 {
		return false
	}
	w.abilityDefs = defs
	return true
}

// SetAbility fills a slot with a bound ability ref. Tests and the
// (future, #217) data spawner use it.
func (w *World) SetAbility(id EntityID, slot int, defIndex int) bool {
	ar := w.Abilities.Row(id)
	if ar == -1 || slot < 0 || slot >= AbilitySlots ||
		defIndex < 0 || defIndex >= len(w.abilityDefs) {
		return false
	}
	w.Abilities.AbilityID[ar][slot] = uint16(defIndex + 1)
	w.Abilities.CastState[ar][slot] = CastReady
	w.Abilities.ReadyAt[ar][slot] = 0
	return true
}

// castTransition flips a slot's cast state and reports it.
func (w *World) castTransition(id EntityID, ar int32, slot int, to uint8) {
	from := w.Abilities.CastState[ar][slot]
	if from == to {
		return
	}
	w.Abilities.CastState[ar][slot] = to
	if w.OnCastTransition != nil {
		w.OnCastTransition(w.tick, id, slot, from, to)
	}
}

// abilitySystem ticks mana regen for every ability row, then drives
// casts. Phase 5, before acquisition/attack.
func (w *World) abilitySystem() {
	a := w.Abilities
	for ar := int32(0); ar < a.count; ar++ {
		if a.ManaRegen[ar] > 0 && a.Mana[ar] < a.MaxMana[ar] {
			m := a.Mana[ar].Add(a.ManaRegen[ar])
			if m > a.MaxMana[ar] {
				m = a.MaxMana[ar]
			}
			a.Mana[ar] = m
		}
		w.driveCast(ar)
	}
}

// driveCast advances one unit's cast machine.
func (w *World) driveCast(ar int32) {
	a := w.Abilities
	id := a.Entity[ar]
	or := w.Orders.Row(id)
	casting := or != -1 && w.Orders.Kind[or] == OrderCastAbility

	// interrupt edge: an active cast whose order head is gone
	if slot := a.CastSlot[ar]; slot != -1 {
		ref := a.AbilityID[ar][slot]
		if !casting || int(w.Orders.Data[or]) != int(ref) {
			w.cancelCast(ar, id, int(slot))
			return
		}
		w.advanceCast(ar, id, int(slot), or)
		return
	}
	if !casting {
		return
	}

	// new cast attempt from the order head
	ref := w.Orders.Data[or]
	def, slot := w.castableSlot(ar, ref)
	if def == nil {
		w.completeOrder(or, id, false) // unknown/unequipped ability
		return
	}
	if !CooldownReady(w.tick, a.ReadyAt[ar][slot]) {
		w.completeOrder(or, id, false) // on cooldown: deterministic refusal
		return
	}
	if a.Mana[ar] < fixed.FromInt(def.ManaCost) {
		w.completeOrder(or, id, false) // insufficient mana
		return
	}
	// range gate: walk into cast range first (the #150 chase pattern)
	if def.CastRange > 0 && w.Orders.Target[or] != 0 {
		tgt := w.Orders.Target[or]
		if !w.Ents.Alive(tgt) {
			w.completeOrder(or, id, false)
			return
		}
		tr, ttr := w.Transforms.Row(id), w.Transforms.Row(tgt)
		if tr == -1 || ttr == -1 {
			w.completeOrder(or, id, false)
			return
		}
		rHi, rLo := fixed.RadiusSq(def.CastRange)
		dHi, dLo := fixed.DistSq(w.Transforms.Pos[tr], w.Transforms.Pos[ttr])
		if dHi > rHi || (dHi == rHi && dLo > rLo) {
			w.StartMoveTo(id, w.Transforms.Pos[ttr])
			return // not in range yet: keep walking, order stays fresh
		}
		if mr := w.Movements.Row(id); mr != -1 && w.Movements.State[mr] == MoveFollowing {
			w.Movements.State[mr] = MoveIdle // feet halt at cast range
		}
	}
	// commit: spend mana, enter castpoint
	a.Mana[ar] = a.Mana[ar].Sub(fixed.FromInt(def.ManaCost))
	a.CastSlot[ar] = int8(slot)
	a.CastEnd[ar] = w.tick + uint32(def.CastPointTicks)
	w.castTransition(id, ar, slot, CastPoint)
	if def.CastPointTicks == 0 {
		w.advanceCast(ar, id, slot, or) // instant cast point fires this tick
	}
}

// advanceCast moves an in-flight cast through its phase clocks.
func (w *World) advanceCast(ar int32, id EntityID, slot int, or int32) {
	a := w.Abilities
	if !CooldownReady(w.tick, a.CastEnd[ar]) {
		return // current phase still running
	}
	def := &w.abilityDefs[a.AbilityID[ar][slot]-1]
	switch a.CastState[ar][slot] {
	case CastPoint:
		// EFFECT edge: composition fires, cooldown commits
		w.ExecuteEffects(def.Effects, EffectCtx{
			Source: id,
			Target: w.Orders.Target[or],
			Point:  w.Orders.Point[or],
		})
		a.ReadyAt[ar][slot] = w.tick + uint32(def.CooldownTicks)
		if def.ChannelTicks > 0 {
			a.CastEnd[ar] = w.tick + uint32(def.ChannelTicks)
			w.castTransition(id, ar, slot, CastChannel)
			return
		}
		a.CastEnd[ar] = w.tick + uint32(def.BackswingTicks)
		w.castTransition(id, ar, slot, CastBackswing)
		if def.BackswingTicks == 0 {
			w.finishCast(ar, id, slot, or)
		}
	case CastChannel:
		a.CastEnd[ar] = w.tick + uint32(def.BackswingTicks)
		w.castTransition(id, ar, slot, CastBackswing)
		if def.BackswingTicks == 0 {
			w.finishCast(ar, id, slot, or)
		}
	case CastBackswing:
		w.finishCast(ar, id, slot, or)
	}
}

// finishCast returns the slot to READY and completes the order.
func (w *World) finishCast(ar int32, id EntityID, slot int, or int32) {
	w.Abilities.CastSlot[ar] = -1
	w.castTransition(id, ar, slot, CastReady)
	w.completeOrder(or, id, true)
}

// cancelCast is the interrupt edge. Castpoint cancel refunds mana and
// leaves the cooldown clock untouched; channel/backswing cancels keep
// both (the EFFECT edge already committed them).
func (w *World) cancelCast(ar int32, id EntityID, slot int) {
	a := w.Abilities
	if a.CastState[ar][slot] == CastPoint {
		def := &w.abilityDefs[a.AbilityID[ar][slot]-1]
		m := a.Mana[ar].Add(fixed.FromInt(def.ManaCost))
		if m > a.MaxMana[ar] {
			m = a.MaxMana[ar]
		}
		a.Mana[ar] = m
	}
	a.CastSlot[ar] = -1
	w.castTransition(id, ar, slot, CastReady)
}

// castableSlot finds the slot equipped with ref. nil = not equipped.
func (w *World) castableSlot(ar int32, ref uint16) (*data.Ability, int) {
	if ref == 0 || int(ref) > len(w.abilityDefs) {
		return nil, -1
	}
	for s := 0; s < AbilitySlots; s++ {
		if w.Abilities.AbilityID[ar][s] == ref {
			return &w.abilityDefs[ref-1], s
		}
	}
	return nil, -1
}

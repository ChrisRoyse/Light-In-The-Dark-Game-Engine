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

const (
	maxAbilityDefs             = 1<<16 - 1 // ref 0 is empty; refs 1..65535 are valid
	maxRuntimeAbilityStringLen = 256
	maxRuntimeAbilityManaCost  = 1 << 20
)

// Ability-lifecycle events (#467). The cast machine emitted nothing before;
// triggers observe spells through these. Ids 29–34 are the next free kind
// values after EvPeriodic(28) — distinct kinds, no #332 collision. The emit
// order within a single cast is deterministic (Cast → [Effect] → [ChannelStart
// → ChannelStop] → Finish, or Stopped on interrupt). Payload convention:
// Src = caster, Dst = target (0 if none), Arg = ability ref. WC3 parity:
// EVENT_UNIT_SPELL_CAST/EFFECT/CHANNEL/ENDCAST/FINISH (see #466 coverage).
const (
	EvAbilityCast         uint16 = 29 // cast committed: entered castpoint
	EvAbilityEffect       uint16 = 30 // EFFECT edge: the effect composition fired
	EvAbilityChannelStart uint16 = 31 // entered the channel phase
	EvAbilityChannelStop  uint16 = 32 // left the channel phase (into backswing)
	EvAbilityFinish       uint16 = 33 // cast completed normally (returned to ready)
	EvAbilityStopped      uint16 = 34 // cast interrupted before finish
)

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
	if len(w.runtimeAbilityDefs) != 0 || len(defs)+cap(w.runtimeAbilityDefs) > maxAbilityDefs {
		return false
	}
	w.abilityDefs = defs
	return true
}

// RegisterAbilityDef appends a deterministic runtime ability row after
// the bound data-table defs and returns its stable ability ref. Call it
// during setup or through a lockstep command path between Step calls;
// registration during a tick phase is refused so row order cannot vary
// by callback timing.
func (w *World) RegisterAbilityDef(def data.Ability) (uint16, bool) {
	if w.inStep || len(w.runtimeAbilityDefs) == cap(w.runtimeAbilityDefs) ||
		w.abilityDefCount() >= maxAbilityDefs ||
		!w.validRuntimeAbilityDef(&def) ||
		w.abilityIDExists(def.ID) {
		return 0, false
	}
	w.runtimeAbilityDefs = append(w.runtimeAbilityDefs, def)
	return uint16(w.abilityDefCount()), true
}

// SetAbility fills a slot with a bound ability ref. Tests and the
// (future, #217) data spawner use it.
func (w *World) SetAbility(id EntityID, slot int, defIndex int) bool {
	if defIndex < 0 || defIndex >= w.abilityDefCount() {
		return false
	}
	return w.SetAbilityRef(id, slot, uint16(defIndex+1))
}

// SetAbilityRef fills a slot with a concrete ability ref. Runtime
// registrations return refs directly, while static table callers may
// continue to use SetAbility's defIndex form.
func (w *World) SetAbilityRef(id EntityID, slot int, ref uint16) bool {
	ar := w.Abilities.Row(id)
	if ar == -1 || !w.Ents.Alive(id) || !validAbilitySlot(slot) || w.abilityDefByRef(ref) == nil {
		return false
	}
	w.AbilityFields.RemoveSlot(id, slot)
	w.Abilities.AbilityID[ar][slot] = ref
	w.Abilities.CastState[ar][slot] = CastReady
	w.Abilities.ReadyAt[ar][slot] = 0
	return true
}

// RemoveAbility clears one ability slot and all per-instance field
// overrides bound to that slot.
func (w *World) RemoveAbility(id EntityID, slot int) bool {
	ar := w.Abilities.Row(id)
	if ar == -1 || !w.Ents.Alive(id) || !validAbilitySlot(slot) || w.Abilities.AbilityID[ar][slot] == 0 {
		return false
	}
	w.AbilityFields.RemoveSlot(id, slot)
	w.Abilities.AbilityID[ar][slot] = 0
	w.Abilities.Level[ar][slot] = 0
	w.Abilities.ReadyAt[ar][slot] = 0
	w.Abilities.CastState[ar][slot] = CastReady
	if w.Abilities.CastSlot[ar] == int8(slot) {
		w.Abilities.CastSlot[ar] = -1
		w.Abilities.CastEnd[ar] = 0
	}
	return true
}

// SetAbilityField writes a per-instance override for an equipped
// ability slot. Unknown fields, empty slots, stale entities, and pool
// exhaustion fail closed.
func (w *World) SetAbilityField(id EntityID, slot int, field AbilityField, value fixed.F64) bool {
	ar := w.Abilities.Row(id)
	if ar == -1 || !w.Ents.Alive(id) || !validAbilitySlot(slot) || w.Abilities.AbilityID[ar][slot] == 0 || !validAbilityField(field) {
		return false
	}
	return w.AbilityFields.Set(w.Ents, id, slot, field, value)
}

// ResolveAbilityField reads the per-instance override when present,
// otherwise the immutable data.Ability default for the equipped slot.
func (w *World) ResolveAbilityField(id EntityID, slot int, field AbilityField) (fixed.F64, bool) {
	ar := w.Abilities.Row(id)
	if ar == -1 || !w.Ents.Alive(id) || !validAbilitySlot(slot) || !validAbilityField(field) {
		return 0, false
	}
	if v, ok := w.AbilityFields.Get(id, slot, field); ok {
		return v, true
	}
	ref := w.Abilities.AbilityID[ar][slot]
	def := w.abilityDefByRef(ref)
	if def == nil {
		return 0, false
	}
	return abilityDefField(def, field)
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
		// Fold any mana-regen / max-mana buff/item/upgrade mods over the base
		// per-tick rate and cap (#522). Untouched-cache identity keeps a
		// non-modded unit bit-exact.
		regen := w.BuffedManaRegen(a.Entity[ar], a.ManaRegen[ar])
		maxMana := w.BuffedMaxMana(a.Entity[ar], a.MaxMana[ar])
		if regen > 0 && a.Mana[ar] < maxMana {
			m := a.Mana[ar].Add(regen)
			if m > maxMana {
				m = maxMana
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
	manaCost, ok := w.ResolveAbilityField(id, slot, AbilityFieldManaCost)
	if !ok || manaCost < 0 {
		w.completeOrder(or, id, false)
		return
	}
	if a.Mana[ar] < manaCost {
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
	a.Mana[ar] = a.Mana[ar].Sub(manaCost)
	a.CastSlot[ar] = int8(slot)
	a.CastEnd[ar] = w.tick + uint32(def.CastPointTicks)
	w.castTransition(id, ar, slot, CastPoint)
	w.Emit(Event{Kind: EvAbilityCast, Src: id, Dst: w.Orders.Target[or], Arg: int64(ref)})
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
	def := w.abilityDefByRef(a.AbilityID[ar][slot])
	if def == nil {
		a.CastSlot[ar] = -1
		w.castTransition(id, ar, slot, CastReady)
		w.completeOrder(or, id, false)
		return
	}
	switch a.CastState[ar][slot] {
	case CastPoint:
		// EFFECT edge: composition fires, cooldown commits
		cooldown, ok := w.ResolveAbilityField(id, slot, AbilityFieldCooldown)
		if !ok || cooldown < 0 {
			a.CastSlot[ar] = -1
			w.castTransition(id, ar, slot, CastReady)
			w.completeOrder(or, id, false)
			return
		}
		effectEv := Event{Kind: EvAbilityEffect, Src: id, Dst: w.Orders.Target[or], Arg: int64(a.AbilityID[ar][slot])}
		if def.TriggerName != "" {
			// #478: behavior is a bound trigger (event = this cast). An unbound
			// name, a disabled trigger, or a false condition is a documented
			// no-op (the spell simply does nothing this cast).
			if tid, ok := w.TriggerByName(def.TriggerName); ok {
				w.FireBoundTrigger(tid, effectEv)
			}
		} else {
			w.ExecuteEffects(def.Effects, EffectCtx{
				Source: id,
				Target: w.Orders.Target[or],
				Point:  w.Orders.Point[or],
			})
		}
		w.Emit(effectEv)
		a.ReadyAt[ar][slot] = w.tick + abilityFieldTicks(cooldown)
		if def.ChannelTicks > 0 {
			a.CastEnd[ar] = w.tick + uint32(def.ChannelTicks)
			w.castTransition(id, ar, slot, CastChannel)
			w.Emit(Event{Kind: EvAbilityChannelStart, Src: id, Dst: w.Orders.Target[or], Arg: int64(a.AbilityID[ar][slot])})
			return
		}
		a.CastEnd[ar] = w.tick + uint32(def.BackswingTicks)
		w.castTransition(id, ar, slot, CastBackswing)
		if def.BackswingTicks == 0 {
			w.finishCast(ar, id, slot, or)
		}
	case CastChannel:
		w.Emit(Event{Kind: EvAbilityChannelStop, Src: id, Dst: w.Orders.Target[or], Arg: int64(a.AbilityID[ar][slot])})
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
	ref := w.Abilities.AbilityID[ar][slot]
	w.Abilities.CastSlot[ar] = -1
	w.castTransition(id, ar, slot, CastReady)
	w.Emit(Event{Kind: EvAbilityFinish, Src: id, Dst: w.Orders.Target[or], Arg: int64(ref)})
	w.completeOrder(or, id, true)
}

// cancelCast is the interrupt edge. Castpoint cancel refunds mana and
// leaves the cooldown clock untouched; channel/backswing cancels keep
// both (the EFFECT edge already committed them).
func (w *World) cancelCast(ar int32, id EntityID, slot int) {
	a := w.Abilities
	if a.CastState[ar][slot] == CastPoint {
		manaCost, ok := w.ResolveAbilityField(id, slot, AbilityFieldManaCost)
		if !ok || manaCost < 0 {
			manaCost = 0
		}
		m := a.Mana[ar].Add(manaCost)
		if maxMana := w.BuffedMaxMana(id, a.MaxMana[ar]); m > maxMana {
			m = maxMana
		}
		a.Mana[ar] = m
	}
	ref := a.AbilityID[ar][slot]
	a.CastSlot[ar] = -1
	w.castTransition(id, ar, slot, CastReady)
	// interrupt: target may be gone, so Dst = 0; the ability ref still identifies
	// which cast was stopped.
	w.Emit(Event{Kind: EvAbilityStopped, Src: id, Dst: 0, Arg: int64(ref)})
}

// castableSlot finds the slot equipped with ref. nil = not equipped.
func (w *World) castableSlot(ar int32, ref uint16) (*data.Ability, int) {
	def := w.abilityDefByRef(ref)
	if def == nil {
		return nil, -1
	}
	for s := 0; s < AbilitySlots; s++ {
		if w.Abilities.AbilityID[ar][s] == ref {
			return def, s
		}
	}
	return nil, -1
}

func (w *World) abilityDefCount() int {
	return len(w.abilityDefs) + len(w.runtimeAbilityDefs)
}

// AbilityDefCount returns the number of static plus runtime ability
// definitions currently addressable by ability refs.
func (w *World) AbilityDefCount() int {
	return w.abilityDefCount()
}

func (w *World) abilityDefByRef(ref uint16) *data.Ability {
	if ref == 0 {
		return nil
	}
	idx := int(ref) - 1
	if idx < len(w.abilityDefs) {
		return &w.abilityDefs[idx]
	}
	idx -= len(w.abilityDefs)
	if idx >= 0 && idx < len(w.runtimeAbilityDefs) {
		return &w.runtimeAbilityDefs[idx]
	}
	return nil
}

// AbilityRefByCode resolves an ability's stable string id to its ability ref
// (defIndex+1), scanning the static table defs then the runtime registrations —
// the same order refs are assigned. Returns (0,false) on an unknown id, so a
// caller can resolve a data-loaded ability without knowing its load position
// (#487). Mirrors UnitTypeID / BuffTypeID.
func (w *World) AbilityRefByCode(id string) (uint16, bool) {
	for i := range w.abilityDefs {
		if w.abilityDefs[i].ID == id {
			return uint16(i + 1), true
		}
	}
	for i := range w.runtimeAbilityDefs {
		if w.runtimeAbilityDefs[i].ID == id {
			return uint16(len(w.abilityDefs) + i + 1), true
		}
	}
	return 0, false
}

func (w *World) abilityIDExists(id string) bool {
	for i := range w.abilityDefs {
		if w.abilityDefs[i].ID == id {
			return true
		}
	}
	for i := range w.runtimeAbilityDefs {
		if w.runtimeAbilityDefs[i].ID == id {
			return true
		}
	}
	return false
}

func (w *World) validRuntimeAbilityDef(def *data.Ability) bool {
	if def == nil ||
		def.ID == "" ||
		len(def.ID) > maxRuntimeAbilityStringLen ||
		len(def.Name) > maxRuntimeAbilityStringLen ||
		def.ManaCost < 0 ||
		def.ManaCost > maxRuntimeAbilityManaCost ||
		def.CastRange < 0 {
		return false
	}
	if def.Effects.Len == 0 {
		return def.Effects.Off == 0
	}
	// Runtime effect-bearing abilities are treated as targeted until
	// the public API carries an explicit target kind; require a positive
	// range rather than silently permitting zero-range target casts.
	if def.CastRange == 0 {
		return false
	}
	end := uint32(def.Effects.Off) + uint32(def.Effects.Len)
	return end <= uint32(len(w.effects))
}

func abilityFieldTicks(v fixed.F64) uint32 {
	if v <= 0 {
		return 0
	}
	n := v.Floor()
	if n > int64(^uint32(0)) {
		return ^uint32(0)
	}
	return uint32(n)
}

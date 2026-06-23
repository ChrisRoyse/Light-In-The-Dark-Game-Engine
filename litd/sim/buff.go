package sim

// Buff state machines (#162, combat-and-orders.md §5.2): buff TYPES
// are data rows (duration, stacking rule, periodic effect list, stat
// modifiers — litd/data/buffs.go); INSTANCES live in the pooled
// BuffPool. This file owns the lifecycle:
//
//	apply (stacking rules) → periodic fires (phase 5, before the
//	damage apply pass) → expiry/dispel sweep (phase 7, one free path)
//
// Stat modifiers act through a derived-stat cache: per entity index,
// per stat, a flat Add plus a multiplicative factor, recomputed ONLY
// when the entity's buff set changes, folded in canonical
// (BuffID, pool index) order so application order can never change a
// derived value. Movement speed, armor, and attack cooldown consult
// the cache at their existing read sites; identity (+0, ×1) when the
// entity carries nothing.
//
// Clock semantics: RemainingTicks starts at DurationTicks and the
// phase-7 sweep of the application tick already decrements it — a
// D-tick buff modifies exactly ticks T..T+D-1. PeriodicClock holds
// the absolute next-fire tick, fixed at application: interval P
// applied at tick T fires at T, T+P, T+2P, … EvBuffExpired is emitted
// from phase 7, so handlers dispatch in the NEXT tick's event phase.

import (
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
)

// EvBuffExpired fires when an instance's RemainingTicks hits zero —
// natural expiry and dispel share the path. Src = the carrier, Dst =
// the buff's source, Arg = the buff-type index. Emitted in pool-index
// order within the sweep.
const EvBuffExpired uint16 = 8

// Buff-attach events (#469). Buffs emitted only EvBuffExpired; a trigger could
// not react to a buff being attached. EvBuffApplied fires when a new instance
// is created, EvBuffRefreshed when an existing instance is refreshed/restacked.
// Ids 37–38 follow the attack events (35–36). Payload: Src = applier source,
// Dst = target, Arg = packBuffArg(buffID, stacks, auraChild) — so a handler
// reads the buff type, the resulting stack count, and whether the instance is
// an aura child, all from one Arg.
const (
	EvBuffApplied   uint16 = 37
	EvBuffRefreshed uint16 = 38
)

// packBuffArg encodes a buff event's Arg: buffID in bits 0–15, the resulting
// stack count in bits 16–23, the aura-child flag in bit 24.
func packBuffArg(buffID uint16, stacks uint8, aura bool) int64 {
	v := int64(buffID) | int64(stacks)<<16
	if aura {
		v |= 1 << 24
	}
	return v
}

// BuffArgID/BuffArgStacks/BuffArgIsAura unpack a buff event's Arg (the inverse
// of packBuffArg) for handlers/tests.
func BuffArgID(arg int64) uint16    { return uint16(arg & 0xFFFF) }
func BuffArgStacks(arg int64) uint8 { return uint8((arg >> 16) & 0xFF) }
func BuffArgIsAura(arg int64) bool  { return arg&(1<<24) != 0 }

// BindBuffTypes installs the loaded buff-type rows. Refs in
// BuffInstance.BuffID index this slice directly. Fail-closed on a set
// too large for the uint16 ref space.
func (w *World) BindBuffTypes(types []data.BuffType) bool {
	if len(types) > 1<<16 {
		return false
	}
	w.buffTypes = types
	idx := make(map[string]uint16, len(types))
	for i := range types {
		if types[i].ID != "" {
			idx[types[i].ID] = uint16(i)
		}
	}
	w.buffTypeByCode = idx
	return true
}

// BuffTypeID resolves a buff code (data.BuffType.ID) to its bound type
// index. ok=false for an unknown code or before BindBuffTypes.
func (w *World) BuffTypeID(code string) (uint16, bool) {
	id, ok := w.buffTypeByCode[code]
	return id, ok
}

// UnitHasBuff reports whether the unit carries at least one live instance
// of the given buff type.
func (w *World) UnitHasBuff(id EntityID, buffID uint16) bool {
	p := w.Buffs
	for i := int32(0); int(i) < p.Cap(); i++ {
		if p.live[i] && p.rows[i].Target == id && p.rows[i].BuffID == buffID {
			return true
		}
	}
	return false
}

// UnitBuffCount returns the number of live buff instances on the unit
// (each instance counts once, regardless of stacks).
func (w *World) UnitBuffCount(id EntityID) int {
	n := 0
	p := w.Buffs
	for i := int32(0); int(i) < p.Cap(); i++ {
		if p.live[i] && p.rows[i].Target == id {
			n++
		}
	}
	return n
}

// BuffStacks returns the stack count of the unit's instance of buffID, or
// 0 if absent.
func (w *World) BuffStacks(id EntityID, buffID uint16) uint8 {
	p := w.Buffs
	for i := int32(0); int(i) < p.Cap(); i++ {
		if p.live[i] && p.rows[i].Target == id && p.rows[i].BuffID == buffID {
			return p.rows[i].Stacks
		}
	}
	return 0
}

// BuffRemainingTicks returns the remaining duration (ticks) of the unit's
// instance of buffID, or 0 if absent.
func (w *World) BuffRemainingTicks(id EntityID, buffID uint16) uint32 {
	p := w.Buffs
	for i := int32(0); int(i) < p.Cap(); i++ {
		if p.live[i] && p.rows[i].Target == id && p.rows[i].BuffID == buffID {
			return p.rows[i].RemainingTicks
		}
	}
	return 0
}

// RemoveBuff frees every live instance of buffID on the unit, returning
// the number removed; the derived-stat cache is recomputed if any went.
func (w *World) RemoveBuff(id EntityID, buffID uint16) int {
	n := 0
	p := w.Buffs
	for i := int32(0); int(i) < p.Cap(); i++ {
		if p.live[i] && p.rows[i].Target == id && p.rows[i].BuffID == buffID {
			p.Free(i)
			n++
		}
	}
	if n > 0 && w.Ents.Alive(id) {
		w.recomputeBuffStats(id)
	}
	return n
}

// RemoveAllBuffs frees every live buff instance on the unit, returning the
// count removed.
func (w *World) RemoveAllBuffs(id EntityID) int {
	n := 0
	p := w.Buffs
	for i := int32(0); int(i) < p.Cap(); i++ {
		if p.live[i] && p.rows[i].Target == id {
			p.Free(i)
			n++
		}
	}
	if n > 0 && w.Ents.Alive(id) {
		w.recomputeBuffStats(id)
	}
	return n
}

// ApplyBuff applies one buff type to target, resolving the type's
// stacking rule against the target's live instances. Returns false
// when the type index is out of range, the target is not alive, or a
// needed pool slot is exhausted (deterministic refusal — never a
// partial apply).
func (w *World) ApplyBuff(target, source EntityID, typeIdx int, stacks uint8) bool {
	if typeIdx < 0 || typeIdx >= len(w.buffTypes) || !w.Ents.Alive(target) {
		return false
	}
	bt := &w.buffTypes[typeIdx]
	if stacks == 0 {
		stacks = 1
	}
	// MaxStacks caps the stack-count accumulator only; strongest-wins
	// compares raw stack counts and the other rules ignore stacks
	if bt.Stacking == data.StackCount && stacks > bt.MaxStacks {
		stacks = bt.MaxStacks
	}

	// existing instance of the same (type, target): lowest pool index
	existing := int32(-1)
	if bt.Stacking != data.StackIndependent {
		p := w.Buffs
		for i := int32(0); int(i) < p.Cap(); i++ {
			if p.live[i] && p.rows[i].BuffID == uint16(typeIdx) && p.rows[i].Target == target {
				existing = i
				break
			}
		}
	}

	if existing != -1 {
		row := w.Buffs.Row(existing)
		switch bt.Stacking {
		case data.StackRefresh:
			row.Source = source
			row.RemainingTicks = uint32(bt.DurationTicks)
			row.PeriodicClock = w.tick // re-fixed phase: fires this tick again
		case data.StackCount:
			s := int(row.Stacks) + int(stacks)
			if s > int(bt.MaxStacks) {
				s = int(bt.MaxStacks)
			}
			row.Stacks = uint8(s)
			row.Source = source
			row.RemainingTicks = uint32(bt.DurationTicks)
		case data.StackStrongestWins:
			if stacks <= row.Stacks {
				return true // weaker or equal application is discarded whole
			}
			row.Stacks = stacks
			row.Source = source
			row.RemainingTicks = uint32(bt.DurationTicks)
			row.PeriodicClock = w.tick
		}
		w.recomputeBuffStats(target)
		w.Emit(Event{Kind: EvBuffRefreshed, Src: source, Dst: target, Arg: packBuffArg(uint16(typeIdx), row.Stacks, false)})
		return true
	}

	i, ok := w.Buffs.Alloc()
	if !ok {
		return false
	}
	*w.Buffs.Row(i) = BuffInstance{
		BuffID:         uint16(typeIdx),
		Stacks:         stacks,
		Target:         target,
		Source:         source,
		RemainingTicks: uint32(bt.DurationTicks),
		PeriodicClock:  w.tick,
	}
	w.recomputeBuffStats(target)
	w.Emit(Event{Kind: EvBuffApplied, Src: source, Dst: target, Arg: packBuffArg(uint16(typeIdx), stacks, false)})
	return true
}

// Dispel zeroes the remaining duration of the target's dispellable
// buffs; the phase-7 sweep frees them through the single expiry path
// (one EvBuffExpired each, no second free site). Returns the number
// of instances marked.
func (w *World) Dispel(target EntityID) int {
	p := w.Buffs
	n := 0
	for i := int32(0); int(i) < p.Cap(); i++ {
		if !p.live[i] || p.rows[i].Target != target {
			continue
		}
		if w.buffTypes[p.rows[i].BuffID].Flags&data.BuffDispellable == 0 {
			continue
		}
		p.rows[i].RemainingTicks = 0
		n++
	}
	return n
}

// buffPeriodicSystem fires due periodic compositions, phase 5 after
// the attack system and before the damage apply pass — periodic
// damage queued this tick lands this tick. Pool-index order. The
// carrier is both source-context target and point anchor; the buff's
// APPLIER stays the effect source so kill credit and team filters
// resolve against the caster.
func (w *World) buffPeriodicSystem() {
	p := w.Buffs
	for i := int32(0); int(i) < p.Cap(); i++ {
		if !p.live[i] {
			continue
		}
		row := &p.rows[i]
		bt := &w.buffTypes[row.BuffID]
		if bt.PeriodTicks == 0 || bt.Periodic.Len == 0 {
			continue
		}
		if !CooldownReady(w.tick, row.PeriodicClock) || !w.Ents.Alive(row.Target) {
			continue
		}
		row.PeriodicClock += uint32(bt.PeriodTicks)
		ctx := EffectCtx{Source: row.Source, Target: row.Target}
		if tr := w.Transforms.Row(row.Target); tr != -1 {
			ctx.Point = w.Transforms.Pos[tr]
		}
		w.ExecuteEffects(bt.Periodic, ctx)
	}
}

// buffExpirySystem is the ONE free path, phase 7 before the deferred
// entity removals. Per live instance, pool-index order: a dead or
// destroyed carrier frees silently (death cleanup, not expiry);
// otherwise RemainingTicks decrements and zero frees with
// EvBuffExpired. Dispel pre-zeroed durations land here too.
func (w *World) buffExpirySystem() {
	p := w.Buffs
	for i := int32(0); int(i) < p.Cap(); i++ {
		if !p.live[i] {
			continue
		}
		row := &p.rows[i]
		if !w.Ents.Alive(row.Target) {
			p.Free(i)
			continue
		}
		if row.RemainingTicks > 0 {
			row.RemainingTicks--
		}
		if row.RemainingTicks == 0 {
			target := row.Target
			// Pack the full buff arg (id + stacks + aura-child flag) like apply/
			// refresh (#488), so an OnBuffExpired handler can read Event.BuffStacks
			// / Event.FromAura on the expiring instance — not just its type id.
			isAura := row.Flags&BuffInstAuraChild != 0
			w.Emit(Event{Kind: EvBuffExpired, Src: target, Dst: row.Source, Arg: packBuffArg(row.BuffID, row.Stacks, isAura)})
			p.Free(i)
			w.recomputeBuffStats(target)
		}
	}
}

// recomputeBuffStats rebuilds one entity's derived-stat cache from
// its live instances, folded in canonical (BuffID, pool index) order:
// per instance, per mod row, per stack — Add sums, Permille
// multiplies. Runs only on buff-set change, never per tick.
func (w *World) recomputeBuffStats(target EntityID) {
	idx := target.Index()
	for s := 0; s < int(data.BuffStatCount); s++ {
		w.buffAdd[s][idx] = 0
		w.buffMult[s][idx] = fixed.One
	}
	w.foldUpgradeStats(target)  // upgrades first (#303)
	w.foldHeroAttrStats(target) // then hero attributes (#304)
	w.foldItemStats(target)     // then carried items (#305); buffs stack on top
	// gather the target's instances; pool-index ascending, then a
	// stable insertion sort by BuffID keeps (BuffID, pool index) order
	sel := w.buffScratch[:0]
	p := w.Buffs
	for i := int32(0); int(i) < p.Cap(); i++ {
		if !p.live[i] || p.rows[i].Target != target {
			continue
		}
		pos := len(sel)
		for pos > 0 && p.rows[sel[pos-1]].BuffID > p.rows[i].BuffID {
			pos--
		}
		sel = append(sel, 0)
		copy(sel[pos+1:], sel[pos:])
		sel[pos] = i
	}
	for _, i := range sel {
		row := &p.rows[i]
		bt := &w.buffTypes[row.BuffID]
		for mi := range bt.Mods {
			m := &bt.Mods[mi]
			mult := fixed.FromInt(m.Permille).Div(fixed.FromInt(1000))
			for s := uint8(0); s < row.Stacks; s++ {
				w.buffAdd[m.Stat][idx] += m.Add
				w.buffMult[m.Stat][idx] = w.buffMult[m.Stat][idx].Mul(mult)
			}
		}
	}
	w.buffScratch = sel[:0]

	// A dropped +max-mana modifier can leave current mana above the new
	// (buffed) cap — clamp it down so the pool never exceeds its maximum
	// (#522). A raise needs no action: regen fills toward the higher cap.
	if ar := w.Abilities.Row(target); ar >= 0 {
		if cap := w.BuffedMaxMana(target, w.Abilities.MaxMana[ar]); w.Abilities.Mana[ar] > cap {
			w.Abilities.Mana[ar] = cap
		}
	}
}

// buffedStat folds one entity's cache into a base value:
// (base + Add) × Mult, floored at zero. The untouched-cache identity
// returns base bit-exactly.
func (w *World) buffedStat(stat uint8, id EntityID, base int64) int64 {
	idx := id.Index()
	add, mult := w.buffAdd[stat][idx], w.buffMult[stat][idx]
	if add == 0 && mult == fixed.One {
		return base
	}
	v := int64(fixed.F64(base + add).Mul(fixed.F64(mult)))
	if v < 0 {
		v = 0
	}
	return v
}

// BuffedMoveSpeed is the movement system's read: base per-tick speed
// through the move-speed cache (Add already in per-tick fixed bits).
func (w *World) BuffedMoveSpeed(id EntityID, base fixed.F64) fixed.F64 {
	return fixed.F64(w.buffedStat(data.StatMoveSpeed, id, int64(base)))
}

// BuffedArmor is the damage pipeline's read: integer armor value
// through the armor cache. Add is integer points; the multiply runs
// in fixed point and floors back to integer armor.
func (w *World) BuffedArmor(id EntityID, base int) int {
	idx := id.Index()
	add, mult := w.buffAdd[data.StatArmor][idx], w.buffMult[data.StatArmor][idx]
	if add == 0 && mult == fixed.One {
		return base
	}
	return int(fixed.FromInt(int32(int64(base) + add)).Mul(mult).Floor())
}

// BuffedRegen is the regen system's read: a unit's base life-per-tick
// regeneration through the life-regen cache (Add already in per-tick fixed
// bits, same units as the unit `regen` field). The untouched-cache identity
// returns base bit-exactly, so a unit with no life-regen mod folds to its base
// regen unchanged — the property that keeps every regen-less determinism golden
// bit-identical.
func (w *World) BuffedRegen(id EntityID, base fixed.F64) fixed.F64 {
	return fixed.F64(w.buffedStat(data.StatLifeRegen, id, int64(base)))
}

// BuffedMaxMana is the mana cap read: a unit's maximum mana through the
// max-mana cache (Add in fixed bits / integer points). Untouched-cache identity
// returns base bit-exactly. Like BuffedArmor, the BASE is what the store
// persists and hashes; only reads fold the modifier.
func (w *World) BuffedMaxMana(id EntityID, base fixed.F64) fixed.F64 {
	return fixed.F64(w.buffedStat(data.StatMaxMana, id, int64(base)))
}

// BuffedManaRegen is the ability system's read: a unit's mana-per-tick
// regeneration through the mana-regen cache (Add in per-tick fixed bits, same
// units as Abilities.ManaRegen). Untouched-cache identity returns base
// bit-exactly, so a unit with no mana-regen mod folds to its base unchanged.
func (w *World) BuffedManaRegen(id EntityID, base fixed.F64) fixed.F64 {
	return fixed.F64(w.buffedStat(data.StatManaRegen, id, int64(base)))
}

// BuffedCooldown is the attack system's read: weapon cooldown ticks
// through the attack-cooldown cache (Add in signed integer ticks; the
// multiply runs in fixed point and floors back), floored at one tick
// — a weapon can never have a zero period.
func (w *World) BuffedCooldown(id EntityID, base uint16) uint32 {
	idx := id.Index()
	add, mult := w.buffAdd[data.StatAttackCooldown][idx], w.buffMult[data.StatAttackCooldown][idx]
	if add == 0 && mult == fixed.One {
		return uint32(base)
	}
	t := fixed.FromInt(int32(int64(base) + add)).Mul(mult).Floor()
	if t < 1 {
		t = 1
	}
	return uint32(t)
}

// execApplyBuff is the `apply-buff` primitive backend. Params in
// schema order: buff (type index), stacks.
func execApplyBuff(w *World, ctx EffectCtx, e *data.CompiledEffect) {
	w.ApplyBuff(ctx.Target, ctx.Source, int(e.Params[0]), uint8(e.Params[1]))
}

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

// BindBuffTypes installs the loaded buff-type rows. Refs in
// BuffInstance.BuffID index this slice directly. Fail-closed on a set
// too large for the uint16 ref space.
func (w *World) BindBuffTypes(types []data.BuffType) bool {
	if len(types) > 1<<16 {
		return false
	}
	w.buffTypes = types
	return true
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
			w.Emit(Event{Kind: EvBuffExpired, Src: target, Dst: row.Source, Arg: int64(row.BuffID)})
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
	w.foldUpgradeStats(target) // upgrades first (#303); buffs stack on top
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

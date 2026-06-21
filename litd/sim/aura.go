package sim

// Aura system (#164, combat-and-orders.md §5.2 auras bullet): an aura
// is a live buff instance whose TYPE carries an aura block (radius,
// child buff, linger). While the instance lives, allies of the
// carrier's team inside the radius — the carrier included — hold a
// child instance flagged BuffInstAuraChild.
//
// One mechanism covers every lifecycle edge: each in-range evaluation
// sets the child's RemainingTicks to the linger value, and nothing
// else ever refreshes it. Walk out of radius, die as the source, or
// dispel the aura — the refresh stops and the ordinary phase-7 sweep
// expires the child exactly linger ticks after its last in-range
// evaluation, EvBuffExpired included. Re-entering during the linger
// refreshes the same instance: no duplicate, no flicker (the data
// loader floors linger above the evaluation cadence).
//
// Evaluation is throttled on the acquisition cadence (§3.1, default
// 5 ticks) with a deterministic per-source phase offset, runs in
// pool-index order, and total-orders candidates by entity index
// before touching the pool — child slot assignment is replay-stable.

import (
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
)

// auraSystem runs phase 5 after the attack system and before the
// periodic pass — children created this tick fire their periodic
// composition this tick (the #162 fires-on-application rule).
func (w *World) auraSystem() {
	p := w.Buffs
	for i := int32(0); int(i) < p.Cap(); i++ {
		if !p.live[i] {
			continue
		}
		row := &p.rows[i]
		if row.Flags&BuffInstAuraChild != 0 {
			continue // children never radiate: no aura chains
		}
		bt := &w.buffTypes[row.BuffID]
		if bt.AuraRadius == 0 {
			continue
		}
		// throttle with per-source phase: sources evaluate on different
		// ticks, same spread as the acquisition scan
		if (w.tick+row.Target.Index())%uint32(w.acquireEvery) != 0 {
			continue
		}
		w.evaluateAura(row, bt.AuraRadius, int(bt.AuraChild), bt.AuraLingerTicks)
	}
}

// evaluateAura refreshes/creates child instances on every ally inside
// the radius of the aura carrier. Candidates are gathered from the
// bucket grid and insertion-sorted by entity index — the total order
// that makes pool slot assignment deterministic.
func (w *World) evaluateAura(aura *BuffInstance, radius fixed.F64, childIdx int, linger uint16) {
	carrier := aura.Target
	tr := w.Transforms.Row(carrier)
	or := w.Owners.Row(carrier)
	if tr == -1 || or == -1 {
		return // unplaced/unowned carrier radiates nothing
	}
	center := w.Transforms.Pos[tr]
	team := w.Owners.Team[or]
	rHi, rLo := fixed.RadiusSq(radius)

	sel := w.auraScratch[:0]
	x0, x1 := bucketCoord(center.X.Sub(radius)), bucketCoord(center.X.Add(radius))
	y0, y1 := bucketCoord(center.Y.Sub(radius)), bucketCoord(center.Y.Add(radius))
	for by := y0; by <= y1; by++ {
		for bx := x0; bx <= x1; bx++ {
			for be := w.bucketHead[by*BucketGridSize+bx]; be != -1; be = w.bucketNext[be] {
				cid := w.bucketID[be]
				if !w.Ents.Alive(cid) {
					continue
				}
				cor := w.Owners.Row(cid)
				if cor == -1 || w.Owners.Team[cor] != team {
					continue // ally-only v1; hostile auras are a data flag away
				}
				ctr := w.Transforms.Row(cid)
				if ctr == -1 {
					continue
				}
				dHi, dLo := fixed.DistSq(center, w.Transforms.Pos[ctr])
				if dHi > rHi || (dHi == rHi && dLo > rLo) {
					continue
				}
				pos := len(sel)
				for pos > 0 && sel[pos-1].Index() > cid.Index() {
					pos--
				}
				sel = append(sel, 0)
				copy(sel[pos+1:], sel[pos:])
				sel[pos] = cid
			}
		}
	}
	for _, cid := range sel {
		w.applyAuraChild(cid, aura.Source, childIdx, linger)
	}
	w.auraScratch = sel[:0]
}

// applyAuraChild refreshes or creates one flagged child. The CHILD
// type's stacking rule decides how overlapping identical auras
// coexist: independent keys instances per (type, target, SOURCE) so
// each aura source maintains its own child; every other rule keys
// (type, target) — one shared instance, refreshed by whichever source
// evaluates, with strongest-wins comparing stack counts. Pool
// exhaustion is the usual deterministic refusal.
func (w *World) applyAuraChild(target, source EntityID, childIdx int, linger uint16) {
	bt := &w.buffTypes[childIdx]
	p := w.Buffs
	perSource := bt.Stacking == data.StackIndependent
	for i := int32(0); int(i) < p.Cap(); i++ {
		if !p.live[i] {
			continue
		}
		row := &p.rows[i]
		if row.BuffID != uint16(childIdx) || row.Target != target || row.Flags&BuffInstAuraChild == 0 {
			continue
		}
		if perSource && row.Source != source {
			continue
		}
		row.RemainingTicks = uint32(linger) // the in-range refresh
		if !perSource {
			row.Source = source
		}
		w.Emit(Event{Kind: EvBuffRefreshed, Src: source, Dst: target, Arg: packBuffArg(uint16(childIdx), row.Stacks, true)})
		return
	}
	i, ok := p.Alloc()
	if !ok {
		return
	}
	*p.Row(i) = BuffInstance{
		BuffID:         uint16(childIdx),
		Stacks:         1,
		Flags:          BuffInstAuraChild,
		Target:         target,
		Source:         source,
		RemainingTicks: uint32(linger),
		PeriodicClock:  w.tick,
	}
	w.recomputeBuffStats(target)
	w.Emit(Event{Kind: EvBuffApplied, Src: source, Dst: target, Arg: packBuffArg(uint16(childIdx), 1, true)})
}

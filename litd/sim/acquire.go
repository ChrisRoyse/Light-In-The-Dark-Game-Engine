package sim

// Deterministic target acquisition (combat-and-orders.md §3.1–§3.2).
// Combat-phase scans for units in an acquiring stance, throttled to
// every AcquireInterval ticks with a per-unit phase offset derived
// from the entity index — the cost spreads across ticks as a function
// of state, never of load.
//
// Selection is a running best under the lexicographic tuple
//
//	(threatClass, distanceSq, entityIndex)
//
// distanceSq is the exact 128-bit fixed.DistSq pair (no square roots,
// no truncation), entityIndex the final total-order tie-break: the
// outcome is independent of bucket scan order, and two byte-identical
// sims always pick the same target.
//
// v1 threat tiers (the table grows with unit-class data, #150+):
// my-recent-attacker (damage memory: LastAttacker within the decay
// window) > armed > unarmed. The weapon targets-allowed mask joins
// the validity filter when #150 wires weapon table columns into the
// Combat store.

import "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"

const (
	// DefaultAcquireInterval is the §3.1 default scan throttle.
	DefaultAcquireInterval = 5
	// DamageMemoryTicks is the decay window of the last-attacker
	// memory: an attacker keeps top threat priority this many ticks
	// after its last hit (~5 s at 20 t/s).
	DamageMemoryTicks = 100
)

// Threat tiers — lower wins (§3.2 tuple position 1).
const (
	threatAttacker uint8 = iota // damaged me within the memory window
	threatArmed                 // carries a usable weapon
	threatPassive               // everything else (workers/structures split later)
)

// SetAcquireInterval overrides the scan throttle (data-tunable per
// §3.1; sim semantics, so a replay-contract version input). Values
// < 1 are rejected — fail closed on a nonsense interval.
func (w *World) SetAcquireInterval(n uint16) bool {
	if n < 1 {
		return false
	}
	w.acquireEvery = n
	return true
}

// acquisitionSystem runs in the combat phase: per acquiring unit due
// this tick, validate/clear the current target and scan for a new one.
func (w *World) acquisitionSystem() {
	interval := uint32(w.acquireEvery)
	c := w.Combats
	for cr := int32(0); cr < c.count; cr++ {
		id := c.Entity[cr]
		idx := id.Index()
		if (w.tick+idx)%interval != 0 {
			continue // not this unit's scan tick (phase offset = f(index))
		}
		if c.AcquisitionRange[cr] <= 0 {
			continue
		}
		// stale-target hygiene: dead or no-longer-hostile targets
		// clear deterministically before any rescan decision
		if t := c.Target[cr]; t != 0 {
			if w.validAcquireTarget(id, t) {
				continue // engaged; chase/drop logic is the attack cycle's (#150)
			}
			c.Target[cr] = 0
		}
		// acquiring stances: default order (Stop), Hold, or Patrol
		// (attack-move-like). A patroller leashing back to its segment
		// suppresses acquisition until it is home (#306). Move/follow/
		// cast and the rest do not auto-acquire.
		if or := w.Orders.Row(id); or != -1 {
			k := w.Orders.Kind[or]
			if k != OrderStop && k != OrderHold && k != OrderPatrol {
				continue
			}
			if k == OrderPatrol && w.patrolReturningRow(id) {
				continue
			}
		}
		c.Target[cr] = w.acquireScan(cr, id)
	}
}

// validAcquireTarget: alive, owned by a hostile team, damageable.
func (w *World) validAcquireTarget(self, target EntityID) bool {
	if !w.Ents.Alive(target) {
		return false
	}
	sr, tr := w.Owners.Row(self), w.Owners.Row(target)
	if sr == -1 || tr == -1 || w.Owners.Team[sr] == w.Owners.Team[tr] {
		return false
	}
	if w.Healths.Row(target) == -1 {
		return false
	}
	if w.visibilityGatesGameplay() && !w.CanSeeEntity(w.Owners.Player[sr], target) {
		return false
	}
	return true
}

// threatClassOf ranks candidate cid for the scanner's combat row.
func (w *World) threatClassOf(cr int32, cid EntityID) uint8 {
	c := w.Combats
	if c.LastAttacker[cr] == cid && w.tick-c.LastDamagedTick[cr] <= DamageMemoryTicks {
		return threatAttacker
	}
	if ccr := c.Row(cid); ccr != -1 && c.Cooldown[ccr][0] > 0 {
		return threatArmed
	}
	return threatPassive
}

// acquireScan walks the buckets of the acquisition-range bounding
// square keeping a running best under (threatClass, distSq128,
// entityIndex). Returns 0 when nothing valid is in range. Zero alloc.
func (w *World) acquireScan(cr int32, id EntityID) EntityID {
	tr := w.Transforms.Row(id)
	sor := w.Owners.Row(id)
	if tr == -1 || sor == -1 {
		return 0 // unplaced or unowned scanners cannot classify enemies
	}
	pos := w.Transforms.Pos[tr]
	team := w.Owners.Team[sor]
	acq := w.Combats.AcquisitionRange[cr]
	rHi, rLo := fixed.RadiusSq(acq)

	x0, x1 := bucketCoord(pos.X.Sub(acq)), bucketCoord(pos.X.Add(acq))
	y0, y1 := bucketCoord(pos.Y.Sub(acq)), bucketCoord(pos.Y.Add(acq))

	var (
		best        EntityID
		bestTier    uint8
		bestHi, bLo uint64
		bestIdx     uint32
	)
	for by := y0; by <= y1; by++ {
		for bx := x0; bx <= x1; bx++ {
			for e := w.bucketHead[by*BucketGridSize+bx]; e != -1; e = w.bucketNext[e] {
				cid := w.bucketID[e]
				if cid == id || !w.Ents.Alive(cid) {
					continue
				}
				cor := w.Owners.Row(cid)
				if cor == -1 || w.Owners.Team[cor] == team {
					continue
				}
				if w.Healths.Row(cid) == -1 {
					continue // not damageable, not acquirable
				}
				if w.visibilityGatesGameplay() && !w.CanSeeEntity(w.Owners.Player[sor], cid) {
					continue
				}
				ctr := w.Transforms.Row(cid)
				if ctr == -1 {
					continue
				}
				dHi, dLo := fixed.DistSq(pos, w.Transforms.Pos[ctr])
				if dHi > rHi || (dHi == rHi && dLo > rLo) {
					continue // outside acquisition range (inclusive boundary)
				}
				tier := w.threatClassOf(cr, cid)
				cIdx := cid.Index()
				if best != 0 {
					// keep current best unless the candidate is
					// strictly smaller under the tuple order
					if tier != bestTier {
						if tier > bestTier {
							continue
						}
					} else if dHi != bestHi {
						if dHi > bestHi {
							continue
						}
					} else if dLo != bLo {
						if dLo > bLo {
							continue
						}
					} else if cIdx >= bestIdx {
						continue
					}
				}
				best, bestTier, bestHi, bLo, bestIdx = cid, tier, dHi, dLo, cIdx
			}
		}
	}
	return best
}

package sim

// Missile flight and impact (#158, ADR #295). Missiles fly at the
// movement-phase tail, BEFORE the bucket reconcile — combat (phase 5)
// sees same-tick post-flight positions for units and missiles alike,
// and no new top-level phase exists.
//
// Per tick per missile: refresh the goal (homing missiles track
// GuideEnt and update GuidePt to its live position — the last known
// position when it dies), then either arrive (snap, execute payload,
// deferred kill) or advance one speed-step. A dead guide target
// resolves per the MissileAoE flag: continue-and-detonate at the last
// known point, or expire payload-less. The launcher dying changes
// nothing — homing delivery does not need a living source.

import (
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
)

// MissileSpec parameterizes SpawnMissile. Target != 0 spawns a homing
// missile; Target == 0 flies to Point. Payload.Len > 0 selects the
// compiled effect list, else Packet delivers (rolled at launch).
//
// A linear skillshot sets Flags|MissileLinear and fills Dir / Range /
// Pierce / Decay instead of Target/Point: it flies along Dir for Range
// world units, colliding with foes of Source's team, delivering the
// payload per hit (per-mille Decay between hits) until Pierce hits or
// range is spent.
type MissileSpec struct {
	Pos     fixed.Vec2
	Source  EntityID
	Target  EntityID
	Point   fixed.Vec2
	Speed   fixed.F64 // world units per tick, must be positive
	Arc     fixed.F64
	Flags   uint8
	Payload data.EffectList
	Packet  DamagePacket

	// Linear skillshot fields (Flags&MissileLinear).
	Dir    fixed.Vec2 // flight direction (need not be normalized; SpawnMissile normalizes)
	Range  fixed.F64  // max travel before expiry, must be positive
	Pierce int32      // max hits (≥1); 1 = single-target skillshot
	Decay  uint16     // per-mille payload decay between pierce hits (0 → 1000 = none)
}

// missileHitRadius is the lateral collision half-width of a linear
// skillshot against a unit centre (world units). v1 constant until a
// per-unit collision size joins the hit test (#334 seam).
const missileHitRadius = 40

// dirIntScale: linear collision projects in integer world units; the
// unit direction is scaled to this integer length so the projection
// keeps sub-unit resolution without F64 overflow (det §2.1).
const dirIntScale = 1024

// SpawnMissile creates a missile entity. Fails deterministically
// (0, false) on pool exhaustion or a bad spec — NEVER a silent
// fire-as-instant fallback (ecs §2).
func (w *World) SpawnMissile(m MissileSpec) (EntityID, bool) {
	if m.Speed <= 0 {
		return 0, false
	}
	var dir fixed.Vec2
	if m.Flags&MissileLinear != 0 {
		// skillshot: validated independently of a guide target
		dir = unitStep(m.Dir, fixed.FromInt(1)) // normalize to unit length
		if dir == (fixed.Vec2{}) || m.Range <= 0 || m.Pierce < 1 {
			return 0, false // degenerate skillshot: deterministic failure
		}
	} else if m.Target != 0 && !w.Ents.Alive(m.Target) {
		return 0, false // launch at a corpse resolves to nothing
	}
	if int(w.Missiles.Count()) >= w.caps.Projectiles {
		return 0, false // pool exhausted: creation fails, like WC3 handle limits
	}
	id, ok := w.Ents.Create()
	if !ok {
		return 0, false
	}
	if !w.Transforms.Add(w.Ents, id, m.Pos, 0) || !w.Missiles.Add(w.Ents, id) {
		if w.Transforms.Row(id) != -1 {
			w.Transforms.Remove(id)
		}
		w.Ents.Destroy(id)
		return 0, false
	}
	w.bucketInsert(id, m.Pos)
	w.MarkSnap(id)
	s := w.Missiles
	r := s.Row(id)
	s.Speed[r] = m.Speed
	s.Arc[r] = m.Arc
	s.Flags[r] = m.Flags
	s.GuideEnt[r] = m.Target
	s.GuidePt[r] = m.Point
	if m.Target != 0 {
		if tr := w.Transforms.Row(m.Target); tr != -1 {
			s.GuidePt[r] = w.Transforms.Pos[tr]
		}
	}
	s.Payload[r] = m.Payload
	s.Packet[r] = m.Packet
	s.Source[r] = m.Source
	s.BirthTick[r] = w.tick
	if m.Flags&MissileLinear != 0 {
		s.Dir[r] = dir
		s.RangeLeft[r] = m.Range
		s.PierceLeft[r] = m.Pierce
		if m.Decay == 0 {
			s.Decay[r] = 1000
		} else {
			s.Decay[r] = m.Decay
		}
		s.GuideEnt[r] = 0
	}
	return id, true
}

// missileSystem advances every missile one tick (movement-phase
// tail). Dense-row iteration; rows killed here stay until phase 7,
// so the walk never invalidates itself mid-pass.
func (w *World) missileSystem() {
	s := w.Missiles
	for r := int32(0); r < s.count; r++ {
		id := s.Entity[r]
		if !w.Ents.Alive(id) {
			continue
		}
		if s.Flags[r]&MissileLinear != 0 {
			w.advanceLinear(id, r) // #331 skillshot
			continue
		}
		// goal refresh
		if tgt := s.GuideEnt[r]; tgt != 0 {
			if w.Ents.Alive(tgt) {
				if tr := w.Transforms.Row(tgt); tr != -1 {
					s.GuidePt[r] = w.Transforms.Pos[tr]
				}
			} else if s.Flags[r]&MissileAoE != 0 {
				s.GuideEnt[r] = 0 // continue to last known position
			} else {
				if w.OnMissileExpire != nil {
					w.OnMissileExpire(w.tick, id, s.GuidePt[r])
				}
				w.KillUnit(id) // expire: payload-less
				continue
			}
		}
		tr := w.Transforms.Row(id)
		if tr == -1 {
			continue
		}
		pos := w.Transforms.Pos[tr]
		goal := s.GuidePt[r]
		sHi, sLo := fixed.RadiusSq(s.Speed[r])
		dHi, dLo := fixed.DistSq(pos, goal)
		if dHi < sHi || (dHi == sHi && dLo <= sLo) {
			w.Transforms.Pos[tr] = goal // snap-arrive
			w.impactMissile(id, r, goal)
			continue
		}
		step := unitStep(goal.Sub(pos), s.Speed[r])
		if step == (fixed.Vec2{}) {
			w.Transforms.Pos[tr] = goal // direction underflow: arrive, never hover
			w.impactMissile(id, r, goal)
			continue
		}
		w.Transforms.Pos[tr] = pos.Add(step)
	}
}

// impactMissile executes the payload at the impact point and marks
// the missile for standard deferred removal.
func (w *World) impactMissile(id EntityID, r int32, at fixed.Vec2) {
	s := w.Missiles
	tgt := s.GuideEnt[r]
	if tgt != 0 && !w.Ents.Alive(tgt) {
		tgt = 0
	}
	if lst := s.Payload[r]; lst.Len > 0 {
		w.ExecuteEffects(lst, EffectCtx{Source: s.Source[r], Target: tgt, Point: at})
	} else {
		p := s.Packet[r]
		if tgt != 0 {
			p.Target = tgt // homing delivery follows the live guide
		}
		if p.Target != 0 {
			w.QueueDamage(p)
		}
	}
	if w.OnMissileImpact != nil {
		w.OnMissileImpact(w.tick, id, at, tgt)
	}
	w.KillUnit(id)
}

// advanceLinear steps a skillshot one tick (#331). It sweeps the
// segment [pos, pos+Dir·step] and delivers the payload to every foe the
// segment crosses THIS advance, front-to-back, up to the remaining
// pierce budget — applying per-mille decay between hits. The collision
// window is referenced from the CURRENT position (along ∈ [0, step)),
// so a foe is crossed in exactly one advance over the whole flight: no
// already-hit set is needed. All projection math is integer world
// units in a direction-scaled space (det §2.1), overflow-safe.
func (w *World) advanceLinear(id EntityID, r int32) {
	s := w.Missiles
	tr := w.Transforms.Row(id)
	if tr == -1 {
		w.KillUnit(id)
		return
	}
	pos := w.Transforms.Pos[tr]
	step := s.Speed[r]
	if s.RangeLeft[r] < step {
		step = s.RangeLeft[r] // final advance: travel only the remainder
	}
	dir := s.Dir[r]

	// integer, scaled direction → overflow-safe projection space.
	dx := dir.X.Mul(fixed.FromInt(dirIntScale)).Floor()
	dy := dir.Y.Mul(fixed.FromInt(dirIntScale)).Floor()
	l2 := dx*dx + dy*dy
	if l2 > 0 && step > 0 {
		l := int64(fixed.SqrtU64(uint64(l2)))
		stepL := (int64(step) * l) >> 32 // window upper bound in along-space
		radiusSqL2 := int64(missileHitRadius) * int64(missileHitRadius) * l2
		w.linearHits(id, r, pos, dx, dy, stepL, radiusSqL2)
	}

	// advance the body and spend range; die when pierce or range is out.
	w.Transforms.Pos[tr] = pos.Add(dir.Scale(step))
	s.RangeLeft[r] = s.RangeLeft[r].Sub(step)
	if s.PierceLeft[r] <= 0 || s.RangeLeft[r] <= 0 {
		if w.OnMissileExpire != nil {
			w.OnMissileExpire(w.tick, id, w.Transforms.Pos[tr])
		}
		w.KillUnit(id)
	}
}

// linearHits resolves the foes this advance crosses, in (along, index)
// order, delivering the payload with decay until the pierce budget is
// spent. Foes are units on a team different from the live launcher's;
// if the launcher is gone the missile classifies nothing (fail-closed)
// and flies through. Zero alloc.
func (w *World) linearHits(id EntityID, r int32, pos fixed.Vec2, dx, dy, stepL, radiusSqL2 int64) {
	s := w.Missiles
	sor := w.Owners.Row(s.Source[r])
	if sor == -1 {
		return // launcher dead/unowned: no foe team to test against
	}
	team := w.Owners.Team[sor]
	radius := fixed.FromInt(missileHitRadius)
	nextPos := pos.Add(w.Missiles.Dir[r].Scale(stepF64(s.Speed[r], s.RangeLeft[r])))
	x0 := bucketCoord(minF(pos.X, nextPos.X).Sub(radius))
	x1 := bucketCoord(maxF(pos.X, nextPos.X).Add(radius))
	y0 := bucketCoord(minF(pos.Y, nextPos.Y).Sub(radius))
	y1 := bucketCoord(maxF(pos.Y, nextPos.Y).Add(radius))
	px, py := pos.X.Floor(), pos.Y.Floor()

	lastAlong, lastIdx := int64(-1), int64(-1) // cursor: pick strictly after
	for s.PierceLeft[r] > 0 {
		var (
			bestAlong = int64(1) << 62
			bestIdx   = int64(1) << 62
			best      EntityID
			bestAt    fixed.Vec2
			found     bool
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
						continue // not damageable
					}
					ctr := w.Transforms.Row(cid)
					if ctr == -1 {
						continue
					}
					cp := w.Transforms.Pos[ctr]
					relx, rely := cp.X.Floor()-px, cp.Y.Floor()-py
					along := relx*dx + rely*dy
					if along < 0 || along >= stepL {
						continue // not within this advance's swept window
					}
					perp := (relx*relx+rely*rely)*(dx*dx+dy*dy) - along*along
					if perp > radiusSqL2 {
						continue // outside the lateral collision radius
					}
					cIdx := int64(cid.Index())
					if along < lastAlong || (along == lastAlong && cIdx <= lastIdx) {
						continue // already delivered to (cursor discipline)
					}
					if !found || along < bestAlong || (along == bestAlong && cIdx < bestIdx) {
						found, bestAlong, bestIdx, best, bestAt = true, along, cIdx, cid, cp
					}
				}
			}
		}
		if !found {
			return
		}
		w.deliverLinearHit(id, r, best, bestAt)
		lastAlong, lastIdx = bestAlong, bestIdx
		s.PierceLeft[r]--
		if s.Payload[r].Len == 0 { // decay the scalar payload for the next hit
			s.Packet[r].Amount = scalePermille(s.Packet[r].Amount, s.Decay[r])
		}
	}
}

// deliverLinearHit applies the (already decay-scaled) payload to one
// pierced victim and signals the impact.
func (w *World) deliverLinearHit(id EntityID, r int32, victim EntityID, at fixed.Vec2) {
	s := w.Missiles
	if lst := s.Payload[r]; lst.Len > 0 {
		w.ExecuteEffects(lst, EffectCtx{Source: s.Source[r], Target: victim, Point: at})
	} else {
		p := s.Packet[r]
		p.Target = victim
		if p.Target != 0 {
			w.QueueDamage(p)
		}
	}
	if w.OnMissileImpact != nil {
		w.OnMissileImpact(w.tick, id, at, victim)
	}
}

// stepF64 clamps the advance distance to the remaining range.
func stepF64(speed, rangeLeft fixed.F64) fixed.F64 {
	if rangeLeft < speed {
		return rangeLeft
	}
	return speed
}

func scalePermille(amt fixed.F64, permille uint16) fixed.F64 {
	return fixed.F64(int64(amt) * int64(permille) / 1000)
}

func minF(a, b fixed.F64) fixed.F64 {
	if a < b {
		return a
	}
	return b
}

func maxF(a, b fixed.F64) fixed.F64 {
	if a > b {
		return a
	}
	return b
}

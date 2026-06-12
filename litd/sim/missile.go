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
}

// SpawnMissile creates a missile entity. Fails deterministically
// (0, false) on pool exhaustion or a bad spec — NEVER a silent
// fire-as-instant fallback (ecs §2).
func (w *World) SpawnMissile(m MissileSpec) (EntityID, bool) {
	if m.Speed <= 0 {
		return 0, false
	}
	if m.Target != 0 && !w.Ents.Alive(m.Target) {
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

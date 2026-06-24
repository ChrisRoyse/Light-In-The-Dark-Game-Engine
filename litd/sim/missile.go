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
	Pos        fixed.Vec2
	Source     EntityID
	Target     EntityID
	Point      fixed.Vec2
	Speed      fixed.F64 // world units per tick, must be positive
	Accel      fixed.F64 // world units per tick^2, non-negative; applied after each tick
	Arc        fixed.F64
	Flags      uint8
	HitMask    uint16 // MissileHit* bits; 0 = enemy-only, no class filter
	GuidanceID uint16 // MissileGuidance*; 0 infers from Target/Point/Linear
	ImpactID   uint16 // MissileImpact*; 0 infers from flags/linear fields
	Payload    data.EffectList
	Packet     DamagePacket

	// Linear skillshot fields (Flags&MissileLinear).
	Dir    fixed.Vec2 // flight direction (need not be normalized; SpawnMissile normalizes)
	Range  fixed.F64  // max travel before expiry, must be positive
	Pierce int32      // max hits (≥1); 1 = single-target skillshot
	Decay  uint16     // per-mille payload decay between pierce hits (0 → 1000 = none)
}

// EvMissileImpact fires when a missile delivers its payload — Src is
// the missile, Dst the victim/target (0 for a point or AoE detonation).
// EvMissileExpired fires when a missile dies without delivering — Src
// is the missile, Dst is 0. Both reach scripts through the OnEvent
// dispatcher (#293); they are emitted alongside the OnMissile*
// presentation callbacks.
const (
	EvMissileImpact  uint16 = 22
	EvMissileExpired uint16 = 23
)

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
	if m.Speed <= 0 || m.Accel < 0 {
		return 0, false
	}
	guidanceID, ok := normalizeMissileGuidance(m)
	if !ok {
		return 0, false
	}
	impactID, ok := normalizeMissileImpact(m, guidanceID)
	if !ok {
		return 0, false
	}
	hitMask, ok := normalizeMissileHitMask(m.HitMask)
	if !ok {
		return 0, false
	}
	if impactID == MissileImpactDetonate {
		m.Flags |= MissileAoE
	}
	var dir fixed.Vec2
	switch guidanceID {
	case MissileGuidanceLinear:
		if m.Flags&MissileLinear == 0 {
			return 0, false
		}
		// skillshot: validated independently of a guide target
		dir = unitStep(m.Dir, fixed.FromInt(1)) // normalize to unit length
		if dir == (fixed.Vec2{}) || m.Range <= 0 || m.Pierce < 1 {
			return 0, false // degenerate skillshot: deterministic failure
		}
	case MissileGuidanceHoming:
		if m.Flags&MissileLinear != 0 || m.Target == 0 || !w.Ents.Alive(m.Target) {
			return 0, false // launch at a corpse resolves to nothing
		}
	case MissileGuidancePoint:
		if m.Flags&MissileLinear != 0 || m.Target != 0 {
			return 0, false
		}
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
	s.Accel[r] = m.Accel
	s.Arc[r] = m.Arc
	s.Flags[r] = m.Flags
	s.HitMask[r] = hitMask
	s.GuidanceID[r] = guidanceID
	s.ImpactID[r] = impactID
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
	// Span: total flight distance in whole world units, captured once at spawn as
	// the render-only arc-progress denominator (#528). A linear skillshot knows
	// its range up front; a point/homing missile measures launch->goal. Never
	// hashed (render-support; see MissileSnapEntry.LifeFrac).
	if m.Flags&MissileLinear != 0 {
		s.Span[r] = int32(m.Range.Floor())
	} else {
		s.Span[r] = int32(flightUnits(m.Pos, s.GuidePt[r]))
	}
	return id, true
}

// flightUnits is the whole-world-unit distance between two points — the
// render-only metric behind missile arc progress (#528). Coordinates are floored
// to whole units before squaring so the sum stays inside uint64 for any real map
// (a fixed-precision squared distance needs 128 bits; this render metric does
// not). Deterministic: integer ops + the sim's bit-exact SqrtU64.
func flightUnits(a, b fixed.Vec2) int64 {
	dx := a.X.Floor() - b.X.Floor()
	dy := a.Y.Floor() - b.Y.Floor()
	return int64(fixed.SqrtU64(uint64(dx*dx + dy*dy)))
}

func normalizeMissileGuidance(m MissileSpec) (uint16, bool) {
	if m.GuidanceID != MissileGuidanceInfer {
		switch m.GuidanceID {
		case MissileGuidanceHoming, MissileGuidancePoint, MissileGuidanceLinear:
			return m.GuidanceID, true
		default:
			return 0, false
		}
	}
	if m.Flags&MissileLinear != 0 {
		return MissileGuidanceLinear, true
	}
	if m.Target != 0 {
		return MissileGuidanceHoming, true
	}
	return MissileGuidancePoint, true
}

func normalizeMissileImpact(m MissileSpec, guidanceID uint16) (uint16, bool) {
	if m.ImpactID != MissileImpactInfer {
		switch m.ImpactID {
		case MissileImpactDeliver, MissileImpactDetonate, MissileImpactPierce, MissileImpactExpire:
			if m.ImpactID == MissileImpactPierce && guidanceID != MissileGuidanceLinear {
				return 0, false
			}
			return m.ImpactID, true
		default:
			return 0, false
		}
	}
	if guidanceID == MissileGuidanceLinear && m.Pierce > 1 {
		return MissileImpactPierce, true
	}
	if m.Flags&MissileAoE != 0 {
		return MissileImpactDetonate, true
	}
	return MissileImpactDeliver, true
}

func normalizeMissileHitMask(mask uint16) (uint16, bool) {
	if mask == 0 {
		return MissileDefaultHitMask, true
	}
	if mask&^MissileHitAllMask != 0 {
		return 0, false
	}
	if mask&MissileHitRelationMask == 0 {
		mask |= MissileHitEnemy
	}
	return mask, true
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
				w.Emit(Event{Kind: EvMissileExpired, Src: id})
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
		w.accelerateMissile(r)
	}
}

// impactMissile executes the payload at the impact point and marks
// the missile for standard deferred removal.
func (w *World) impactMissile(id EntityID, r int32, at fixed.Vec2) {
	s := w.Missiles
	if s.ImpactID[r] == MissileImpactExpire {
		w.expireMissileAt(id, at)
		return
	}
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
	// Presentation cue (non-hashing): the impact point won't be in the next
	// snapshot — the missile dies this tick — so carry it on the event (#309).
	w.EmitRenderEventAt(RenderMissileImpact, id, s.ImpactID[r], at)
	w.Emit(Event{Kind: EvMissileImpact, Src: id, Dst: tgt})
	w.KillUnit(id)
}

func (w *World) expireMissileAt(id EntityID, at fixed.Vec2) {
	if w.OnMissileExpire != nil {
		w.OnMissileExpire(w.tick, id, at)
	}
	w.Emit(Event{Kind: EvMissileExpired, Src: id})
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
		if w.linearHits(id, r, pos, dx, dy, stepL, radiusSqL2) {
			return
		}
	}

	// advance the body and spend range; die when pierce or range is out.
	w.Transforms.Pos[tr] = pos.Add(dir.Scale(step))
	s.RangeLeft[r] = s.RangeLeft[r].Sub(step)
	if s.PierceLeft[r] <= 0 || s.RangeLeft[r] <= 0 {
		w.expireMissileAt(id, w.Transforms.Pos[tr])
		return
	}
	w.accelerateMissile(r)
}

// linearHits resolves the foes this advance crosses, in (along, index)
// order, delivering the payload with decay until the pierce budget is
// spent. Foes are units on a team different from the live launcher's;
// if the launcher is gone the missile classifies nothing (fail-closed)
// and flies through. Zero alloc.
func (w *World) linearHits(id EntityID, r int32, pos fixed.Vec2, dx, dy, stepL, radiusSqL2 int64) bool {
	s := w.Missiles
	sor := w.Owners.Row(s.Source[r])
	if sor == -1 {
		return false // launcher dead/unowned: no foe team to test against
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
					if w.Healths.Row(cid) == -1 {
						continue // not damageable
					}
					if !w.missileHitAllowed(cid, team, s.HitMask[r]) {
						continue
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
			return false
		}
		if s.ImpactID[r] == MissileImpactExpire {
			if tr := w.Transforms.Row(id); tr != -1 {
				w.Transforms.Pos[tr] = bestAt
			}
			w.expireMissileAt(id, bestAt)
			return true
		}
		w.deliverLinearHit(id, r, best, bestAt)
		lastAlong, lastIdx = bestAlong, bestIdx
		s.PierceLeft[r]--
		if s.Payload[r].Len == 0 { // decay the scalar payload for the next hit
			s.Packet[r].Amount = scalePermille(s.Packet[r].Amount, s.Decay[r])
		}
	}
	return false
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
	// Presentation cue (non-hashing); carry the impact point (#309).
	w.EmitRenderEventAt(RenderMissileImpact, id, s.ImpactID[r], at)
	w.Emit(Event{Kind: EvMissileImpact, Src: id, Dst: victim})
}

func (w *World) missileHitAllowed(target EntityID, sourceTeam uint8, mask uint16) bool {
	or := w.Owners.Row(target)
	if or == -1 {
		return false
	}
	sameTeam := w.Owners.Team[or] == sourceTeam
	if sameTeam {
		if mask&MissileHitAlly == 0 {
			return false
		}
	} else if mask&MissileHitEnemy == 0 {
		return false
	}
	classMask := mask & MissileHitClassMask
	if classMask == 0 {
		return true
	}
	return w.missileTargetClass(target)&classMask != 0
}

func (w *World) missileTargetClass(target EntityID) uint16 {
	if cr := w.Collisions.Row(target); cr != -1 {
		flags := w.Collisions.PathFlags[cr]
		if flags&PathBuild != 0 {
			return MissileHitStructure
		}
		if flags&PathAir != 0 {
			return MissileHitAir
		}
		if flags&PathGround != 0 {
			return MissileHitGround
		}
	}
	if ur := w.UnitTypes.Row(target); ur != -1 && int(w.UnitTypes.TypeID[ur]) < len(w.unitDefs) {
		def := &w.unitDefs[w.UnitTypes.TypeID[ur]]
		if def.Footprint > 0 || def.BuildTicks > 0 {
			return MissileHitStructure
		}
		if def.Pathing == data.PathingAir {
			return MissileHitAir
		}
		return MissileHitGround
	}
	return MissileHitGround
}

// DetonateMissile delivers a missile's payload at its current position
// immediately and removes it (deferred kill). Returns false on a
// non-missile or dead handle (R-API-5). Script-facing — Missile.Detonate
// (#293).
func (w *World) DetonateMissile(id EntityID) bool {
	if mr, ok := w.projMover(id); ok {
		// Mover-backed projectile (#590): deliver the payload once at the current
		// position, then consume the body — force DoneImpact so a linear/expire
		// projectile still delivers, matching impactMissile's immediate detonate.
		if !w.Ents.Alive(id) {
			return false
		}
		w.Movers.DoneMode[mr] = uint8(MoverDoneImpact)
		w.moverComplete(mr)
		return true
	}
	r := w.Missiles.Row(id)
	if r == -1 || !w.Ents.Alive(id) {
		return false
	}
	tr := w.Transforms.Row(id)
	if tr == -1 {
		return false
	}
	w.impactMissile(id, r, w.Transforms.Pos[tr])
	return true
}

// projMover resolves a projectile body entity to its driving mover row, or
// ok=false if id is not a live mover-backed projectile (#590).
func (w *World) projMover(id EntityID) (int32, bool) {
	pr := w.ProjRender.Row(id)
	if pr == -1 {
		return 0, false
	}
	return w.Movers.resolve(w.ProjRender.Mover[pr])
}

// ExpireMissile removes a missile payload-less and emits EvMissileExpired
// (plus the OnMissileExpire callback). Returns false on a non-missile or
// dead handle. Script-facing — Missile.Expire (#293).
func (w *World) ExpireMissile(id EntityID) bool {
	if mr, ok := w.projMover(id); ok {
		if !w.Ents.Alive(id) {
			return false
		}
		// DoneExpire -> moverComplete -> projectileExpire fires OnMissileExpire +
		// EvMissileExpired once (do not also fire here, #590 — that double-cued).
		w.Movers.DoneMode[mr] = uint8(MoverDoneExpire)
		w.moverComplete(mr)
		return true
	}
	r := w.Missiles.Row(id)
	if r == -1 || !w.Ents.Alive(id) {
		return false
	}
	last := w.Missiles.GuidePt[r]
	if tr := w.Transforms.Row(id); tr != -1 {
		last = w.Transforms.Pos[tr]
	}
	if w.OnMissileExpire != nil {
		w.OnMissileExpire(w.tick, id, last)
	}
	w.Emit(Event{Kind: EvMissileExpired, Src: id})
	w.KillUnit(id)
	return true
}

// SetMissileTarget retargets a missile mid-flight to home on target
// (refreshing the goal to its live position), or toward its current goal
// point when target == 0. Retargeting converts a linear skillshot into a
// guided missile. Returns false on a non-missile/dead handle or a dead
// target. Script-facing — Missile.SetTarget (#293).
func (w *World) SetMissileTarget(id, target EntityID) bool {
	if mr, ok := w.projMover(id); ok {
		if !w.Ents.Alive(id) || (target != 0 && !w.Ents.Alive(target)) {
			return false
		}
		ms := w.Movers
		ms.Flags[mr] &^= MoverSwept // a retargeted projectile is guided, not a skillshot
		ms.DoneMode[mr] = uint8(MoverDoneImpact)
		if target != 0 {
			ms.Kind[mr] = uint8(MoverHoming)
			ms.Anchor[mr] = target
			ms.TurnRate[mr] = 0
		} else {
			ms.Kind[mr] = uint8(MoverPoint)
			ms.Anchor[mr] = 0
			// A former skillshot has no goal point — aim at the current position
			// so it arrives rather than flying to the origin (degenerate legacy
			// linear->point case); a point mover keeps its existing Goal.
			if ms.Goal[mr] == (fixed.Vec2{}) {
				if tr := w.Transforms.Row(id); tr != -1 {
					ms.Goal[mr] = w.Transforms.Pos[tr]
				}
			}
		}
		return true
	}
	r := w.Missiles.Row(id)
	if r == -1 || !w.Ents.Alive(id) {
		return false
	}
	if target != 0 && !w.Ents.Alive(target) {
		return false
	}
	s := w.Missiles
	s.Flags[r] &^= MissileLinear // a retargeted missile is guided, not a skillshot
	s.GuideEnt[r] = target
	if target != 0 {
		s.GuidanceID[r] = MissileGuidanceHoming
		if tr := w.Transforms.Row(target); tr != -1 {
			s.GuidePt[r] = w.Transforms.Pos[tr]
		}
	} else {
		s.GuidanceID[r] = MissileGuidancePoint
	}
	if s.ImpactID[r] == MissileImpactPierce {
		s.ImpactID[r] = MissileImpactDeliver
	}
	return true
}

func (w *World) accelerateMissile(r int32) {
	accel := w.Missiles.Accel[r]
	if accel == 0 {
		return
	}
	speed := w.Missiles.Speed[r]
	if fixed.MaxF64.Sub(speed) < accel {
		w.Missiles.Speed[r] = fixed.MaxF64
		return
	}
	w.Missiles.Speed[r] = speed.Add(accel)
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

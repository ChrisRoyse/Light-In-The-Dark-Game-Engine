package sim

// Mover-backed projectile launch (#590). spawnMoverProjectile is the
// replacement engine under SpawnMissile: instead of a MissileStore row it
// builds a lightweight body entity, a mover that drives the body by the
// missile's guidance mode, and a render-only ProjRender record. The legacy
// MissileSpec is mapped 1:1 onto a MoverSpec so the public launch API and
// damage/effect semantics are preserved; the flight + collision + delivery
// then run entirely on the unified mover system (epic #548).
//
// Mode mapping (validated identically to the legacy path via the shared
// normalize* helpers in missile.go):
//
//	linear skillshot -> MoverLinear | MoverSwept | MoverConsume, DoneExpire
//	                    (swept segment collision delivers per pierce hit en
//	                    route — the #620 port of advanceLinear; completion
//	                    just frees the spent body).
//	point            -> MoverPoint | MoverConsume, DoneImpact (single
//	                    ExecuteEffects at the goal — the missile impact model;
//	                    ImpactExpire -> DoneExpire, payload-less).
//	homing           -> MoverHoming | MoverConsume, DoneImpact, Anchor=Target,
//	                    TurnRate 0 (instant tracking, byte-identical to the
//	                    missile's beeline). ImpactExpire -> DoneExpire.
//
// Guide-death semantics follow #626 decision (1): a homing projectile whose
// guide dies delivers its payload at the last-pursued point (DoneImpact,
// tgt=0), unifying the legacy AoE/non-AoE split — non-AoE homing no longer
// fizzles payload-less. Intentional, documented divergence from the legacy
// missile (missile.go:259-266); covered by the parity test.
//
// Damage source: the mover's Owner carries the launcher (m.Source) for
// attribution (moverImpact/moverDetonate read Owner). Projectile movers are
// exempt from CancelOwnedBy (mover.go), so the launcher dying never aborts a
// shot in flight — the missile model.

import "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"

func (w *World) spawnMoverProjectile(m MissileSpec) (EntityID, bool) {
	// Validation + normalization: byte-for-byte the legacy SpawnMissile front
	// matter, so a spec that the missile path rejected the mover path rejects
	// identically (deterministic (0,false), never a silent instant fallback).
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
	var dir fixed.Vec2
	switch guidanceID {
	case MissileGuidanceLinear:
		if m.Flags&MissileLinear == 0 {
			return 0, false
		}
		dir = unitStep(m.Dir, fixed.FromInt(1)) // normalize to unit length
		if dir == (fixed.Vec2{}) || m.Range <= 0 || m.Pierce < 1 {
			return 0, false
		}
	case MissileGuidanceHoming:
		if m.Flags&MissileLinear != 0 || m.Target == 0 || !w.Ents.Alive(m.Target) {
			return 0, false
		}
	case MissileGuidancePoint:
		if m.Flags&MissileLinear != 0 || m.Target != 0 {
			return 0, false
		}
	}

	// Build the mover spec by mode before touching the entity pool, so a bad
	// mapping can't leak a half-spawned body.
	spec := MoverSpec{
		Target:  0, // filled after the body exists
		Owner:   m.Source,
		Speed:   m.Speed,
		Accel:   m.Accel,
		HitMask: hitMask,
		Payload: m.Payload,
		Packet:  m.Packet,
		Flags:   MoverConsume,
	}
	// Decay convention bridge (#590): MissileSpec.Decay is KEEP-per-mille with 0
	// as the "no decay" sentinel (stored 1000 = keep all). MoverSpec.Decay is
	// REDUCTION-per-mille (0 = no decay), what moverDecay applies. Convert
	// keep->reduction so a piercing skillshot decays identically via either path.
	keep := m.Decay
	if keep == 0 {
		keep = 1000 // missile sentinel: 0 means keep everything
	}
	decay := uint16(1000 - keep)
	var span int32
	switch guidanceID {
	case MissileGuidanceLinear:
		spec.Kind = MoverLinear
		spec.Dir = dir
		spec.RangeLeft = m.Range
		spec.Radius = fixed.FromInt(missileHitRadius)
		spec.Pierce = m.Pierce
		spec.Decay = decay
		spec.Flags |= MoverSwept
		spec.DoneMode = MoverDoneExpire
		span = int32(m.Range.Floor())
	case MissileGuidancePoint:
		spec.Kind = MoverPoint
		spec.Goal = m.Point
		if impactID == MissileImpactExpire {
			spec.DoneMode = MoverDoneExpire
		} else {
			spec.DoneMode = MoverDoneImpact
		}
		span = int32(flightUnits(m.Pos, m.Point))
	case MissileGuidanceHoming:
		spec.Kind = MoverHoming
		spec.Anchor = m.Target
		spec.TurnRate = 0
		if impactID == MissileImpactExpire {
			spec.DoneMode = MoverDoneExpire
		} else {
			spec.DoneMode = MoverDoneImpact
		}
		goal := m.Point
		if tr := w.Transforms.Row(m.Target); tr != -1 {
			goal = w.Transforms.Pos[tr]
		}
		span = int32(flightUnits(m.Pos, goal))
	}

	// Body entity: a transform-only mote (no unit components), bucketed and
	// snapshot-marked exactly like a missile body, NOT counted against the unit
	// cap (destroyEntity skips the decrement for a ProjRender body, #590).
	id, ok := w.Ents.Create()
	if !ok {
		return 0, false
	}
	if !w.Transforms.Add(w.Ents, id, m.Pos, 0) {
		w.Ents.Destroy(id)
		return 0, false
	}
	spec.Target = id
	mid := w.Movers.Create(spec)
	if mid == 0 {
		w.Transforms.Remove(id) // mover pool exhausted: roll the body back
		w.Ents.Destroy(id)
		return 0, false
	}
	w.bucketInsert(id, m.Pos)
	w.MarkSnap(id)
	w.ProjRender.Add(id, mid, m.Arc, guidanceID, span)
	return id, true
}

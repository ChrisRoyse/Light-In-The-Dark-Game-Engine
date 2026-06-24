package sim

// Mover advance — phase 4 (#584). Each tick, every live mover computes its
// next position by kind and writes its Target's transform. Linear/Point/
// Homing land here; Orbit/Arc/Spline (#585) and Custom (#586) extend the
// switch; collision (#587), unit authority (#588), and the completion
// policy (#589) layer on top. Reuses unitStep (movement.go) for one-tick
// normalized displacement, so a mover and a unit move with identical
// fixed-point quantization.

import "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"

// moverSystem advances every live mover one tick, in ascending slot order
// (deterministic). Called from phaseMovement before the bucket reconcile,
// so a moved transform is re-filed this tick.
func (w *World) moverSystem() {
	ms := w.Movers
	for r := int32(1); r < int32(len(ms.live)); r++ {
		if !ms.live[r] {
			continue
		}
		// Capture the pre-step position so a non-flying mover blocked by
		// terrain can be clamped back (#588).
		pre, _, hasPre := w.moverPos(r)
		swept := w.moverSwept(r)
		switch MoverKind(ms.Kind[r]) {
		case MoverLinear:
			// A swept skillshot collides over the step segment from the
			// pre-step position BEFORE advancing the body (#620), mirroring the
			// missile's advanceLinear order.
			if swept {
				w.moverSweptCollide(r)
			}
			if ms.live[r] {
				w.moverStepLinear(r)
			}
		case MoverPoint:
			w.moverStepPoint(r)
		case MoverHoming:
			w.moverStepHoming(r)
		case MoverOrbitUnit:
			w.moverStepOrbit(r, true)
		case MoverOrbitPoint:
			w.moverStepOrbit(r, false)
		case MoverArc:
			w.moverStepArc(r)
		case MoverSpline:
			w.moverStepSpline(r)
		case MoverCustom:
			w.moverStepCustom(r)
		}
		// Terrain block (#588): a non-flying mover may not push its Target
		// into impassable terrain — clamp back to the pre-step position.
		if ms.live[r] && hasPre {
			if tr := w.Transforms.Row(ms.Target[r]); tr != -1 {
				if to := w.Transforms.Pos[tr]; to != pre && w.moverTerrainBlocks(r, to) {
					w.moverWrite(tr, pre)
				}
			}
		}
		// Collision runs after the move, only if the step didn't already
		// complete the mover (#587). A consumed mover stops here. A swept
		// linear mover already collided pre-step (#620): skip the endpoint
		// test and instead complete (consume) when its pierce budget is spent.
		// Deliver-on-completion projectiles (DoneImpact / DoneDetonate) do NOT
		// take the en-route endpoint test — they deliver only at the goal, the
		// missile point/homing model; running it too would double-deliver to a
		// unit the projectile flies through before completing (#625).
		if ms.live[r] {
			switch {
			case swept:
				if ms.Pierce[r] <= 0 {
					w.moverComplete(r)
				}
			case MoverDoneMode(ms.DoneMode[r]) == MoverDoneImpact || MoverDoneMode(ms.DoneMode[r]) == MoverDoneDetonate:
				// delivery happens at completion; no en-route contact test
			default:
				w.moverCollide(r)
			}
		}
	}
}

// moverSwept reports whether a mover uses the swept-segment skillshot
// collision model (#620): a linear mover with the MoverSwept flag and an
// active hit mask. All other kinds (and non-swept linear) keep the endpoint
// radius test.
func (w *World) moverSwept(r int32) bool {
	ms := w.Movers
	return MoverKind(ms.Kind[r]) == MoverLinear && ms.Flags[r]&MoverSwept != 0 && ms.HitMask[r] != 0
}

// moverPos reads the mover's Target transform position; ok=false if the
// target has no transform (destroyed) — the caller completes the mover.
func (w *World) moverPos(r int32) (fixed.Vec2, int32, bool) {
	tr := w.Transforms.Row(w.Movers.Target[r])
	if tr == -1 {
		return fixed.Vec2{}, -1, false
	}
	return w.Transforms.Pos[tr], tr, true
}

// moverWrite stores the new position into the Target transform.
func (w *World) moverWrite(tr int32, p fixed.Vec2) { w.Transforms.Pos[tr] = p }

func (w *World) moverStepLinear(r int32) {
	ms := w.Movers
	pos, tr, ok := w.moverPos(r)
	if !ok {
		w.moverComplete(r)
		return
	}
	step := unitStep(ms.Dir[r], ms.Speed[r])
	w.moverWrite(tr, pos.Add(step))
	ms.RangeLeft[r] -= ms.Speed[r]
	if ms.RangeLeft[r] <= 0 {
		w.moverComplete(r)
		return
	}
	ms.Speed[r] += ms.Accel[r] // accelerate after the move (missile parity, #590)
}

func (w *World) moverStepPoint(r int32) {
	ms := w.Movers
	pos, tr, ok := w.moverPos(r)
	if !ok {
		w.moverComplete(r)
		return
	}
	to := ms.Goal[r].Sub(pos)
	// Arrived: within one tick's reach of the goal → snap + complete.
	if distLE(to, ms.Speed[r]) {
		w.moverWrite(tr, ms.Goal[r])
		w.moverComplete(r)
		return
	}
	w.moverWrite(tr, pos.Add(unitStep(to, ms.Speed[r])))
	ms.Speed[r] += ms.Accel[r] // accelerate after the move (missile parity, #590)
}

func (w *World) moverStepHoming(r int32) {
	ms := w.Movers
	pos, tr, ok := w.moverPos(r)
	if !ok {
		w.moverComplete(r)
		return
	}
	// Desired direction = toward the anchor's current position. A vanished
	// anchor leaves the current Dir unchanged (fly straight on).
	if ar := w.Transforms.Row(ms.Anchor[r]); ar != -1 {
		apos := w.Transforms.Pos[ar]
		// Projectile homing (MoverConsume) snap-arrives and completes when it
		// reaches the live anchor (#590) — the missile homing-impact model,
		// using the same within-one-step predicate (distLE vs Speed) as the
		// missile's snap-arrive. A non-consume homing mover (a guided unit)
		// keeps pursuing past the anchor, unchanged.
		if ms.Flags[r]&MoverConsume != 0 {
			// Track the guide's last-known position so an AoE projectile can
			// coast to it and detonate there if the guide dies (#590 missile
			// GuidePt parity — see the dead-anchor branch below).
			ms.Goal[r] = apos
			if distLE(apos.Sub(pos), ms.Speed[r]) {
				w.moverWrite(tr, apos)
				w.moverComplete(r)
				return
			}
		}
		desired := apos.Sub(pos)
		if desired.X != 0 || desired.Y != 0 {
			if ms.TurnRate[r] == 0 {
				// Instant turn: beeline along the raw desired vector. unitStep
				// normalizes it, so this is byte-identical to a point/missile
				// step — NO angle-LUT round-trip (the #593 parity fix; the LUT
				// quantizes to 16-bit BAM and would drift the low bits).
				ms.Dir[r] = desired
			} else {
				ms.Dir[r] = w.turnToward(ms.Dir[r], desired, ms.TurnRate[r])
			}
		}
	} else if ms.Flags[r]&MoverConsume != 0 && ms.Anchor[r] != 0 {
		// The guide died mid-flight (#590/#626, missile guide-invalidation model).
		// A non-AoE homing projectile (MoverExpireOnGuideLoss) fizzles payload-less
		// — the legacy ImpactDeliver missile behavior. An AoE homing projectile
		// (no flag) completes via DoneImpact, delivering its payload at the current
		// (last-pursued) position — the legacy MissileAoE coast-and-detonate (the
		// body is already adjacent to the last-known point, within a sub-step).
		if ms.Flags[r]&MoverExpireOnGuideLoss != 0 {
			w.projectileExpire(r)
			if ms.Flags[r]&MoverConsume != 0 {
				w.KillUnit(ms.Target[r])
			}
			w.moverFree(r)
			return
		}
		// An expire-mode projectile (DoneExpire) dies in place, payload-less —
		// no last-point delivery, so do not coast.
		if MoverDoneMode(ms.DoneMode[r]) == MoverDoneExpire {
			w.moverComplete(r)
			return
		}
		// AoE: coast toward the guide's last-known position (tracked in Goal)
		// and detonate on arrival — the legacy MissileAoE coast-and-detonate at
		// GuidePt, NOT an in-place detonation where the body happened to be when
		// the guide died (which would splash the wrong area, #590).
		to := ms.Goal[r].Sub(pos)
		if distLE(to, ms.Speed[r]) {
			w.moverWrite(tr, ms.Goal[r])
			w.moverComplete(r)
			return
		}
		w.moverWrite(tr, pos.Add(unitStep(to, ms.Speed[r])))
		ms.Speed[r] += ms.Accel[r]
		return
	}
	w.moverWrite(tr, pos.Add(unitStep(ms.Dir[r], ms.Speed[r])))
	ms.Speed[r] += ms.Accel[r] // accelerate after the move (missile parity, #590)
}

// turnToward rotates cur toward the direction of want by at most maxTurn
// (a BAM angle); maxTurn==0 means snap instantly. Deterministic — angles
// via the committed LUT (Atan2/UnitVec).
func (w *World) turnToward(cur, want fixed.Vec2, maxTurn fixed.Angle) fixed.Vec2 {
	wantAng := fixed.Atan2(want.Y, want.X)
	if maxTurn == 0 {
		return wantAng.UnitVec()
	}
	curAng := fixed.Atan2(cur.Y, cur.X)
	// Shortest signed delta on the wrapping circle (int16 of the BAM diff).
	delta := int16(uint16(wantAng) - uint16(curAng))
	mt := int32(maxTurn)
	d := int32(delta)
	if d > mt {
		d = mt
	} else if d < -mt {
		d = -mt
	}
	return (curAng + fixed.Angle(int16(d))).UnitVec()
}

// distLE reports whether |v| <= r, comparing squared magnitudes in 128
// bits (no sqrt, no overflow) via the same fixed helpers the spatial
// queries use (queries.go). Negative r is never <= a magnitude.
func distLE(v fixed.Vec2, r fixed.F64) bool {
	if r < 0 {
		return false
	}
	dHi, dLo := fixed.DistSq(fixed.Vec2{}, v)
	rHi, rLo := fixed.RadiusSq(r)
	if dHi != rHi {
		return dHi < rHi
	}
	return dLo <= rLo
}

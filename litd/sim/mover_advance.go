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
		switch MoverKind(ms.Kind[r]) {
		case MoverLinear:
			w.moverStepLinear(r)
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
		}
	}
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
	}
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
		desired := w.Transforms.Pos[ar].Sub(pos)
		if desired.X != 0 || desired.Y != 0 {
			ms.Dir[r] = w.turnToward(ms.Dir[r], desired, ms.TurnRate[r])
		}
	}
	w.moverWrite(tr, pos.Add(unitStep(ms.Dir[r], ms.Speed[r])))
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

// moverComplete handles a mover that finished its motion this tick. #584
// seam: expire (free the mover). The full completion policy — loop /
// detonate / cont, plus consume-on-impact — is layered in by #589, which
// replaces this body with a DoneMode dispatch.
func (w *World) moverComplete(r int32) {
	ms := w.Movers
	ms.live[r] = false
	ms.Gen[r]++
	ms.free = append(ms.free, r)
	ms.count--
}

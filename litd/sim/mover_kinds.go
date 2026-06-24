package sim

// Orbit / Arc / Spline mover steps (#585) — the curved-motion kinds, all
// fixed-point + LUT-based (R-MOV-3). They extend the phase-4 switch in
// moverSystem (#584). Gameplay z (when flying) lives in the Height column,
// written each tick by Arc (ballistic parabola) and held constant by
// Orbit; collision (#587) and render (#590) read it.

import "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"

// orbit-anchor center: the anchored unit's live position; a vanished
// anchor freezes the orbit at its last center via Goal (set on creation).
func (w *World) moverOrbitCenter(r int32, unit bool) fixed.Vec2 {
	ms := w.Movers
	if unit {
		if ar := w.Transforms.Row(ms.Anchor[r]); ar != -1 {
			return w.Transforms.Pos[ar]
		}
	}
	return ms.Goal[r]
}

// moverStepOrbit circles the center at AngVel·tick, radius Radius. Orbit is
// continuous — it never self-completes (cancel via owner death / explicit).
func (w *World) moverStepOrbit(r int32, unit bool) {
	ms := w.Movers
	tr := w.Transforms.Row(ms.Target[r])
	if tr == -1 {
		w.moverComplete(r)
		return
	}
	ms.Angle[r] += ms.AngVel[r]
	center := w.moverOrbitCenter(r, unit)
	w.moverWrite(tr, center.Add(ms.Angle[r].UnitVec().Scale(ms.Radius[r])))
	// Height column holds the (constant) orbit z — already set on creation.
}

// moverStepArc advances a ballistic arc toward Goal at horizontal Speed,
// writing the parabolic gameplay z into Height each tick. Parametrization
// (creator-supplied, #591): CState[0] = total horizontal distance,
// CState[1] = apex height. z = 4·apex·t·(1−t), t = traveled/total. Arrives
// (snap + complete) within one tick's reach of Goal.
func (w *World) moverStepArc(r int32) {
	ms := w.Movers
	pos, tr, ok := w.moverPos(r)
	if !ok {
		w.moverComplete(r)
		return
	}
	to := ms.Goal[r].Sub(pos)
	total := fixed.F64(ms.CState[r][0])
	apex := fixed.F64(ms.CState[r][1])
	if distLE(to, ms.Speed[r]) {
		w.moverWrite(tr, ms.Goal[r])
		ms.Height[r] = 0 // landed
		w.moverComplete(r)
		return
	}
	next := pos.Add(unitStep(to, ms.Speed[r]))
	w.moverWrite(tr, next)
	// t = traveled/total = 1 - remaining/total. remaining ≈ |Goal-next|.
	if total > 0 {
		rem := ms.Goal[r].Sub(next)
		remLen := vecLen(rem)
		t := fixed.One - remLen.Mul(fixedInv(total))
		ms.Height[r] = arcZ(apex, t)
	}
}

// arcZ = 4·apex·t·(1−t): 0 at t∈{0,1}, apex at t=0.5. Clamped to t∈[0,1].
func arcZ(apex, t fixed.F64) fixed.F64 {
	if t < 0 {
		t = 0
	} else if t > fixed.One {
		t = fixed.One
	}
	four := fixed.FromInt(4)
	return four.Mul(apex).Mul(t).Mul(fixed.One - t)
}

// moverStepSpline advances WpParam by Speed (param units/tick) and writes
// the Catmull-Rom point through the waypoint span. Completes (snap to last)
// when the param reaches the final segment end.
func (w *World) moverStepSpline(r int32) {
	ms := w.Movers
	tr := w.Transforms.Row(ms.Target[r])
	if tr == -1 {
		w.moverComplete(r)
		return
	}
	n := ms.WpLen[r]
	if n < 2 {
		w.moverComplete(r) // degenerate spline
		return
	}
	last := fixed.FromInt(n - 1)
	ms.WpParam[r] += ms.Speed[r]
	if ms.WpParam[r] >= last {
		w.moverWrite(tr, ms.waypoints[ms.WpStart[r]+n-1])
		w.moverComplete(r)
		return
	}
	w.moverWrite(tr, w.catmullRom(r, ms.WpParam[r]))
}

// catmullRom samples the spline at param p∈[0, n-1] over the mover's
// waypoint span (endpoints duplicated for the phantom control points).
func (w *World) catmullRom(r int32, p fixed.F64) fixed.Vec2 {
	ms := w.Movers
	start, n := ms.WpStart[r], ms.WpLen[r]
	seg := int32(p >> 32) // floor(p)
	if seg > n-2 {
		seg = n - 2
	}
	t := p - fixed.FromInt(seg) // local 0..1
	at := func(i int32) fixed.Vec2 {
		if i < 0 {
			i = 0
		} else if i > n-1 {
			i = n - 1
		}
		return ms.waypoints[start+i]
	}
	p0, p1, p2, p3 := at(seg-1), at(seg), at(seg+1), at(seg+2)
	t2 := t.Mul(t)
	t3 := t2.Mul(t)
	half := fixedInv(fixed.FromInt(2))
	// 0.5 * (2P1 + (-P0+P2)t + (2P0-5P1+4P2-P3)t² + (-P0+3P1-3P2+P3)t³)
	c0 := p1.Scale(fixed.FromInt(2))
	c1 := p2.Sub(p0).Scale(t)
	c2 := p0.Scale(fixed.FromInt(2)).Sub(p1.Scale(fixed.FromInt(5))).Add(p2.Scale(fixed.FromInt(4))).Sub(p3).Scale(t2)
	c3 := p1.Scale(fixed.FromInt(3)).Sub(p0).Sub(p2.Scale(fixed.FromInt(3))).Add(p3).Scale(t3)
	return c0.Add(c1).Add(c2).Add(c3).Scale(half)
}

// vecLen returns |v| in 32.32 via the integer sqrt path (deterministic).
// LenSq raw = realLenSq·2^32, so |v|_raw = sqrt(LenSqRaw)·2^16.
func vecLen(v fixed.Vec2) fixed.F64 {
	return fixed.F64(uint64(fixed.SqrtU64(uint64(v.LenSq()))) << 16)
}

// fixedInv returns One/x in 32.32 (x != 0). Used for the param ratio.
func fixedInv(x fixed.F64) fixed.F64 { return fixed.One.Div(x) }

package sim

// Patrol / follow / hold order behaviors (#306, combat-and-orders.md
// §2.1–2.3, §3.1). The order machinery (orders.go) and the attack
// cycle (attack.go) already exist; this file adds the three remaining
// movement-order state machines and the PatrolStore that holds a
// patroller's two endpoints.
//
//   Patrol — ping-pong between the issue point (captured at order
//     start) and the target point. An auto-acquire stance: while
//     patrolling the unit grabs hostile targets and the attack cycle
//     chases them, but only out to a leash distance from the patrol
//     SEGMENT; past the leash it drops the target, returns to the
//     nearest point on the segment, and resumes the ping-pong.
//   Follow — track a target entity with re-path hysteresis (re-path
//     only when the target has moved past a threshold from the last
//     pathed-to point). Never auto-engages (WC3). Target death stops.
//   Hold — handled in attack.go: auto-acquire + fire in range, but the
//     chase movement is suppressed (no reposition, no leash).
//
// Leash and re-path thresholds are world configuration (defaults
// below), deterministic and in fixed-point. Zero allocation.

import "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"

// Order kinds appended for #306 (after OrderHarvest=6 #300,
// OrderPickup=7 #305). Append-only: prior values stay stable so
// recorded replays keep decoding.
const (
	OrderPatrol uint8 = 8
	OrderFollow uint8 = 9
)

// PatrolStore flag bits.
const (
	patrolLegToA    uint8 = 1 << 0 // current ping-pong destination is A (else B)
	patrolReturning uint8 = 1 << 1 // leashed: walking back to the segment
	patrolChasing   uint8 = 1 << 2 // engaged a target last tick (attack drives the feet)
)

// DefaultPatrolLeash / DefaultFollowRepath are the config defaults
// (world units) until a data table supplies per-unit values.
const (
	DefaultPatrolLeash  = 400
	DefaultFollowRepath = 96
)

// ---- patrol store (T2 pattern) ----

type PatrolStore struct {
	A      []fixed.Vec2 // issue-point endpoint
	B      []fixed.Vec2 // target-point endpoint
	Flags  []uint8
	Entity []EntityID

	rowOf []int32
	count int32
}

func NewPatrolStore(rowCap, entityCap int) *PatrolStore {
	if rowCap <= 0 || entityCap <= 0 || rowCap > entityCap {
		panic("sim: store caps must satisfy 0 < rowCap <= entityCap")
	}
	s := &PatrolStore{
		A:      make([]fixed.Vec2, rowCap),
		B:      make([]fixed.Vec2, rowCap),
		Flags:  make([]uint8, rowCap),
		Entity: make([]EntityID, rowCap),
		rowOf:  make([]int32, entityCap),
	}
	for i := range s.rowOf {
		s.rowOf[i] = -1
	}
	return s
}

func (s *PatrolStore) add(e *Entities, id EntityID, a, b fixed.Vec2) int32 {
	if !e.Alive(id) || int(s.count) == len(s.A) {
		return -1
	}
	r := s.rowOf[id.Index()]
	if r != -1 { // re-issue: overwrite the existing row
		s.A[r], s.B[r], s.Flags[r] = a, b, 0
		return r
	}
	r = s.count
	s.A[r], s.B[r], s.Flags[r], s.Entity[r] = a, b, 0, id
	s.rowOf[id.Index()] = r
	s.count++
	return r
}

func (s *PatrolStore) Remove(id EntityID) bool {
	r := s.rowOf[id.Index()]
	if r == -1 {
		return false
	}
	last := s.count - 1
	s.A[r] = s.A[last]
	s.B[r] = s.B[last]
	s.Flags[r] = s.Flags[last]
	s.Entity[r] = s.Entity[last]
	s.rowOf[s.Entity[r].Index()] = r
	s.rowOf[id.Index()] = -1
	s.count--
	return true
}

func (s *PatrolStore) Row(id EntityID) int32 {
	if int(id.Index()) >= len(s.rowOf) {
		return -1
	}
	return s.rowOf[id.Index()]
}
func (s *PatrolStore) Count() int32 { return s.count }

// ---- world config ----

// SetPatrolLeash / SetFollowRepath override the behavior thresholds
// (world units). Zero restores the default.
func (w *World) SetPatrolLeash(worldUnits int32) {
	if worldUnits <= 0 {
		worldUnits = DefaultPatrolLeash
	}
	w.patrolLeash = fixed.FromInt(worldUnits)
}

func (w *World) SetFollowRepath(worldUnits int32) {
	if worldUnits <= 0 {
		worldUnits = DefaultFollowRepath
	}
	w.followRepath = fixed.FromInt(worldUnits)
}

func (w *World) patrolLeashDist() fixed.F64 {
	if w.patrolLeash == 0 {
		return fixed.FromInt(DefaultPatrolLeash)
	}
	return w.patrolLeash
}

func (w *World) followRepathDist() fixed.F64 {
	if w.followRepath == 0 {
		return fixed.FromInt(DefaultFollowRepath)
	}
	return w.followRepath
}

// patrolReturningRow reports whether the unit is a patroller currently
// leashing back to its segment (acquisition suppression).
func (w *World) patrolReturningRow(id EntityID) bool {
	pr := w.Patrol.Row(id)
	return pr != -1 && w.Patrol.Flags[pr]&patrolReturning != 0
}

// ---- patrol drive (ordersSystem phase 3) ----

func (w *World) drivePatrol(r int32, id EntityID) {
	s := w.Orders
	mr := w.Movements.Row(id)
	tr := w.Transforms.Row(id)
	if mr == -1 || tr == -1 {
		return // no body to patrol; hold the order (visible state)
	}
	if s.Phase[r] == orderFresh {
		pr := w.Patrol.add(w.Ents, id, w.Transforms.Pos[tr], s.Point[r])
		if pr == -1 {
			w.completeOrder(r, id, false)
			return
		}
		w.Patrol.Flags[pr] = 0 // leg → B
		w.StartMoveTo(id, w.Patrol.B[pr])
		s.Phase[r] = orderRunning
		return
	}
	pr := w.Patrol.Row(id)
	if pr == -1 { // store evicted (cap pressure): fail visibly
		w.completeOrder(r, id, false)
		return
	}
	pos := w.Transforms.Pos[tr]
	dest := w.patrolLeg(pr)

	if w.Patrol.Flags[pr]&patrolReturning != 0 {
		switch w.Movements.State[mr] {
		case MoveIdle, MoveBlocked: // back on the segment: resume the leg
			w.Patrol.Flags[pr] &^= patrolReturning
			w.StartMoveTo(id, dest)
		}
		return
	}

	// engaged? the attack cycle owns the feet while a target stands.
	if cr := w.Combats.Row(id); cr != -1 && w.Combats.Target[cr] != 0 &&
		w.validAcquireTarget(id, w.Combats.Target[cr]) {
		w.Patrol.Flags[pr] |= patrolChasing
		if w.segDistFarther(pos, w.Patrol.A[pr], w.Patrol.B[pr], w.patrolLeashDist()) {
			w.Combats.Target[cr] = 0 // leash break
			w.Patrol.Flags[pr] |= patrolReturning
			w.StartMoveTo(id, w.segNearest(pos, w.Patrol.A[pr], w.Patrol.B[pr]))
		}
		return
	}

	// just disengaged: resume the leg from wherever the chase left us.
	if w.Patrol.Flags[pr]&patrolChasing != 0 {
		w.Patrol.Flags[pr] &^= patrolChasing
		w.StartMoveTo(id, dest)
		return
	}

	// normal ping-pong: at an endpoint (idle/blocked) → flip and walk.
	switch w.Movements.State[mr] {
	case MoveIdle, MoveBlocked:
		w.Patrol.Flags[pr] ^= patrolLegToA
		w.StartMoveTo(id, w.patrolLeg(pr))
	}
}

func (w *World) patrolLeg(pr int32) fixed.Vec2 {
	if w.Patrol.Flags[pr]&patrolLegToA != 0 {
		return w.Patrol.A[pr]
	}
	return w.Patrol.B[pr]
}

// ---- follow drive (ordersSystem phase 3) ----

func (w *World) driveFollow(r int32, id EntityID) {
	s := w.Orders
	tgt := s.Target[r]
	if tgt == 0 || !w.Ents.Alive(tgt) {
		w.completeOrder(r, id, true) // target gone: stop (WC3)
		return
	}
	mr := w.Movements.Row(id)
	ttr := w.Transforms.Row(tgt)
	if mr == -1 || ttr == -1 {
		return
	}
	tgtPos := w.Transforms.Pos[ttr]
	if s.Phase[r] == orderFresh {
		w.StartMoveTo(id, tgtPos)
		s.Point[r] = tgtPos // remember the last pathed-to point (hysteresis anchor)
		s.Phase[r] = orderRunning
		return
	}
	// re-path only when the target has slipped past the threshold from
	// the point we last pathed to — no per-tick thrash.
	if !fixed.DistSqLess(tgtPos, s.Point[r], w.followRepathDist()) {
		w.StartMoveTo(id, tgtPos)
		s.Point[r] = tgtPos
	}
}

// ---- segment geometry (integer world units; overflow-safe) ----

// segNearest returns the closest point on segment AB to P, in
// world-unit resolution.
func (w *World) segNearest(p, a, b fixed.Vec2) fixed.Vec2 {
	cx, cy := segClosest(p, a, b)
	return fixed.Vec2{X: fixed.FromInt(int32(cx)), Y: fixed.FromInt(int32(cy))}
}

// segDistFarther reports whether P is farther than `leash` from
// segment AB.
func (w *World) segDistFarther(p, a, b fixed.Vec2, leash fixed.F64) bool {
	cx, cy := segClosest(p, a, b)
	closest := fixed.Vec2{X: fixed.FromInt(int32(cx)), Y: fixed.FromInt(int32(cy))}
	return !fixed.DistSqLess(p, closest, leash)
}

// segClosest projects P onto segment AB in integer world units. All
// products fit int64 for coordinates within the world bounds
// (|coord| ≤ ~1e5 → products ≤ ~1e10), so this never overflows the
// way raw 32.32 LenSq would.
func segClosest(p, a, b fixed.Vec2) (int64, int64) {
	ax, ay := a.X.Floor(), a.Y.Floor()
	bx, by := b.X.Floor(), b.Y.Floor()
	px, py := p.X.Floor(), p.Y.Floor()
	abx, aby := bx-ax, by-ay
	denom := abx*abx + aby*aby
	if denom == 0 {
		return ax, ay // degenerate segment: A == B
	}
	num := (px-ax)*abx + (py-ay)*aby
	if num <= 0 {
		return ax, ay
	}
	if num >= denom {
		return bx, by
	}
	return ax + num*abx/denom, ay + num*aby/denom
}

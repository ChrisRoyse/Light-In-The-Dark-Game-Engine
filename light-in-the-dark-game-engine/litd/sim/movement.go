package sim

// Movement system (tick phase 4 — tick-and-scheduler.md §4,
// ecs-architecture.md §6/§7): waypoint following, fixed-point
// position integration, turn-rate-limited facing. Iterates the
// Movement store's dense rows in row order and probes the Transform
// store through rowOf — the canonical join idiom. No dt exists:
// Speed and TurnRate are PER-TICK increments converted from data
// tables at load (R-SIM-1). Arrival tests use DistSqLess (exact
// 128-bit compare, determinism.md §2.4) — no sqrt in comparisons;
// the only square root is the displacement normalization, computed
// on down-shifted integer magnitudes.

import (
	"math/bits"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim/path"
)

// EvMoveDone fires in the events phase when a unit consumes its
// final waypoint — the order-completion signal (#144 consumes it).
const EvMoveDone uint16 = 2

// CellCenter converts a pathing-grid cell index to the world
// coordinates of its center (cell = 32 world units, pathfinding.md
// §2).
func CellCenter(cell int32) fixed.Vec2 {
	x := cell % path.GridSize
	y := cell / path.GridSize
	return fixed.Vec2{X: fixed.FromInt(x*32 + 16), Y: fixed.FromInt(y*32 + 16)}
}

// StartPath puts a unit on a pooled path: waypoint cursor at the
// first cell, state Following. Fails closed on dead entities,
// missing Movement, or an invalid/empty path.
func (w *World) StartPath(id EntityID, pid path.PathID) bool {
	r := w.Movements.Row(id)
	if r == -1 || !w.Ents.Alive(id) || !w.Paths.Valid(pid) {
		return false
	}
	wps := w.Paths.Waypoints(pid)
	if len(wps) == 0 {
		return false
	}
	w.Movements.PathHandle[r] = uint32(pid)
	w.Movements.WaypointIdx[r] = 0
	w.Movements.Target[r] = CellCenter(wps[0])
	w.Movements.State[r] = MoveFollowing
	return true
}

// StartMoveTo points a unit straight at a world position (no path —
// the short-range/diverged case).
func (w *World) StartMoveTo(id EntityID, pt fixed.Vec2) bool {
	r := w.Movements.Row(id)
	if r == -1 || !w.Ents.Alive(id) {
		return false
	}
	w.Movements.PathHandle[r] = NoPath
	w.Movements.WaypointIdx[r] = 0
	w.Movements.Target[r] = pt
	w.Movements.State[r] = MoveFollowing
	return true
}

// mulDivSigned returns num * mul / den with a 128-bit intermediate
// (no overflow, no truncation before the divide). den must be
// positive and >= |mul| when |num| bounds the result — here mul is a
// direction component and den the direction magnitude, so the
// quotient never exceeds |num|.
func mulDivSigned(num fixed.F64, mul, den int64) fixed.F64 {
	neg := false
	n := int64(num)
	if n < 0 {
		neg = !neg
		n = -n
	}
	if mul < 0 {
		neg = !neg
		mul = -mul
	}
	hi, lo := bits.Mul64(uint64(n), uint64(mul))
	q, _ := bits.Div64(hi, lo, uint64(den))
	if neg {
		return fixed.F64(-int64(q))
	}
	return fixed.F64(int64(q))
}

// unitStep returns dir/|dir| × speed: the one-tick displacement.
// Down-shifts the direction so its squared magnitude fits uint64 for
// SqrtU64 (the shift cancels in the ratio); a zero vector (direction
// underflowed the shift) returns the zero step — caller snaps.
func unitStep(dir fixed.Vec2, speed fixed.F64) fixed.Vec2 {
	dx, dy := int64(dir.X), int64(dir.Y)
	adx, ady := dx, dy
	if adx < 0 {
		adx = -adx
	}
	if ady < 0 {
		ady = -ady
	}
	maxc := adx
	if ady > maxc {
		maxc = ady
	}
	shift := uint(0)
	for maxc>>shift >= 1<<31 {
		shift++
	}
	sx, sy := dx>>shift, dy>>shift
	lenSq := uint64(sx*sx) + uint64(sy*sy)
	l := int64(fixed.SqrtU64(lenSq))
	if l == 0 {
		return fixed.Vec2{}
	}
	return fixed.Vec2{
		X: mulDivSigned(speed, sx, l),
		Y: mulDivSigned(speed, sy, l),
	}
}

// movementSystem advances every Following unit by one tick.
func (w *World) movementSystem() {
	m := w.Movements
	for r := int32(0); r < m.count; r++ { // dense rows, fixed order (ecs §6)
		if m.State[r] != MoveFollowing && m.State[r] != MoveFlow {
			continue
		}
		id := m.Entity[r]
		if w.Pauses.Has(id) {
			continue // paused units freeze: no integration this tick (#217)
		}
		if w.moverAuthHeld(id) {
			continue // a MoverAuthority mover owns this transform this tick (#588)
		}
		tr := w.Transforms.Row(id) // ecs §7 join probe
		if tr == -1 {
			m.State[r] = MoveIdle // transform vanished: fail closed
			continue
		}
		if m.State[r] == MoveFlow && !w.prepareFlowTarget(r, tr) {
			continue
		}
		pos := w.Transforms.Pos[tr]
		target := m.Target[r]
		dir := target.Sub(pos)

		// facing first: shortest-arc turn clamped by per-tick rate
		if dir.X != 0 || dir.Y != 0 {
			want := fixed.Atan2(dir.Y, dir.X)
			w.Transforms.Facing[tr] = fixed.TurnToward(w.Transforms.Facing[tr], want, m.TurnRate[r])
			// #376 prop-window gate: if the new facing is still outside the
			// unit's propulsion window, turn in place this tick — no
			// translation. A no-gate (default) window never blocks, so this is
			// a no-op for unauthored units (golden trace stable).
			if w.propWindowBlocks(id, w.Transforms.Facing[tr], want) {
				continue
			}
		}

		speed := w.BuffedMoveSpeed(m.Entity[r], m.Speed[r]) // #162 derived-stat cache
		if speed <= 0 {
			continue // immobile (or fully slowed) unit: stays put
		}

		// arrival: strictly closer than one tick's displacement (or
		// exactly on it) → snap, never overshoot, never oscillate
		var nextPos fixed.Vec2
		arrived := false
		if pos == target || fixed.DistSqLess(pos, target, speed) {
			nextPos, arrived = target, true
		} else {
			step := unitStep(dir, speed)
			if step == (fixed.Vec2{}) {
				nextPos, arrived = target, true // sub-unit remainder
			} else {
				nextPos = pos.Add(step)
			}
		}

		if w.Grid != nil { // §5 local avoidance (avoidance.go)
			w.moveWithAvoidance(r, tr, id, nextPos, arrived)
			continue
		}
		w.Transforms.Pos[tr] = nextPos
		if arrived {
			if m.State[r] == MoveFlow {
				w.advanceFlow(r)
			} else {
				w.advanceWaypoint(r)
			}
		}
	}
}

// advanceWaypoint pops the next waypoint or completes the move:
// final waypoint → state Idle, path released, EvMoveDone emitted
// (the order-completion signal).
func (w *World) advanceWaypoint(r int32) {
	m := w.Movements
	if m.PathHandle[r] != NoPath {
		pid := path.PathID(m.PathHandle[r])
		if w.Paths.Valid(pid) {
			wps := w.Paths.Waypoints(pid)
			next := m.WaypointIdx[r] + 1
			if int(next) < len(wps) {
				m.WaypointIdx[r] = next
				m.Target[r] = CellCenter(wps[next])
				return
			}
			w.Paths.Release(pid)
		}
		m.PathHandle[r] = NoPath
	}
	m.State[r] = MoveIdle
	w.Emit(Event{Kind: EvMoveDone, Src: m.Entity[r]})
}

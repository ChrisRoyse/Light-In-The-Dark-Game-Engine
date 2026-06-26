package sim

// Local avoidance (pathfinding.md §5 — historically the #1 desync
// source in RTS engines, so every rule states its ordering):
//
//   reservation — a unit OWNS exactly one grid cell (OccupiedDynamic
//     + reservedBy); entering a new cell requires acquiring it FIRST.
//   sidestep    — a blocked mover tries the lateral cell closer to
//     its target (octile compare; tie → counterclockwise side).
//   shove       — an idle unit holding a requested cell receives a
//     deterministic move order to the first reservable adjacent cell
//     in compass order. Contention (two shoves into one free cell,
//     mutual-shove cycles) resolves by dense-row processing order —
//     the ecs §6 entity-index rule.
//   stall       — a mover blocked for StallRepathTicks consecutive
//     ticks goes MoveBlocked and emits EvRepathNeeded; the order
//     layer re-paths. The threshold is data, not code.
//
// The subsystem activates when the World holds a pathing grid
// (SetGrid at map load); without one, movement integrates exactly as
// before — the bootstrap seam, not a fallback that hides failure.
//
// This produces WC3's single-file queuing at chokepoints, not RVO
// crowd flow — a deliberate fidelity choice (§5).

import (
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim/path"
)

// EvRepathNeeded fires when a mover exhausts its stall threshold —
// the order layer's cue to re-enqueue a path request.
const EvRepathNeeded uint16 = 3

// DefaultStallRepathTicks is the stall threshold (data; WC3-feel).
const DefaultStallRepathTicks uint16 = 8

// avoidNeighborOrder is the fixed compass order for shove targets
// (N, NE, E, SE, S, SW, W, NW — pathfinding §4 rule 3's order).
var avoidNeighborOrder = [8][2]int32{
	{0, 1}, {1, 1}, {1, 0}, {1, -1}, {0, -1}, {-1, -1}, {-1, 0}, {-1, 1},
}

// SetGrid installs the match's pathing grid and allocates the
// reservation table (map-load time, R-GC-2).
func (w *World) SetGrid(g *path.Grid) {
	w.Grid = g
	if g == nil {
		w.pathDilated = nil
		w.pathHPA = nil
		w.pathQueue = nil
		w.pathFlow = nil
		w.pathProvider = nil
		w.flowRefs = [path.FlowSlots]uint16{}
		return
	}
	if w.reservedBy == nil {
		w.reservedBy = make([]EntityID, path.GridSize*path.GridSize)
	}
	w.bindPathingGrid(g)
}

// StallRepathTicks returns the active stall threshold.
func (w *World) StallRepathTicks() uint16 {
	if w.stallRepath == 0 {
		return DefaultStallRepathTicks
	}
	return w.stallRepath
}

// SetStallRepathTicks overrides the stall threshold (data tables).
func (w *World) SetStallRepathTicks(t uint16) { w.stallRepath = t }

// cellOfPos quantizes a world position to its pathing cell, or -1
// out of bounds (cell = 32 world units).
func cellOfPos(p fixed.Vec2) int32 {
	cx := int32(p.X.Floor() >> 5)
	cy := int32(p.Y.Floor() >> 5)
	if cx < 0 || cy < 0 || cx >= path.GridSize || cy >= path.GridSize {
		return -1
	}
	return cy*path.GridSize + cx
}

// cellReservable reports whether id may take cell c: in bounds,
// statically walkable, and not owned by anyone else. OccupiedDynamic
// is the occupancy truth — EntityID 0 is a VALID unit handle
// (generation 0, index 0), so the reservedBy array alone can never
// mean "free"; it names the owner only while the flag is set.
func (w *World) cellReservable(c int32, id EntityID) bool {
	if c < 0 {
		return false
	}
	x, y := c%path.GridSize, c/path.GridSize
	f := w.Grid.FlagsAt(x, y)
	if f&path.Walkable == 0 || f&path.OccupiedStatic != 0 {
		return false
	}
	return f&path.OccupiedDynamic == 0 || w.reservedBy[c] == id
}

// reserveCell moves id's single-cell ownership to c (releasing any
// previous cell). Caller must have checked cellReservable.
func (w *World) reserveCell(r int32, id EntityID, c int32) {
	if prev := w.Movements.ResCell[r]; prev != -1 && prev != c {
		w.releaseCellIndex(prev, id)
	}
	w.reservedBy[c] = id
	w.Grid.OrFlags(c%path.GridSize, c/path.GridSize, path.OccupiedDynamic)
	w.Movements.ResCell[r] = c
}

func (w *World) releaseCellIndex(c int32, id EntityID) {
	if c < 0 {
		return
	}
	x, y := c%path.GridSize, c/path.GridSize
	if w.Grid.FlagsAt(x, y)&path.OccupiedDynamic != 0 && w.reservedBy[c] == id {
		w.reservedBy[c] = 0 // hygiene only; the flag is the truth
		w.Grid.ClearFlags(x, y, path.OccupiedDynamic)
	}
}

// releaseReservation frees a unit's cell (death/removal hook).
func (w *World) releaseReservation(r int32, id EntityID) {
	if w.Grid == nil {
		return
	}
	if c := w.Movements.ResCell[r]; c != -1 {
		w.releaseCellIndex(c, id)
		w.Movements.ResCell[r] = -1
	}
}

// OccupyCell claims the cell under a unit's current position — the
// spawn/placement hook. Fails closed when the cell is taken.
func (w *World) OccupyCell(id EntityID) bool {
	r := w.Movements.Row(id)
	tr := w.Transforms.Row(id)
	if w.Grid == nil || r == -1 || tr == -1 {
		return false
	}
	c := cellOfPos(w.Transforms.Pos[tr])
	if !w.cellReservable(c, id) {
		return false
	}
	w.reserveCell(r, id, c)
	return true
}

// moveWithAvoidance is the grid-aware phase-4 step for one mover.
// The plain integration (movement.go) computed nextPos; this commits
// it only if the destination cell can be owned, otherwise runs the
// §5 ladder: shove idle occupant → sidestep → stall toward re-path.
func (w *World) moveWithAvoidance(r, tr int32, id EntityID, nextPos fixed.Vec2, arrived bool) {
	m := w.Movements
	curCell := m.ResCell[r]
	nextCell := cellOfPos(nextPos)
	if nextCell == -1 {
		m.Stall[r] = 0
		return // integration would leave the grid: hold position
	}
	if nextCell == curCell || w.cellReservable(nextCell, id) {
		if nextCell != curCell {
			w.reserveCell(r, id, nextCell)
		}
		w.Transforms.Pos[tr] = nextPos
		m.Stall[r] = 0
		if arrived {
			if m.State[r] == MoveFlow {
				w.advanceFlow(r)
			} else {
				w.advanceWaypoint(r)
			}
		}
		return
	}

	// blocked: the §5 ladder, in fixed order
	m.Stall[r]++

	// 1. shove an idle occupant (its escape = first reservable
	//    neighbor in compass order; row order arbitrates contention).
	//    reservedBy is only meaningful under the OccupiedDynamic flag —
	//    a static wall has no occupant to shove.
	nx, ny := nextCell%path.GridSize, nextCell/path.GridSize
	if w.Grid.FlagsAt(nx, ny)&path.OccupiedDynamic != 0 && w.reservedBy[nextCell] != id {
		occupant := w.reservedBy[nextCell]
		or := m.Row(occupant)
		if or != -1 && m.State[or] == MoveIdle {
			ox, oy := nextCell%path.GridSize, nextCell/path.GridSize
			for i := range avoidNeighborOrder {
				d := avoidNeighborOrder[i]
				ex, ey := ox+d[0], oy+d[1]
				if ex < 0 || ey < 0 || ex >= path.GridSize || ey >= path.GridSize {
					continue
				}
				ec := ey*path.GridSize + ex
				if !w.cellReservable(ec, occupant) {
					continue
				}
				w.StartMoveTo(occupant, fixed.Vec2{
					X: fixed.FromInt(ex*32 + 16),
					Y: fixed.FromInt(ey*32 + 16),
				})
				if w.OnShove != nil {
					w.OnShove(w.tick, id, occupant, ec)
				}
				break
			}
		}
	}

	// 2. sidestep: lateral cells relative to the blocked heading;
	//    prefer the one closer to the target (octile), tie → the
	//    counterclockwise side. The sidestep only steers THIS tick.
	pos := w.Transforms.Pos[tr]
	cx, cy := int32(pos.X.Floor()>>5), int32(pos.Y.Floor()>>5)
	hx, hy := nextCell%path.GridSize-cx, nextCell/path.GridSize-cy
	clampDir(&hx, &hy)
	tgt := m.Target[r]
	tcx, tcy := int32(tgt.X.Floor()>>5), int32(tgt.Y.Floor()>>5)
	// lateral = heading rotated ±90°: ccw (-hy, hx), cw (hy, -hx)
	for _, lat := range [2][2]int32{{-hy, hx}, {hy, -hx}} {
		lx, ly := cx+lat[0], cy+lat[1]
		if lx < 0 || ly < 0 || lx >= path.GridSize || ly >= path.GridSize {
			continue
		}
		lc := ly*path.GridSize + lx
		if !w.cellReservable(lc, id) {
			continue
		}
		// "closer to the path": the chosen side must not move away
		// from the target versus the other side — with both free,
		// pick the octile-closer one; enumeration order is the tie
		other := [2]int32{cx + hy, cy - hx}
		if lat == [2]int32{-hy, hx} { // evaluating ccw first
			ox2, oy2 := other[0], other[1]
			if ox2 >= 0 && oy2 >= 0 && ox2 < path.GridSize && oy2 < path.GridSize {
				oc := oy2*path.GridSize + ox2
				if w.cellReservable(oc, id) &&
					path.Octile(ox2, oy2, tcx, tcy) < path.Octile(lx, ly, tcx, tcy) {
					continue // cw side is strictly closer: let it win
				}
			}
		}
		w.reserveCell(r, id, lc)
		step := w.clampedStepToward(r, tr, fixed.Vec2{
			X: fixed.FromInt(lx*32 + 16), Y: fixed.FromInt(ly*32 + 16)})
		w.Transforms.Pos[tr] = step
		return
	}

	// 3. stall threshold → hand back to the order layer
	if m.Stall[r] >= w.StallRepathTicks() {
		if m.State[r] == MoveFlow {
			w.releaseFlowRow(r)
		}
		m.State[r] = MoveBlocked
		m.Stall[r] = 0
		w.Emit(Event{Kind: EvRepathNeeded, Src: id})
	}
}

func clampDir(x, y *int32) {
	if *x > 1 {
		*x = 1
	}
	if *x < -1 {
		*x = -1
	}
	if *y > 1 {
		*y = 1
	}
	if *y < -1 {
		*y = -1
	}
}

// clampedStepToward returns the position after one tick's movement
// from the unit's position toward pt (snapping when within reach).
func (w *World) clampedStepToward(r, tr int32, pt fixed.Vec2) fixed.Vec2 {
	pos := w.Transforms.Pos[tr]
	speed := w.Movements.Speed[r]
	if pos == pt || fixed.DistSqLess(pos, pt, speed) {
		return pt
	}
	return pos.Add(unitStep(pt.Sub(pos), speed))
}

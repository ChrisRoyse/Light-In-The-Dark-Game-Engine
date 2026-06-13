package sim

import (
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim/path"
)

// placeSearchRings bounds the nearest-walkable nudge: 16 cells = 512 world
// units, well past any plausible single blocker patch. Beyond it the unit is
// placed raw (no walkable cell nearby — a degenerate map, not a normal case).
const placeSearchRings = 16

// PlaceUnit relocates id to pos. This is the API SetPosition backend — the D3
// collapse of SetUnitX/SetUnitY/SetUnitPosition/SetUnitPositionLoc.
//
// With skipPathing it places raw (the SetUnitX/SetUnitY teleport capability).
// Otherwise it respects static pathing (the SetUnitPosition collision
// capability, units.md hazard 3): if pos's cell is statically walkable the
// unit keeps the exact pos, else it is nudged to the nearest statically
// walkable cell center. With no grid bound there is no walkability data, so it
// places raw. Returns false on a dead or transform-less unit. Deterministic
// and allocation-free.
func (w *World) PlaceUnit(id EntityID, pos fixed.Vec2, skipPathing bool) bool {
	dest := pos
	if !skipPathing && w.Grid != nil {
		if np, ok := w.nearestWalkable(pos); ok {
			dest = np
		}
	}
	return w.TeleportUnit(id, dest)
}

// nearestWalkable returns the world position to place a unit targeting pos so
// it lands on statically walkable ground. If pos's own cell is walkable it
// returns pos unchanged (exact placement, as WC3 SetUnitPosition keeps the
// coordinates when already pathable). Otherwise it scans outward in
// deterministic square rings up to placeSearchRings and returns the center of
// the first walkable cell found, ok=false if none. Caller must have checked
// w.Grid != nil.
func (w *World) nearestWalkable(pos fixed.Vec2) (fixed.Vec2, bool) {
	if c := cellOfPos(pos); c >= 0 && w.cellStaticWalkable(c) {
		return pos, true
	}
	cx0 := int32(pos.X.Floor() >> 5)
	cy0 := int32(pos.Y.Floor() >> 5)
	for ring := int32(1); ring <= placeSearchRings; ring++ {
		for dy := -ring; dy <= ring; dy++ {
			onYEdge := dy == -ring || dy == ring
			for dx := -ring; dx <= ring; dx++ {
				// only the ring perimeter; interior was scanned by smaller rings
				if !onYEdge && dx != -ring && dx != ring {
					continue
				}
				cx, cy := cx0+dx, cy0+dy
				if cx < 0 || cy < 0 || cx >= path.GridSize || cy >= path.GridSize {
					continue
				}
				cc := cy*path.GridSize + cx
				if w.cellStaticWalkable(cc) {
					return CellCenter(cc), true
				}
			}
		}
	}
	return fixed.Vec2{}, false
}

// cellStaticWalkable reports whether cell c is ground a unit may be placed on:
// statically walkable and not stamped by a building/destructable. Dynamic
// (unit) occupancy is ignored — placement may overlap units transiently, as
// in WC3.
func (w *World) cellStaticWalkable(c int32) bool {
	x, y := c%path.GridSize, c/path.GridSize
	f := w.Grid.FlagsAt(x, y)
	return f&path.Walkable != 0 && f&path.OccupiedStatic == 0
}

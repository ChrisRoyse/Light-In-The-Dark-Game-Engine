package litd

// Spatial unit queries (#239; groups-and-enumeration.md). The JASS
// GroupEnumUnitsInRange / GroupEnumUnitsInRect callback machinery —
// CreateGroup, GroupEnum*, ForGroup, GetEnumUnit, FirstOfGroup,
// DestroyGroup — collapses to slice-returning queries (R-EXEC-4). Results
// are a snapshot in ascending entity-id order (deterministic, and safe to
// mutate the world while iterating). The Append* twins write into a
// caller-pooled buffer for the zero-allocation hot path (R-GC-2).

import "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"

// UnitView is the read-only payload a UnitFilter receives — pointerless,
// carrying only precomputed read data so no mutating verb or wait is
// reachable from a filter (mirrors EventView; execution-model.md §5).
type UnitView struct {
	ownerPlayer int32
	pos         Vec2
}

// OwnerPlayer returns the slot owning the unit (-1 if none).
func (v UnitView) OwnerPlayer() int { return int(v.ownerPlayer) }

// Position returns the unit's world position.
func (v UnitView) Position() Vec2 { return v.pos }

// UnitFilter selects units for a query. A nil filter matches all. It must
// be pure (the read-only UnitView is all it is handed).
type UnitFilter func(UnitView) bool

// viewOf builds the read-only view for a candidate.
func (g *Game) viewOf(id sim.EntityID) UnitView {
	v := UnitView{ownerPlayer: g.ownerOf(id)}
	if r := g.w.Transforms.Row(id); r != -1 {
		p := g.w.Transforms.Pos[r]
		v.pos = Vec2{X: toFloat(p.X), Y: toFloat(p.Y)}
	}
	return v
}

// project filters the sim-id scratch into dst as Unit handles.
func (g *Game) project(dst []Unit, filter UnitFilter) []Unit {
	for _, id := range g.queryScratch {
		if filter == nil || filter(g.viewOf(id)) {
			dst = append(dst, Unit{id: id, g: g})
		}
	}
	return dst
}

// AppendUnitsIn appends the units inside rect that pass filter to dst, in
// ascending entity-id order, and returns the grown slice. Zero-alloc when
// dst (and the game's internal scratch) already have capacity.
func (g *Game) AppendUnitsIn(dst []Unit, rect Rect, filter UnitFilter) []Unit {
	if g == nil || g.w == nil {
		return dst
	}
	g.queryScratch = g.w.AppendUnitsInRect(g.queryScratch[:0],
		fromFloat(rect.MinX), fromFloat(rect.MinY), fromFloat(rect.MaxX), fromFloat(rect.MaxY))
	return g.project(dst, filter)
}

// AppendUnitsInRange appends the units within r of pos that pass filter to
// dst, ascending entity-id order. Zero-alloc with spare capacity.
func (g *Game) AppendUnitsInRange(dst []Unit, pos Vec2, r float64, filter UnitFilter) []Unit {
	if g == nil || g.w == nil {
		return dst
	}
	g.queryScratch = g.w.AppendUnitsInRange(g.queryScratch[:0], vec(pos), fromFloat(r))
	return g.project(dst, filter)
}

// UnitsIn returns the units inside rect passing filter (nil = all), in
// ascending entity-id order. The slice is a snapshot — killing units
// during iteration leaves it unchanged (the handles just go invalid).
// Always non-nil (empty slice on no match). JASS: GroupEnumUnitsInRect.
func (g *Game) UnitsIn(rect Rect, filter UnitFilter) []Unit {
	return g.AppendUnitsIn(make([]Unit, 0), rect, filter)
}

// UnitsInRange returns the units within r of pos passing filter, ascending
// entity-id order, non-nil. JASS: GroupEnumUnitsInRange.
func (g *Game) UnitsInRange(pos Vec2, r float64, filter UnitFilter) []Unit {
	return g.AppendUnitsInRange(make([]Unit, 0), pos, r, filter)
}

// AllUnits returns every unit passing filter, ascending entity-id order,
// non-nil. JASS: GroupEnumUnitsOfPlayer with an all-players walk / the
// CountUnitsBJ family.
func (g *Game) AllUnits(filter UnitFilter) []Unit {
	if g == nil || g.w == nil {
		return make([]Unit, 0)
	}
	g.queryScratch = g.w.AppendAllUnits(g.queryScratch[:0])
	return g.project(make([]Unit, 0), filter)
}

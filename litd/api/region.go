package litd

// Region noun (regions-rects-locations.md; #241). A region is the
// trigger-area capability that a value type can't provide: a
// script-created set of 32-wu grid cells, tested for point or unit
// containment. Scripts NewRegion, add rects/cells, then branch on
// containment — so the cell set is gameplay state, owned and hashed by
// the sim (litd/sim/region.go).
//
// Enter/leave events (a unit crossing into/out of a region firing
// through the event bus) need movement-phase integration and land with
// #371; this file is the containment surface the JASS region natives
// (CreateRegion, RegionAddRect/Cell, IsPointInRegion, IsUnitInRegion,
// GetWorldBounds) collapse onto.

// NewRegion creates an empty region. JASS: CreateRegion. Returns the
// zero-value Region (no-op) on a nil game.
func (g *Game) NewRegion() Region {
	if g == nil || g.w == nil {
		return Region{}
	}
	id, gen := g.w.Regions.NewRegion()
	return Region{id: id, gen: gen, g: g}
}

// WorldBounds returns the playable world rectangle. JASS: GetWorldBounds.
// Zero rect on a nil game.
func (g *Game) WorldBounds() Rect {
	if g == nil || g.w == nil {
		return Rect{}
	}
	minx, miny, maxx, maxy := g.w.WorldBounds()
	return Rect{MinX: toFloat(minx), MinY: toFloat(miny), MaxX: toFloat(maxx), MaxY: toFloat(maxy)}
}

// Remove destroys the region and frees its slot (RemoveRegion). The
// handle goes invalid. Idempotent no-op on an already-removed or
// zero-value region.
func (r Region) Remove() {
	if !r.Valid() {
		return
	}
	r.g.w.Regions.Remove(r.id, r.gen)
}

// AddRect adds every grid cell overlapping rc to the region.
// JASS: RegionAddRect. No-op on an invalid region.
func (r Region) AddRect(rc Rect) {
	if !r.Valid() {
		r.g.reportInvalid("Region.AddRect")
		return
	}
	r.g.w.Regions.AddRect(r.id, r.gen,
		fromFloat(rc.MinX), fromFloat(rc.MinY), fromFloat(rc.MaxX), fromFloat(rc.MaxY))
}

// RemoveRect clears every grid cell overlapping rc from the region.
// JASS: RegionClearRect. No-op on an invalid region.
func (r Region) RemoveRect(rc Rect) {
	if !r.Valid() {
		r.g.reportInvalid("Region.RemoveRect")
		return
	}
	r.g.w.Regions.ClearRect(r.id, r.gen,
		fromFloat(rc.MinX), fromFloat(rc.MinY), fromFloat(rc.MaxX), fromFloat(rc.MaxY))
}

// AddCell adds the single grid cell containing p to the region.
// JASS: RegionAddCell (and the RegionAddCellAtLoc location variant).
// No-op on an invalid region.
func (r Region) AddCell(p Vec2) {
	if !r.Valid() {
		r.g.reportInvalid("Region.AddCell")
		return
	}
	r.g.w.Regions.AddCell(r.id, r.gen, vec(p))
}

// RemoveCell clears the single grid cell containing p from the region.
// JASS: RegionClearCell. No-op on an invalid region.
func (r Region) RemoveCell(p Vec2) {
	if !r.Valid() {
		r.g.reportInvalid("Region.RemoveCell")
		return
	}
	r.g.w.Regions.ClearCell(r.id, r.gen, vec(p))
}

// Contains reports whether p falls in a region cell. JASS:
// IsPointInRegion (and IsLocationInRegion). False on an invalid region.
func (r Region) Contains(p Vec2) bool {
	if !r.Valid() {
		r.g.reportInvalid("Region.Contains")
		return false
	}
	return r.g.w.Regions.ContainsPoint(r.id, r.gen, vec(p))
}

// ContainsUnit reports whether u's position falls in the region. JASS:
// IsUnitInRegion. False on an invalid region or unit.
func (r Region) ContainsUnit(u Unit) bool {
	if !r.Valid() {
		r.g.reportInvalid("Region.ContainsUnit")
		return false
	}
	if u.g != r.g || !u.Valid() {
		return false
	}
	return r.g.w.RegionContainsUnit(r.id, r.gen, u.id)
}

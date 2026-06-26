package sim

// Building placement validation + construction (#301, combat-and-orders
// §2, pathfinding §2.3). A worker issued OrderBuild walks to the site;
// on arrival construction STARTS — cost is deducted, the footprint is
// stamped OccupiedStatic on the pathing grid, and the building entity
// spawns at progress 0 with a low starting HP that ramps to the table
// max as integer progress ticks accumulate. Cancel refunds a per-mille
// of cost, unstamps the grid, and destroys the unfinished building.
// Destruction mid-construction releases the worker and unstamps the
// grid but refunds nothing.
//
// Placement validation, cost admission, and the start stamp all fail
// CLOSED and deterministically: an occupied/unpathable site or
// insufficient resources refuses with no side effect (no cost, no
// order, no stamp), and a second worker racing the same site loses to
// the first by dense-row order.

import (
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim/path"
)

// OrderBuild is appended after OrderFollow=9 (#306). Append-only.
const OrderBuild uint8 = 10

// Construction events.
const (
	// EvConstructStarted: Src = worker, Dst = building entity.
	EvConstructStarted uint16 = 19
	// EvConstructFinished: Src = building, Dst = building.
	EvConstructFinished uint16 = 20
	// EvConstructCancelled: Src = worker (0 if released), Dst = building.
	EvConstructCancelled uint16 = 21
)

// BuildStartPermille is the fraction of max HP a structure has at
// progress 0 — it ramps linearly to 1000 (full) at completion. WC3
// foundations start around a tenth of finished HP.
const BuildStartPermille = 100

// buildDef resolves the immutable building row, or nil if typeID is not
// a constructable building.
func (w *World) buildDef(typeID uint16) *data.Unit {
	if w.unitDefs == nil || int(typeID) >= len(w.unitDefs) {
		return nil
	}
	d := &w.unitDefs[typeID]
	if d.BuildTicks == 0 || d.Footprint == 0 {
		return nil // not a constructable building
	}
	return d
}

// footprintCells centres a typeID's square footprint on a world point,
// returning the origin cell (fx,fy) and side w in grid cells.
func footprintCells(site fixed.Vec2, side int32) (fx, fy, fw int32) {
	cx := int32(site.X.Floor() >> 5) // cell = 32 world units
	cy := int32(site.Y.Floor() >> 5)
	return cx - side/2, cy - side/2, side
}

// cellsBuildable reports whether every cell of [fx,fy,fw²] is in bounds,
// flagged Buildable, and free of any static/dynamic occupancy. No grid
// → nothing is buildable (fail closed: a sim that never loaded a map
// cannot place structures).
func (w *World) cellsBuildable(fx, fy, fw int32) bool {
	if w.Grid == nil || fw <= 0 {
		return false
	}
	for y := fy; y < fy+fw; y++ {
		for x := fx; x < fx+fw; x++ {
			if !path.InBounds(x, y) {
				return false
			}
			f := w.Grid.FlagsAt(x, y)
			if f&path.Buildable == 0 {
				return false
			}
			if f&(path.OccupiedStatic|path.OccupiedDynamic) != 0 {
				return false
			}
		}
	}
	return true
}

// ValidatePlacement reports whether a building of typeID may be placed
// centred on site right now (footprint in bounds, buildable, free).
func (w *World) ValidatePlacement(typeID uint16, site fixed.Vec2) bool {
	def := w.buildDef(typeID)
	if def == nil {
		return false
	}
	fx, fy, fw := footprintCells(site, int32(def.Footprint))
	return w.cellsBuildable(fx, fy, fw)
}

// IssueBuild orders a worker to construct typeID at site. Placement is
// validated at issue: an invalid site refuses immediately (false) with
// no order and no cost. Cost is deducted only when construction starts
// (on arrival), per the spec.
func (w *World) IssueBuild(worker EntityID, typeID uint16, site fixed.Vec2) bool {
	if w.Orders.Row(worker) == -1 || !w.Ents.Alive(worker) {
		return false
	}
	if w.buildDef(typeID) == nil {
		return false
	}
	if w.Movements.Row(worker) == -1 {
		return false // a builder must be able to walk to the site
	}
	if !w.ValidatePlacement(typeID, site) {
		return false // occupied/unpathable/off-map: deterministic refusal
	}
	return w.IssueOrder(worker, Order{Kind: OrderBuild, Point: site, Data: typeID}, false)
}

// buildSearchRings bounds the deterministic outward placement scan.
const buildSearchRings = 24

// PlaceBuildingNear runs a deterministic outward ring scan from center for a
// buildable site for typeID and, finding one, picks the lowest-entity-id idle
// builder owned by player and issues the build. Returns the builder, the chosen
// site, and ok. ok=false — a recorded no-op the AI observes — when no buildable
// site is found within the scan or no idle builder is available. The sim owns
// site + builder selection (R-EXEC-3): the AI names a structure type only, never
// raw coordinates.
func (w *World) PlaceBuildingNear(player uint8, typeID uint16, center fixed.Vec2) (EntityID, fixed.Vec2, bool) {
	if w.buildDef(typeID) == nil {
		return 0, fixed.Vec2{}, false
	}
	site, ok := w.findBuildSite(typeID, center)
	if !ok {
		return 0, fixed.Vec2{}, false
	}
	worker, ok := w.idleBuilder(player)
	if !ok {
		return 0, fixed.Vec2{}, false
	}
	if !w.IssueBuild(worker, typeID, site) {
		return 0, fixed.Vec2{}, false
	}
	return worker, site, true
}

// findBuildSite scans cells in expanding square rings around center's cell, in a
// fixed (row-major per ring) order, and returns the first cell-centred site that
// passes ValidatePlacement. Deterministic and grid-driven.
func (w *World) findBuildSite(typeID uint16, center fixed.Vec2) (fixed.Vec2, bool) {
	if w.Grid == nil {
		return fixed.Vec2{}, false
	}
	cx := int32(center.X.Floor() >> 5) // cell = 32 world units
	cy := int32(center.Y.Floor() >> 5)
	for ring := int32(0); ring <= buildSearchRings; ring++ {
		for dy := -ring; dy <= ring; dy++ {
			for dx := -ring; dx <= ring; dx++ {
				if ring > 0 && dx > -ring && dx < ring && dy > -ring && dy < ring {
					continue // interior cells already covered by a smaller ring
				}
				x, y := cx+dx, cy+dy
				if !path.InBounds(x, y) {
					continue
				}
				site := CellCenter(y*path.GridSize + x)
				if w.ValidatePlacement(typeID, site) {
					return site, true
				}
			}
		}
	}
	return fixed.Vec2{}, false
}

// idleBuilder returns the lowest-entity-id mobile, idle (Stop-stance) unit owned
// by player that is not itself a producer structure — a builder candidate.
func (w *World) idleBuilder(player uint8) (EntityID, bool) {
	o := w.Owners
	var pick EntityID
	pickIdx := int64(1) << 62
	found := false
	for r := int32(0); r < o.count; r++ {
		if o.Player[r] != player {
			continue
		}
		id := o.Entity[r]
		or := w.Orders.Row(id)
		if or == -1 || w.Movements.Row(id) == -1 || w.Produce.Row(id) != -1 {
			continue
		}
		if w.Orders.Kind[or] != OrderStop {
			continue // busy (harvesting / building / moving)
		}
		if idx := int64(id.Index()); idx < pickIdx {
			pick, pickIdx, found = id, idx, true
		}
	}
	return pick, found
}

// buildReach is the worker-to-site distance at which construction may
// start: the footprint half-extent plus a worker arm's length.
func buildReach(side int32) fixed.F64 {
	return fixed.FromInt(side*16 + 96) // cell=32 → half-extent = side*16
}

// driveBuild (ordersSystem phase 3): walk to the site, then start.
func (w *World) driveBuild(r int32, id EntityID) {
	s := w.Orders
	typeID := s.Data[r]
	site := s.Point[r]
	def := w.buildDef(typeID)
	if def == nil {
		w.completeOrder(r, id, false)
		return
	}
	mr := w.Movements.Row(id)
	tr := w.Transforms.Row(id)
	if mr == -1 || tr == -1 {
		w.completeOrder(r, id, false)
		return
	}
	if fixed.DistSqLess(w.Transforms.Pos[tr], site, buildReach(int32(def.Footprint))) {
		w.completeOrder(r, id, w.startConstruction(id, typeID, site))
		return
	}
	if s.Phase[r] == orderFresh {
		if !w.StartMoveTo(id, site) {
			w.completeOrder(r, id, false)
			return
		}
		s.Phase[r] = orderRunning
		return
	}
	switch w.Movements.State[mr] {
	case MoveIdle, MoveBlocked: // arrived but still short of reach: unreachable
		w.completeOrder(r, id, false)
	}
}

// startConstruction deducts cost, spawns the building at progress 0,
// stamps the footprint, and links the builder. Returns false (with NO
// side effect) on a now-occupied site, insufficient resources, or pool
// exhaustion — fail closed, never a partial build.
func (w *World) startConstruction(worker EntityID, typeID uint16, site fixed.Vec2) bool {
	def := w.buildDef(typeID)
	if def == nil {
		return false
	}
	or := w.Owners.Row(worker)
	if or == -1 {
		return false
	}
	p, team := w.Owners.Player[or], w.Owners.Team[or]
	fx, fy, fw := footprintCells(site, int32(def.Footprint))
	if !w.cellsBuildable(fx, fy, fw) {
		return false // a racing builder already stamped this site
	}
	if int(p) >= MaxPlayers || !w.canAffordBuild(p, def) {
		return false // insufficient resources: no deduction, no build
	}
	b, ok := w.SpawnFromTable(typeID, p, team, site)
	if !ok {
		return false // pool exhausted: nothing spent
	}
	br := w.Build.add(w.Ents, b, fx, fy, fw)
	if br == -1 {
		w.DestroyUnit(b)
		return false
	}
	// committed: spend, stamp, link, set the foundation HP.
	w.spendBuild(p, def)
	w.stampStatic(path.Rect{X: fx, Y: fy, W: fw, H: fw})
	w.Build.Builder[br] = worker
	w.setConstructHP(b, def, 0)
	w.Emit(Event{Kind: EvConstructStarted, Src: worker, Dst: b})
	return true
}

// canAffordBuild reports whether player p can pay def's resource cost.
func (w *World) canAffordBuild(p uint8, def *data.Unit) bool {
	for i := 0; i < len(def.Costs) && i < w.resourceCount; i++ {
		if w.resources[p][i] < def.Costs[i] {
			return false
		}
	}
	return true
}

func (w *World) spendBuild(p uint8, def *data.Unit) {
	for i := 0; i < len(def.Costs) && i < w.resourceCount; i++ {
		w.resources[p][i] -= def.Costs[i]
	}
}

// setConstructHP sets a building's max/current HP to the ramp value at
// `progress` ticks (overrides SpawnFromTable's full-HP default).
func (w *World) setConstructHP(b EntityID, def *data.Unit, progress uint16) {
	hr := w.Healths.Row(b)
	if hr == -1 {
		return
	}
	m := w.constructMax(def, progress)
	w.Healths.MaxLife[hr] = m
	w.Healths.Life[hr] = m
}

// constructMax is the ramped max HP at `progress` ticks: a linear lerp
// from BuildStartPermille to 1000 over def.BuildTicks.
func (w *World) constructMax(def *data.Unit, progress uint16) fixed.F64 {
	final := fixed.FromInt(def.Life)
	if def.BuildTicks == 0 {
		return final
	}
	if progress >= def.BuildTicks {
		return final
	}
	pm := BuildStartPermille + (1000-BuildStartPermille)*int(progress)/int(def.BuildTicks)
	return scalePermille(final, uint16(pm))
}

// constructionSystem advances every rising structure one tick: ramp the
// max HP (current grows by the same increment so combat damage during
// construction persists), and on the last tick finalize HP, release the
// builder, and emit EvConstructFinished. Dense-row order; zero alloc.
func (w *World) constructionSystem() {
	bs := w.Build
	for r := int32(0); r < bs.count; r++ {
		b := bs.Entity[r]
		if !w.Ents.Alive(b) {
			continue
		}
		ut := w.UnitTypes.Row(b)
		if ut == -1 {
			continue
		}
		def := &w.unitDefs[w.UnitTypes.TypeID[ut]]
		if def.BuildTicks == 0 || bs.Progress[r] >= def.BuildTicks {
			continue // finished structures are inert here
		}
		hr := w.Healths.Row(b)
		oldMax := fixed.F64(0)
		if hr != -1 {
			oldMax = w.Healths.MaxLife[hr]
		}
		bs.Progress[r]++
		newMax := w.constructMax(def, bs.Progress[r])
		if hr != -1 {
			delta := newMax.Sub(oldMax)
			w.Healths.MaxLife[hr] = newMax
			w.Healths.Life[hr] = w.Healths.Life[hr].Add(delta)
			if w.Healths.Life[hr] > newMax {
				w.Healths.Life[hr] = newMax
			}
		}
		if bs.Progress[r] >= def.BuildTicks {
			bs.Builder[r] = 0 // worker released on completion
			w.Emit(Event{Kind: EvConstructFinished, Src: b, Dst: b})
		}
	}
}

// CancelConstruction refunds a per-mille of the building's cost,
// unstamps the footprint, and destroys the unfinished structure. A
// finished building is not cancellable (false). The grid unstamp and
// row removal run in DestroyUnit at cleanup.
func (w *World) CancelConstruction(b EntityID) bool {
	br := w.Build.Row(b)
	if br == -1 {
		return false
	}
	ut := w.UnitTypes.Row(b)
	if ut == -1 {
		return false
	}
	def := &w.unitDefs[w.UnitTypes.TypeID[ut]]
	if def.BuildTicks == 0 || w.Build.Progress[br] >= def.BuildTicks {
		return false // already complete: nothing to cancel
	}
	// Already cancelled or dying THIS tick? The kill is deferred to phase 7, so the
	// Build row still resolves the rest of the tick — reject a second cancel rather
	// than refund twice (a building dying from combat is likewise non-refundable).
	if !w.Ents.Alive(b) || w.markedForDeath(b) {
		return false
	}
	if or := w.Owners.Row(b); or != -1 {
		if p := w.Owners.Player[or]; p < MaxPlayers {
			for i := 0; i < len(def.Costs) && i < w.resourceCount; i++ {
				refund := def.Costs[i] * int64(def.RefundPermille) / 1000
				w.resources[p][i] += refund
			}
		}
	}
	worker := w.Build.Builder[br]
	w.Emit(Event{Kind: EvConstructCancelled, Src: worker, Dst: b})
	w.KillUnit(b) // deferred: DestroyUnit unstamps the grid + drops the row
	return true
}

// IsUnderConstruction reports whether b is a rising (unfinished)
// structure. A valid attack target (WC3), and the cancel-eligible set.
func (w *World) IsUnderConstruction(b EntityID) bool {
	br := w.Build.Row(b)
	if br == -1 {
		return false
	}
	ut := w.UnitTypes.Row(b)
	if ut == -1 {
		return false
	}
	return w.Build.Progress[br] < w.unitDefs[w.UnitTypes.TypeID[ut]].BuildTicks
}

// destroyBuild is called from DestroyUnit: a dying STRUCTURE unstamps
// its footprint; a dying WORKER that was a builder releases its link
// (construction continues — WC3 keeps a building rising if its builder
// is lost). No refund — destruction is not a cancel.
func (w *World) destroyBuild(id EntityID) {
	if br := w.Build.Row(id); br != -1 { // a structure
		w.clearStatic(path.Rect{X: w.Build.FX[br], Y: w.Build.FY[br], W: w.Build.FW[br], H: w.Build.FW[br]})
		w.Build.Remove(id)
		return
	}
	if br := w.Build.builderRow(id); br != -1 { // a builder worker died
		w.Build.Builder[br] = 0
	}
}

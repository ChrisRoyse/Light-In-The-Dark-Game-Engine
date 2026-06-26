package sim

import (
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim/path"
)

func (w *World) bindPathingGrid(g *path.Grid) {
	key := path.LayerKey{Required: path.Walkable, Blocked: path.OccupiedStatic}
	w.pathDilated = path.NewDilatedSet(g, []path.LayerKey{key})
	w.pathHPA = path.NewHPA(g, w.pathDilated.Layer(0), path.NewSearcher(g))
	w.pathQueue = path.NewQueue(w.pathHPA, w.Paths)
	w.pathQueue.OnResult = w.onPathResult
	w.pathFlow = path.NewFlowSet(g, w.pathDilated.Layer(0))
	w.pathProvider = path.NewProvider(path.NewSharer([]*path.Queue{w.pathQueue}), w.pathFlow)
	w.flowRefs = [path.FlowSlots]uint16{}
}

func (w *World) pathingSystem() {
	w.pathLastExp = 0
	if w.pathQueue != nil {
		w.pathLastExp = w.pathQueue.Service(path.DefaultExpansionBudget)
	}
	if w.pathFlow != nil {
		w.pathFlow.Pump(path.DefaultIntegrationBudget)
	}
}

// PathExpansionsLastTick reports the counted path-search expansions
// consumed by the most recent pathing phase.
func (w *World) PathExpansionsLastTick() int32 { return w.pathLastExp }

// PathQueueDepth reports in-flight path requests, including a parked
// head request. Worlds without a bound pathing grid report zero.
func (w *World) PathQueueDepth() int {
	if w.pathQueue == nil {
		return 0
	}
	return w.pathQueue.InFlight()
}

func (w *World) startMoveOrder(or int32, id EntityID) (uint8, bool) {
	if w.Grid == nil || w.pathQueue == nil {
		if !w.StartMoveTo(id, w.Orders.Point[or]) {
			return orderFresh, false
		}
		return orderRunning, true
	}
	if !w.enqueueMovePath(or, id) {
		return orderFresh, false
	}
	return orderPathing, true
}

func (w *World) enqueueMovePath(or int32, id EntityID) bool {
	tr := w.Transforms.Row(id)
	if tr == -1 || w.pathQueue == nil {
		return false
	}
	start := cellOfPos(w.Transforms.Pos[tr])
	goal := cellOfPos(w.Orders.Point[or])
	if start < 0 || goal < 0 {
		return false
	}
	seq := w.pathSeq
	w.pathSeq++
	return w.pathQueue.Enqueue(path.Request{
		Owner: uint32(id),
		SX:    start % path.GridSize,
		SY:    start / path.GridSize,
		TX:    goal % path.GridSize,
		TY:    goal / path.GridSize,
		Tick:  w.tick,
		Seq:   seq,
	})
}

func (w *World) onPathResult(ev path.ServiceEvent) {
	if !ev.Done {
		return
	}
	id := EntityID(ev.Owner)
	or := w.Orders.Row(id)
	if or == -1 || w.Orders.Kind[or] != OrderMove || w.Orders.Phase[or] != orderPathing {
		if w.Paths.Valid(ev.Path) {
			w.Paths.Release(ev.Path)
		}
		return
	}
	switch ev.Status {
	case path.StatusCompleted:
		if !w.Paths.Valid(ev.Path) {
			w.completeOrder(or, id, true)
			return
		}
		if !w.StartPath(id, ev.Path) {
			if w.Paths.Valid(ev.Path) {
				w.Paths.Release(ev.Path)
			}
			w.completeOrder(or, id, false)
			return
		}
		w.Orders.Phase[or] = orderRunning
	case path.StatusPartial:
		if !w.Paths.Valid(ev.Path) || !w.StartPath(id, ev.Path) {
			if w.Paths.Valid(ev.Path) {
				w.Paths.Release(ev.Path)
			}
			w.completeOrder(or, id, false)
			return
		}
		w.Orders.Phase[or] = orderRunning
	default:
		w.completeOrder(or, id, false)
	}
}

func (w *World) issueFlowMoveGroup(goalPoint fixed.Vec2, actors []EntityID) bool {
	if w.pathFlow == nil {
		return false
	}
	goalCell := cellOfPos(goalPoint)
	if goalCell < 0 {
		return false
	}
	slot, _, ok := w.pathFlow.AcquireAsync(goalCell%path.GridSize, goalCell/path.GridSize)
	if !ok {
		return false
	}
	started := 0
	for _, id := range actors {
		or := w.Orders.Row(id)
		mr := w.Movements.Row(id)
		if or == -1 || mr == -1 {
			continue
		}
		w.issueOrderRow(or, id, Order{Kind: OrderMove, Point: goalPoint}, false)
		if !w.startFlow(id, slot, goalCell) {
			w.completeOrder(or, id, false)
			continue
		}
		w.Orders.Phase[or] = orderRunning
		started++
	}
	if started == 0 {
		w.pathFlow.Release(slot)
		return false
	}
	return true
}

func (w *World) startFlow(id EntityID, slot int, goalCell int32) bool {
	r := w.Movements.Row(id)
	if r == -1 || !w.Ents.Alive(id) || slot < 0 || slot >= path.FlowSlots {
		return false
	}
	w.releaseMoveHandle(r)
	w.flowRefs[slot]++
	w.Movements.PathHandle[r] = uint32(slot)
	w.Movements.WaypointIdx[r] = goalCell
	w.Movements.Target[r] = CellCenter(goalCell)
	w.Movements.State[r] = MoveFlow
	return true
}

func (w *World) prepareFlowTarget(r, tr int32) bool {
	if w.pathFlow == nil {
		w.releaseFlowRow(r)
		w.Movements.State[r] = MoveIdle
		return false
	}
	slot := int(w.Movements.PathHandle[r])
	if slot < 0 || slot >= path.FlowSlots {
		w.blockFlowRow(r)
		return false
	}
	if !w.pathFlow.Ready(slot) {
		return false
	}
	pos := w.Transforms.Pos[tr]
	cell := cellOfPos(pos)
	goal := w.Movements.WaypointIdx[r]
	if cell < 0 {
		w.blockFlowRow(r)
		return false
	}
	if cell == goal {
		w.advanceFlow(r)
		return false
	}
	x, y := cell%path.GridSize, cell/path.GridSize
	dx, dy := path.Step(w.pathFlow.Dir(slot, x, y))
	if dx == 0 && dy == 0 {
		w.blockFlowRow(r)
		return false
	}
	nextX, nextY := x+dx, y+dy
	if !path.InBounds(nextX, nextY) {
		w.blockFlowRow(r)
		return false
	}
	w.Movements.Target[r] = CellCenter(nextY*path.GridSize + nextX)
	return true
}

func (w *World) advanceFlow(r int32) {
	id := w.Movements.Entity[r]
	tr := w.Transforms.Row(id)
	if tr == -1 || cellOfPos(w.Transforms.Pos[tr]) != w.Movements.WaypointIdx[r] {
		return
	}
	w.releaseFlowRow(r)
	w.Movements.State[r] = MoveIdle
	w.Emit(Event{Kind: EvMoveDone, Src: id})
}

func (w *World) blockFlowRow(r int32) {
	id := w.Movements.Entity[r]
	w.releaseFlowRow(r)
	w.Movements.State[r] = MoveBlocked
	w.Emit(Event{Kind: EvRepathNeeded, Src: id})
}

func (w *World) releaseMoveHandle(r int32) {
	if w.Movements.State[r] == MoveFlow {
		w.releaseFlowRow(r)
		return
	}
	if w.Movements.PathHandle[r] != NoPath {
		pid := path.PathID(w.Movements.PathHandle[r])
		if w.Paths.Valid(pid) {
			w.Paths.Release(pid)
		}
		w.Movements.PathHandle[r] = NoPath
	}
}

func (w *World) releaseFlowRow(r int32) {
	if w.Movements.State[r] != MoveFlow {
		return
	}
	slot := int(w.Movements.PathHandle[r])
	if slot >= 0 && slot < path.FlowSlots && w.flowRefs[slot] > 0 {
		w.flowRefs[slot]--
		if w.flowRefs[slot] == 0 && w.pathFlow != nil {
			w.pathFlow.Release(slot)
		}
	}
	w.Movements.PathHandle[r] = NoPath
	w.Movements.WaypointIdx[r] = 0
}

func (w *World) stampStatic(r path.Rect) {
	if w.Grid == nil {
		return
	}
	if w.pathDilated == nil || w.pathHPA == nil {
		w.Grid.StampStatic(r)
		return
	}
	w.pathDilated.StampStatic(r)
	w.pathHPA.RebuildRect(r)
	w.Paths.InvalidateRect(r, w.onPathInvalidated)
	if w.pathFlow != nil {
		w.pathFlow.InvalidateAllAsync()
	}
}

func (w *World) clearStatic(r path.Rect) {
	if w.Grid == nil {
		return
	}
	if w.pathDilated == nil || w.pathHPA == nil {
		w.Grid.ClearStatic(r)
		return
	}
	w.pathDilated.ClearStatic(r)
	w.pathHPA.RebuildRect(r)
	w.Paths.InvalidateRect(r, w.onPathInvalidated)
	if w.pathFlow != nil {
		w.pathFlow.InvalidateAllAsync()
	}
}

func (w *World) onPathInvalidated(pid path.PathID) {
	for mr := int32(0); mr < w.Movements.count; mr++ {
		if w.Movements.State[mr] != MoveFollowing || w.Movements.PathHandle[mr] != uint32(pid) {
			continue
		}
		id := w.Movements.Entity[mr]
		w.Movements.PathHandle[mr] = NoPath
		or := w.Orders.Row(id)
		if or == -1 || w.Orders.Kind[or] != OrderMove {
			w.Movements.State[mr] = MoveIdle
			continue
		}
		if w.enqueueMovePath(or, id) {
			w.Orders.Phase[or] = orderPathing
			w.Movements.State[mr] = MoveIdle
		} else {
			w.Movements.State[mr] = MoveBlocked
		}
	}
}

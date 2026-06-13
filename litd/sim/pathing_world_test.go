package sim

import (
	"fmt"
	"strings"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim/path"
)

func TestWorldPathingMoveOrderQueuesAndStartsPathFSV(t *testing.T) {
	w, id := pathingOrderWorld(t)
	target := CellCenter(cellIdx(26, 10))
	before := worldPathingDump(w, id)
	if !w.IssueOrder(id, Order{Kind: OrderMove, Point: target}, false) {
		t.Fatal("IssueOrder refused move")
	}
	afterIssue := worldPathingDump(w, id)
	w.Step()
	afterStep := worldPathingDump(w, id)
	t.Logf("FSV path move BEFORE: %s", before)
	t.Logf("FSV path move AFTER issue: %s", afterIssue)
	t.Logf("FSV path move AFTER step:  %s", afterStep)

	mr := w.Movements.Row(id)
	if w.Movements.State[mr] != MoveFollowing {
		t.Fatalf("movement state = %d, want MoveFollowing; %s", w.Movements.State[mr], afterStep)
	}
	pid := path.PathID(w.Movements.PathHandle[mr])
	if !w.Paths.Valid(pid) {
		t.Fatalf("movement does not hold a valid path handle: %s", afterStep)
	}
	wps := w.Paths.Waypoints(pid)
	if len(wps) == 0 {
		t.Fatalf("delivered path has no waypoints: %s", afterStep)
	}
	if w.pathQueue.InFlight() != 0 {
		t.Fatalf("path queue still in flight after delivery: %s", afterStep)
	}
}

func TestWorldPathingInvalidTargetFailsClosedFSV(t *testing.T) {
	w, id := pathingOrderWorld(t)
	target := fixed.Vec2{X: fixed.FromInt(path.GridSize*32 + 10), Y: fixed.FromInt(32)}
	before := worldPathingDump(w, id)
	if !w.IssueOrder(id, Order{Kind: OrderMove, Point: target}, false) {
		t.Fatal("IssueOrder refused before order system could validate target")
	}
	afterIssue := worldPathingDump(w, id)
	w.Step()
	afterStep := worldPathingDump(w, id)
	t.Logf("FSV path invalid target BEFORE: %s", before)
	t.Logf("FSV path invalid target AFTER issue: %s", afterIssue)
	t.Logf("FSV path invalid target AFTER step:  %s", afterStep)

	or := w.Orders.Row(id)
	mr := w.Movements.Row(id)
	if w.Orders.Kind[or] != OrderStop || w.Movements.State[mr] != MoveIdle || w.Paths.Live() != 0 {
		t.Fatalf("invalid target did not fail closed to stop/idle/no path: %s", afterStep)
	}
	if w.pathQueue.InFlight() != 0 || w.pathQueue.Dropped() != 0 {
		t.Fatalf("invalid target mutated queue counters: %s", afterStep)
	}
}

func TestWorldFlowBackendForSharedGoalCommandFSV(t *testing.T) {
	w := NewWorld(Caps{Units: path.DefaultFlowThreshold + 4})
	w.SetGrid(openGrid())
	rec := CommandRecord{
		Version: CommandVersion,
		Player:  0,
		Opcode:  OpMove,
		Point:   CellCenter(cellIdx(80, 80)),
	}
	rec.UnitCount = uint8(path.DefaultFlowThreshold)
	for i := 0; i < path.DefaultFlowThreshold; i++ {
		id := addOwnedPathingMover(t, w, int32(10+i%10), int32(10+i/10), 8*fixed.One, 0)
		rec.Units[i] = id
	}
	before := worldPathingDump(w, rec.Units[0])
	w.applyCommandRecord(&rec)
	afterApply := worldPathingDump(w, rec.Units[0])
	fmr := w.Movements.Row(rec.Units[0])
	if fmr == -1 {
		t.Fatalf("flow command setup failed without movement row: %s", afterApply)
	}
	slot := int(w.Movements.PathHandle[fmr])
	if slot < 0 || slot >= path.FlowSlots {
		t.Fatalf("flow command setup failed with invalid slot %d: %s", slot, afterApply)
	}
	for ticks := 0; ticks < 80 && !w.pathFlow.Ready(slot); ticks++ {
		w.pathingSystem()
	}
	afterPump := worldPathingDump(w, rec.Units[0])
	w.Step()
	afterStep := worldPathingDump(w, rec.Units[0])
	t.Logf("FSV flow command BEFORE: %s", before)
	t.Logf("FSV flow command AFTER apply: slot=%d refs=%d %s", slot, w.flowRefs[slot], afterApply)
	t.Logf("FSV flow command AFTER pump:  ready=%v dir@start=%d %s",
		w.pathFlow.Ready(slot), w.pathFlow.Dir(slot, 10, 10), afterPump)
	t.Logf("FSV flow command AFTER step:  %s", afterStep)

	if w.pathProvider.SelectBackend(path.DefaultFlowThreshold) != path.BackendFlow {
		t.Fatal("provider did not select flow backend at threshold")
	}
	if slot < 0 || slot >= path.FlowSlots || w.flowRefs[slot] != uint16(path.DefaultFlowThreshold) {
		t.Fatalf("flow refs for slot %d = %d, want %d; %s", slot, w.flowRefs[slot], path.DefaultFlowThreshold, afterApply)
	}
	for i := 0; i < int(rec.UnitCount); i++ {
		mr := w.Movements.Row(rec.Units[i])
		if w.Movements.State[mr] != MoveFlow || int(w.Movements.PathHandle[mr]) != slot {
			t.Fatalf("unit %d not on shared flow slot: %s", i, worldPathingDump(w, rec.Units[i]))
		}
	}
	if !w.pathFlow.Ready(slot) || w.pathFlow.Dir(slot, 10, 10) == path.DirNone {
		t.Fatalf("flow field did not become readable: %s", afterPump)
	}
}

func TestWorldFlowUnreachableCellFailsBlockedFSV(t *testing.T) {
	w := NewWorld(Caps{Units: path.DefaultFlowThreshold})
	g := openGrid()
	for x := int32(0); x < path.GridSize; x++ {
		g.ClearFlags(x, 20, path.Walkable)
	}
	w.SetGrid(g)
	rec := CommandRecord{
		Version: CommandVersion,
		Player:  0,
		Opcode:  OpMove,
		Point:   CellCenter(cellIdx(80, 80)),
	}
	rec.UnitCount = uint8(path.DefaultFlowThreshold)
	for i := 0; i < path.DefaultFlowThreshold; i++ {
		id := addOwnedPathingMover(t, w, int32(10+i%10), int32(10+i/10), 8*fixed.One, 0)
		rec.Units[i] = id
	}
	before := worldPathingDump(w, rec.Units[0])
	w.applyCommandRecord(&rec)
	afterApply := worldPathingDump(w, rec.Units[0])
	fmr := w.Movements.Row(rec.Units[0])
	if fmr == -1 {
		t.Fatalf("flow unreachable setup failed without movement row: %s", afterApply)
	}
	slot := int(w.Movements.PathHandle[fmr])
	if slot < 0 || slot >= path.FlowSlots {
		t.Fatalf("flow unreachable setup failed with invalid slot %d: %s", slot, afterApply)
	}
	for ticks := 0; ticks < 80 && !w.pathFlow.Ready(slot); ticks++ {
		w.pathingSystem()
	}
	afterPump := worldPathingDump(w, rec.Units[0])
	readyAfterPump := w.pathFlow.Ready(slot)
	dirAfterPump := w.pathFlow.Dir(slot, 10, 10)
	w.Step()
	afterBlocked := worldPathingDump(w, rec.Units[0])
	mrAfterBlocked := w.Movements.Row(rec.Units[0])
	stateAfterBlocked := w.Movements.State[mrAfterBlocked]
	refsAfterBlocked := w.flowRefs[slot]
	w.Step()
	afterOrder := worldPathingDump(w, rec.Units[0])
	t.Logf("FSV flow unreachable BEFORE: %s", before)
	t.Logf("FSV flow unreachable AFTER apply: slot=%d %s", slot, afterApply)
	t.Logf("FSV flow unreachable AFTER pump:  ready=%v dir@start=%d %s",
		readyAfterPump, dirAfterPump, afterPump)
	t.Logf("FSV flow unreachable AFTER block: refs=%d state=%d %s", refsAfterBlocked, stateAfterBlocked, afterBlocked)
	t.Logf("FSV flow unreachable AFTER order: %s", afterOrder)

	if !readyAfterPump {
		t.Fatalf("flow unreachable setup failed: slot=%d %s", slot, afterPump)
	}
	if dirAfterPump != path.DirNone {
		t.Fatalf("partitioned start unexpectedly has a flow direction: %s", afterPump)
	}
	if stateAfterBlocked != MoveBlocked || refsAfterBlocked != 0 {
		t.Fatalf("unreachable flow did not enter blocked state and release refs: %s", afterBlocked)
	}
	mr := w.Movements.Row(rec.Units[0])
	or := w.Orders.Row(rec.Units[0])
	if w.Movements.State[mr] != MoveIdle || w.Orders.Kind[or] != OrderStop || w.flowRefs[slot] != 0 {
		t.Fatalf("unreachable flow did not fail closed after order pass: %s", afterOrder)
	}
}

func TestWorldPathInvalidationRequeuesMoveFSV(t *testing.T) {
	w, id := pathingOrderWorld(t)
	if !w.IssueOrder(id, Order{Kind: OrderMove, Point: CellCenter(cellIdx(26, 10))}, false) {
		t.Fatal("IssueOrder refused move")
	}
	w.Step()
	mr := w.Movements.Row(id)
	pid := path.PathID(w.Movements.PathHandle[mr])
	if !w.Paths.Valid(pid) {
		t.Fatalf("test setup failed to deliver a path: %s", worldPathingDump(w, id))
	}
	wps := w.Paths.Waypoints(pid)
	stamp := wps[len(wps)/2]
	before := worldPathingDump(w, id)
	w.stampStatic(path.Rect{X: stamp % path.GridSize, Y: stamp / path.GridSize, W: 1, H: 1})
	afterStamp := worldPathingDump(w, id)
	w.pathingSystem()
	afterService := worldPathingDump(w, id)
	t.Logf("FSV path invalidation BEFORE: path=%08x stampCell=%d %s", uint32(pid), stamp, before)
	t.Logf("FSV path invalidation AFTER stamp:   oldValid=%v %s", w.Paths.Valid(pid), afterStamp)
	t.Logf("FSV path invalidation AFTER service: %s", afterService)

	if w.Paths.Valid(pid) {
		t.Fatalf("old path survived intersecting stamp: %s", afterStamp)
	}
	or := w.Orders.Row(id)
	if w.Orders.Phase[or] != orderRunning || w.Movements.State[mr] != MoveFollowing {
		t.Fatalf("repath did not redeliver a running path: %s", afterService)
	}
	if w.Movements.PathHandle[mr] == uint32(pid) || w.Movements.PathHandle[mr] == NoPath {
		t.Fatalf("movement did not switch to a new path: %s", afterService)
	}
}

func TestWorldDestroyUnitReleasesPathingHandlesFSV(t *testing.T) {
	w, id := pathingOrderWorld(t)
	if !w.IssueOrder(id, Order{Kind: OrderMove, Point: CellCenter(cellIdx(26, 10))}, false) {
		t.Fatal("IssueOrder refused move")
	}
	w.Step()
	mr := w.Movements.Row(id)
	pid := path.PathID(w.Movements.PathHandle[mr])
	beforePath := worldPathingDump(w, id)
	beforePathLive := w.Paths.Valid(pid)
	destroyedPath := w.DestroyUnit(id)
	afterPath := worldPathingDump(w, id)
	t.Logf("FSV destroy path holder BEFORE: path=%08x valid=%v %s", uint32(pid), beforePathLive, beforePath)
	t.Logf("FSV destroy path holder AFTER:  destroyed=%v valid=%v pathsLive=%d %s",
		destroyedPath, w.Paths.Valid(pid), w.Paths.Live(), afterPath)

	if !destroyedPath || !beforePathLive {
		t.Fatalf("path holder setup failed: destroyed=%v beforeValid=%v %s", destroyedPath, beforePathLive, beforePath)
	}
	if w.Paths.Valid(pid) || w.Paths.Live() != 0 {
		t.Fatalf("DestroyUnit leaked pooled path: %s", afterPath)
	}

	fw := NewWorld(Caps{Units: path.DefaultFlowThreshold})
	fw.SetGrid(openGrid())
	rec := CommandRecord{
		Version: CommandVersion,
		Player:  0,
		Opcode:  OpMove,
		Point:   CellCenter(cellIdx(80, 80)),
	}
	rec.UnitCount = uint8(path.DefaultFlowThreshold)
	for i := 0; i < path.DefaultFlowThreshold; i++ {
		id := addOwnedPathingMover(t, fw, int32(10+i%10), int32(10+i/10), 8*fixed.One, 0)
		rec.Units[i] = id
	}
	fw.applyCommandRecord(&rec)
	first := rec.Units[0]
	beforeFlow := worldPathingDump(fw, first)
	fmr := fw.Movements.Row(first)
	if fmr == -1 {
		t.Fatalf("flow holder setup failed without movement row: %s", beforeFlow)
	}
	slot := int(fw.Movements.PathHandle[fmr])
	if slot < 0 || slot >= path.FlowSlots {
		t.Fatalf("flow holder setup failed with invalid slot %d: %s", slot, beforeFlow)
	}
	refsBefore := fw.flowRefs[slot]
	destroyedOne := fw.DestroyUnit(first)
	afterOne := worldPathingDump(fw, first)
	refsAfterOne := fw.flowRefs[slot]
	_, liveAfterOne, _ := fw.pathFlow.SlotState(slot)
	for i := 1; i < int(rec.UnitCount); i++ {
		if !fw.DestroyUnit(rec.Units[i]) {
			t.Fatalf("DestroyUnit refused flow member %d", i)
		}
	}
	afterAll := worldPathingDump(fw, first)
	_, liveAfterAll, _ := fw.pathFlow.SlotState(slot)
	t.Logf("FSV destroy flow holders BEFORE: slot=%d refs=%d %s", slot, refsBefore, beforeFlow)
	t.Logf("FSV destroy flow holders AFTER one:  destroyed=%v live=%v refs=%d %s", destroyedOne, liveAfterOne, refsAfterOne, afterOne)
	t.Logf("FSV destroy flow holders AFTER all:  live=%v refs=%d %s", liveAfterAll, fw.flowRefs[slot], afterAll)

	if !destroyedOne {
		t.Fatalf("flow holder setup failed: slot=%d destroyedOne=%v %s", slot, destroyedOne, beforeFlow)
	}
	if refsBefore != uint16(path.DefaultFlowThreshold) ||
		refsAfterOne != uint16(path.DefaultFlowThreshold-1) || !liveAfterOne {
		t.Fatalf("DestroyUnit did not decrement shared flow refs after one member: %s", afterOne)
	}
	if fw.flowRefs[slot] != 0 || liveAfterAll {
		t.Fatalf("DestroyUnit leaked shared flow slot: %s", afterAll)
	}
}

func pathingOrderWorld(t *testing.T) (*World, EntityID) {
	t.Helper()
	w := NewWorld(Caps{Units: 64, PathRequests: 64})
	g := openGrid()
	for y := int32(0); y < 24; y++ {
		if y == 14 {
			continue
		}
		g.ClearFlags(18, y, path.Walkable)
	}
	w.SetGrid(g)
	return w, addOwnedPathingMover(t, w, 10, 10, 4*fixed.One, 0)
}

func addOwnedPathingMover(t *testing.T, w *World, x, y int32, speed fixed.F64, player uint8) EntityID {
	t.Helper()
	id := addMover(t, w, x, y, speed)
	if !w.Orders.Add(w.Ents, id) {
		t.Fatal("Orders.Add failed")
	}
	if !w.Owners.Add(w.Ents, id, player, player, player) {
		t.Fatal("Owners.Add failed")
	}
	return id
}

func worldPathingDump(w *World, id EntityID) string {
	var parts []string
	or := w.Orders.Row(id)
	if or == -1 {
		parts = append(parts, "order=<none>")
	} else {
		parts = append(parts, fmt.Sprintf("order={kind:%d phase:%d point:(%d,%d)}",
			w.Orders.Kind[or], w.Orders.Phase[or], int64(w.Orders.Point[or].X), int64(w.Orders.Point[or].Y)))
	}
	mr := w.Movements.Row(id)
	if mr == -1 {
		parts = append(parts, "move=<none>")
	} else {
		tr := w.Transforms.Row(id)
		pos := fixed.Vec2{}
		if tr != -1 {
			pos = w.Transforms.Pos[tr]
		}
		parts = append(parts, fmt.Sprintf("move={state:%d path:%08x wp:%d pos:(%d,%d) target:(%d,%d) res:%d}",
			w.Movements.State[mr], w.Movements.PathHandle[mr], w.Movements.WaypointIdx[mr],
			int64(pos.X), int64(pos.Y),
			int64(w.Movements.Target[mr].X), int64(w.Movements.Target[mr].Y), w.Movements.ResCell[mr]))
	}
	if w.pathQueue == nil {
		parts = append(parts, "queue=<nil>")
	} else {
		logs := w.pathQueue.Log()
		tail := ""
		if len(logs) > 0 {
			ev := logs[len(logs)-1]
			tail = fmt.Sprintf(" last={owner:%d done:%v status:%s path:%08x exp:%d parked:%v}",
				ev.Owner, ev.Done, ev.Status, uint32(ev.Path), ev.Expansions, ev.Parked)
		}
		parts = append(parts, fmt.Sprintf("queue={pending:%d inflight:%d dropped:%d%s}",
			w.pathQueue.Pending(), w.pathQueue.InFlight(), w.pathQueue.Dropped(), tail))
	}
	parts = append(parts, fmt.Sprintf("pathsLive=%d", w.Paths.Live()))
	if w.pathFlow != nil {
		refs := make([]string, 0, path.FlowSlots)
		for i := 0; i < path.FlowSlots; i++ {
			goal, live, use := w.pathFlow.SlotState(i)
			refs = append(refs, fmt.Sprintf("s%d{ref:%d goal:%d live:%v use:%d ready:%v}",
				i, w.flowRefs[i], goal, live, use, w.pathFlow.Ready(i)))
		}
		parts = append(parts, "flow=["+strings.Join(refs, ",")+"]")
	}
	return strings.Join(parts, " ")
}

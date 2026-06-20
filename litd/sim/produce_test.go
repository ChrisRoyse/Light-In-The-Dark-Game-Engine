package sim

// #302 production-queue tests. SoT = per-tick queue dumps, the
// resource/food ledgers, spawn traces (EvUnitTrained / refusal
// events), and the deterministic spawn-cell choice.

import (
	"bytes"
	"fmt"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim/path"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/statehash"
)

// Type IDs into prodDefs.
const (
	tFootman uint16 = 0
	tHall    uint16 = 1
	tWorker  uint16 = 2
)

// prodDefs is the synthetic unit table: footman 50g/2 food/40t,
// worker 25g/1 food/20t (harvester), hall trains both and provides
// 10 food + gold depot.
func prodDefs() []data.Unit {
	return []data.Unit{
		{ID: "footman", Life: 100, MoveSpeedPerTick: 2 * fixed.One, TurnRatePerTick: 65535,
			CollisionSize: 16, Costs: []int64{50, 0}, TrainTicks: 40, FoodCost: 2},
		{ID: "hall", Life: 1000, CollisionSize: 64, FoodProvided: 10, DepotMask: 0b01,
			Trains: []uint16{tFootman, tWorker}},
		{ID: "worker", Life: 80, MoveSpeedPerTick: 4 * fixed.One, TurnRatePerTick: 65535,
			CollisionSize: 16, Costs: []int64{25, 0}, TrainTicks: 20, FoodCost: 1,
			Harvest: data.HarvestSpec{GatherTicks: 10, Capacity: 10, Mask: 0b01}},
	}
}

func prodWorld(t *testing.T) (*World, EntityID) {
	t.Helper()
	w := NewWorld(Caps{Units: 64})
	if !w.BindEconomy(2) || !w.BindUnitDefs(prodDefs()) {
		t.Fatal("bind failed")
	}
	hall, ok := w.SpawnFromTable(tHall, 0, 0, pt2(200, 200))
	if !ok {
		t.Fatal("hall spawn failed")
	}
	if w.Produce.Row(hall) == -1 {
		t.Fatal("hall did not get a produce row from its trains list")
	}
	w.resources[0][0] = 500 // synthetic gold
	return w, hall
}

func qdump(w *World, b EntityID) string {
	r := w.Produce.Row(b)
	if r == -1 {
		return "no-row"
	}
	el, tot := w.TrainProgress(b)
	return fmt.Sprintf("t%-4d q=%v n=%d progress=%d/%d gold=%d food=%d/%d",
		w.Tick(), w.Produce.Queue[r][:w.Produce.QCount[r]], w.Produce.QCount[r],
		el, tot, w.Resources(0, 0), w.FoodUsed(0), w.FoodCap(0))
}

// Happy path: enqueue two footmen — cost deducted and food reserved
// at enqueue, first spawns exactly TrainTicks later, second restarts
// from zero, ledger lands exact.
func TestProduceHappyPath(t *testing.T) {
	w, hall := prodWorld(t)
	var trained []string
	w.RegisterHandler(hA, func(w *World, e Event) {
		trained = append(trained, fmt.Sprintf("t%d trained type=%d unit=%d", w.Tick(), e.Arg, e.Dst.Index()))
	})
	w.Subscribe(EvUnitTrained, hA)

	if got := w.EnqueueTrain(hall, tFootman); got != TrainOK {
		t.Fatalf("enqueue 1: reason %d", got)
	}
	if got := w.EnqueueTrain(hall, tFootman); got != TrainOK {
		t.Fatalf("enqueue 2: reason %d", got)
	}
	t.Logf("after enqueue: %s", qdump(w, hall))
	if w.Resources(0, 0) != 400 || w.FoodUsed(0) != 4 {
		t.Fatalf("admission ledger wrong: gold=%d food=%d (want 400, 4)", w.Resources(0, 0), w.FoodUsed(0))
	}
	for i := 0; i < 81; i++ {
		w.Step()
	}
	t.Logf("after 81 ticks: %s", qdump(w, hall))
	t.Logf("spawn trace: %v", trained)
	if len(trained) != 2 {
		t.Fatalf("trained %d units, want 2: %v", len(trained), trained)
	}
	// Done was stamped at enqueue (tick 0): head completes the step
	// where tick reaches 40, the requeued second 40 ticks later
	want := []string{"t40 trained", "t80 trained"}
	for i, p := range want {
		if len(trained[i]) < len(p) || trained[i][:len(p)] != p {
			t.Fatalf("spawn %d at wrong tick: %q want prefix %q", i, trained[i], p)
		}
	}
	if w.UnitCount() != 3 || w.FoodUsed(0) != 4 || w.Resources(0, 0) != 400 {
		t.Fatalf("post-spawn state: units=%d food=%d gold=%d", w.UnitCount(), w.FoodUsed(0), w.Resources(0, 0))
	}
	// the spawned footman is a full table unit
	r := w.Produce.Row(hall)
	if w.Produce.QCount[r] != 0 || w.Produce.Done[r] != 0 {
		t.Fatalf("queue not drained: %s", qdump(w, hall))
	}
}

// A trained unit stages a non-hashing RenderUnitReady presentation cue (#313)
// carrying its unit-type id, in the snapshot of the spawn tick. X+X=Y: train a
// worker (type id 2) → exactly one RenderUnitReady with Data == 2, at the tick the
// worker spawns; no cue on any other tick.
func TestSnapshotUnitReadyRenderEventFSV(t *testing.T) {
	w, hall := prodWorld(t)
	if got := w.EnqueueTrain(hall, tWorker); got != TrainOK {
		t.Fatalf("enqueue worker: reason %d", got)
	}
	var found []RenderEvent
	var readyTick uint32
	for i := 0; i < 25; i++ { // worker TrainTicks=20 → spawns at tick 20
		w.Step()
		for _, e := range w.Snaps.Curr().Events {
			if e.Kind == RenderUnitReady {
				found = append(found, e)
				readyTick = w.Snaps.Curr().Tick
			}
		}
	}
	if len(found) != 1 {
		t.Fatalf("RenderUnitReady cues = %d, want exactly 1", len(found))
	}
	if found[0].Data != tWorker {
		t.Fatalf("ready cue Data = %d, want tWorker (%d)", found[0].Data, tWorker)
	}
	// the cue's entity is a live, present unit (UI routing needs no position, but
	// the snapshot still carries it).
	if _, present := snapEntry(w.Snaps.Curr(), found[0].Ent); !present {
		t.Fatalf("trained unit %d absent from snapshot", found[0].Ent.Index())
	}
	t.Logf("FSV #313 sim: trained worker → 1 RenderUnitReady{Data=typeID=%d} at spawn tick %d", tWorker, readyTick)
}

// Edge 1: queue cap — the 8th enqueue refuses with TrainQueueFull and
// deducts nothing.
func TestProduceQueueFullNoDeduction(t *testing.T) {
	w, hall := prodWorld(t)
	w.resources[0][0] = 10000
	for i := 0; i < ProduceQueueCap; i++ {
		if got := w.EnqueueTrain(hall, tWorker); got != TrainOK {
			t.Fatalf("enqueue %d: reason %d", i+1, got)
		}
	}
	before := qdump(w, hall)
	got := w.EnqueueTrain(hall, tWorker)
	after := qdump(w, hall)
	t.Logf("before 8th: %s", before)
	t.Logf("8th -> reason %d", got)
	t.Logf("after 8th:  %s", after)
	if got != TrainQueueFull {
		t.Fatalf("8th enqueue: reason %d, want TrainQueueFull(%d)", got, TrainQueueFull)
	}
	if before != after {
		t.Fatalf("refusal changed state:\nbefore %s\nafter  %s", before, after)
	}
	if w.Resources(0, 0) != 10000-7*25 || w.FoodUsed(0) != 7 {
		t.Fatalf("ledger after 7 admits: gold=%d food=%d", w.Resources(0, 0), w.FoodUsed(0))
	}
}

// Edge 2: cancel — mid-queue cancel refunds in full and shifts; head
// cancel restarts the next entry from zero progress.
func TestProduceCancelRefund(t *testing.T) {
	w, hall := prodWorld(t)
	w.EnqueueTrain(hall, tFootman) // slot 0 (head)
	w.EnqueueTrain(hall, tWorker)  // slot 1
	w.EnqueueTrain(hall, tFootman) // slot 2
	for i := 0; i < 10; i++ {
		w.Step()
	}
	r := w.Produce.Row(hall)
	t.Logf("before cancel slot 1: %s", qdump(w, hall))
	if !w.CancelTrain(hall, 1) {
		t.Fatal("cancel slot 1 refused")
	}
	t.Logf("after cancel slot 1:  %s", qdump(w, hall))
	if w.Resources(0, 0) != 500-2*50 || w.FoodUsed(0) != 4 {
		t.Fatalf("mid-queue refund wrong: gold=%d food=%d (want 400, 4)", w.Resources(0, 0), w.FoodUsed(0))
	}
	if w.Produce.QCount[r] != 2 || w.Produce.Queue[r][0] != tFootman || w.Produce.Queue[r][1] != tFootman {
		t.Fatalf("queue did not shift: %v n=%d", w.Produce.Queue[r], w.Produce.QCount[r])
	}
	if w.Produce.Queue[r][2] != 0 || w.Produce.QFlags[r][2] != 0 {
		t.Fatalf("freed tail slot not zeroed: %v", w.Produce.Queue[r])
	}
	doneBefore := w.Produce.Done[r]
	if !w.CancelTrain(hall, 0) { // head: 10 ticks of progress lost
		t.Fatal("cancel head refused")
	}
	t.Logf("after cancel head:    %s (head clock %d -> %d)", qdump(w, hall), doneBefore, w.Produce.Done[r])
	if w.Resources(0, 0) != 500-50 || w.FoodUsed(0) != 2 {
		t.Fatalf("head refund wrong: gold=%d food=%d (want 450, 2)", w.Resources(0, 0), w.FoodUsed(0))
	}
	if w.Produce.Done[r] != w.Tick()+40 {
		t.Fatalf("next entry did not restart from zero: done=%d want %d", w.Produce.Done[r], w.Tick()+40)
	}
	if !w.CancelTrain(hall, 0) || w.Produce.Done[r] != 0 || w.Produce.QCount[r] != 0 {
		t.Fatalf("emptying cancel wrong: %s", qdump(w, hall))
	}
	if w.Resources(0, 0) != 500 || w.FoodUsed(0) != 0 {
		t.Fatalf("full refund chain wrong: gold=%d food=%d (want 500, 0)", w.Resources(0, 0), w.FoodUsed(0))
	}
	if w.CancelTrain(hall, 0) {
		t.Fatal("cancel on empty queue accepted")
	}
}

// Refusal vocabulary: every admission gate refuses with its named
// reason and zero state change.
func TestProduceAdmissionRefusals(t *testing.T) {
	w, hall := prodWorld(t)
	footman, _ := w.SpawnFromTable(tFootman, 0, 0, pt2(400, 400))
	check := func(name string, got, want uint8) {
		t.Helper()
		ledger := qdump(w, hall)
		t.Logf("%s -> reason %d (%s)", name, got, ledger)
		if got != want {
			t.Fatalf("%s: reason %d, want %d", name, got, want)
		}
	}
	check("unknown type", w.EnqueueTrain(hall, 99), TrainUnknownType)
	check("not in trains list", w.EnqueueTrain(hall, tHall), TrainNotTrainable)
	check("non-producer", w.EnqueueTrain(footman, tWorker), TrainNoProducer)
	w.SetTechGate(func(p uint8, typeID uint16) bool { return false })
	check("tech locked", w.EnqueueTrain(hall, tFootman), TrainTechLocked)
	w.SetTechGate(nil)
	w.resources[0][0] = 49
	check("no resources", w.EnqueueTrain(hall, tFootman), TrainNoResources)
	w.resources[0][0] = 500
	w.foodUsed[0] = w.foodCap[0] - 1 // 1 food headroom < footman's 2
	check("no food", w.EnqueueTrain(hall, tFootman), TrainNoFood)
	if w.Resources(0, 0) != 500 || w.Produce.QCount[w.Produce.Row(hall)] != 0 {
		t.Fatalf("refusals changed state: %s", qdump(w, hall))
	}
}

// Edge 5: food cap hit between enqueue and completion — the unit
// still spawns; its reservation was taken at admission.
func TestProduceFoodReservedAtEnqueue(t *testing.T) {
	w, hall := prodWorld(t)
	if got := w.EnqueueTrain(hall, tFootman); got != TrainOK {
		t.Fatalf("enqueue: %d", got)
	}
	// eat ALL remaining headroom after the reservation: 10 cap - 2
	// reserved = 8 more food
	w.foodUsed[0] = w.foodCap[0]
	t.Logf("cap saturated mid-train: %s", qdump(w, hall))
	if w.CanAddFood(0, 1) {
		t.Fatal("test premise broken: headroom left")
	}
	for i := 0; i < 41; i++ {
		w.Step()
	}
	t.Logf("after completion: %s units=%d", qdump(w, hall), w.UnitCount())
	if w.UnitCount() != 2 {
		t.Fatalf("reserved unit did not spawn at full cap: units=%d", w.UnitCount())
	}
	if w.FoodUsed(0) != w.FoodCap(0) {
		t.Fatalf("ledger drifted: food=%d/%d (reservation handover must be net zero)", w.FoodUsed(0), w.FoodCap(0))
	}
}

// Edge 4: rally onto a resource node — the spawned worker resolves
// harvest through the smart table and gathers without a player order.
func TestProduceRallyResourceHarvest(t *testing.T) {
	w, hall := prodWorld(t)
	smart := &data.SmartTable{
		UnitClasses: []string{"basic", "worker"},
		Rules:       make([][]uint8, data.TargetClassCount),
	}
	for tc := range smart.Rules {
		smart.Rules[tc] = []uint8{OpMove, OpMove}
	}
	smart.Rules[data.TCResource] = []uint8{OpMove, OpHarvest}
	if !w.BindSmartTable(smart, []uint8{0, 0, 1}) { // worker = class 1
		t.Fatal("smart bind failed")
	}
	mine := addMine(t, w, pt2(300, 200), 1000, false)
	if !w.SetRallyTarget(hall, mine) {
		t.Fatal("rally set failed")
	}
	if got := w.EnqueueTrain(hall, tWorker); got != TrainOK {
		t.Fatalf("enqueue: %d", got)
	}
	var spawned EntityID
	w.RegisterHandler(hA, func(w *World, e Event) { spawned = e.Dst })
	w.Subscribe(EvUnitTrained, hA)
	for i := 0; i < 21; i++ {
		w.Step()
	}
	if spawned == 0 {
		t.Fatal("no spawn")
	}
	or := w.Orders.Row(spawned)
	t.Logf("t%d spawned worker %d order kind=%d target=%d", w.Tick(), spawned.Index(), w.Orders.Kind[or], w.Orders.Target[or].Index())
	if w.Orders.Kind[or] != OrderHarvest || w.Orders.Target[or] != mine {
		t.Fatalf("rally did not resolve to harvest: kind=%d", w.Orders.Kind[or])
	}
	for i := 0; i < 400 && w.Resources(0, 0) <= 500-25; i++ {
		w.Step()
	}
	t.Logf("t%d gold=%d (rally worker delivering)", w.Tick(), w.Resources(0, 0))
	if w.Resources(0, 0) <= 500-25 {
		t.Fatal("rally worker never deposited")
	}

	// point rally on the same building: next unit moves there
	if !w.SetRallyPoint(hall, pt2(500, 500)) {
		t.Fatal("rally point failed")
	}
	spawned = 0
	w.EnqueueTrain(hall, tFootman)
	for i := 0; i < 41 && spawned == 0; i++ {
		w.Step()
	}
	or = w.Orders.Row(spawned)
	t.Logf("t%d footman %d order kind=%d point=(%d,%d)", w.Tick(), spawned.Index(), w.Orders.Kind[or], w.Orders.Point[or].X.Floor(), w.Orders.Point[or].Y.Floor())
	if w.Orders.Kind[or] != OrderMove || w.Orders.Point[or] != pt2(500, 500) {
		t.Fatalf("point rally did not issue move: kind=%d", w.Orders.Kind[or])
	}
}

// Edge 3: exit cells blocked — the scan deterministically skips to
// the first free cell; two runs choose the identical position.
func TestProduceSpawnBlockedDeterministicScan(t *testing.T) {
	run := func() (fixed.Vec2, string) {
		w, hall := prodWorld(t)
		g := path.NewGrid()
		for y := int32(0); y < path.GridSize; y++ {
			for x := int32(0); x < path.GridSize; x++ {
				g.SetFlags(x, y, path.Walkable)
			}
		}
		// hall at (200,200), footman ring-1 distance = (64+16+32) =
		// 112 wu. Block E, N, W candidates (dirs 0..2); S stays free.
		for _, c := range []fixed.Vec2{
			{X: pt2(312, 200).X, Y: pt2(312, 200).Y},
			{X: pt2(200, 312).X, Y: pt2(200, 312).Y},
			{X: pt2(88, 200).X, Y: pt2(88, 200).Y},
		} {
			cell := cellOfPos(c)
			g.OrFlags(cell%path.GridSize, cell/path.GridSize, path.OccupiedStatic)
		}
		w.SetGrid(g)
		var spawned EntityID
		w.RegisterHandler(hA, func(w *World, e Event) { spawned = e.Dst })
		w.Subscribe(EvUnitTrained, hA)
		w.EnqueueTrain(hall, tFootman)
		for i := 0; i < 41 && spawned == 0; i++ {
			w.Step()
		}
		tr := w.Transforms.Row(spawned)
		pos := w.Transforms.Pos[tr]
		return pos, fmt.Sprintf("spawned at (%d,%d) cell=%d", pos.X.Floor(), pos.Y.Floor(), cellOfPos(pos))
	}
	p1, log1 := run()
	p2, log2 := run()
	t.Logf("run 1: %s", log1)
	t.Logf("run 2: %s", log2)
	if p1 != p2 {
		t.Fatalf("spawn cell not deterministic: %v vs %v", p1, p2)
	}
	want := pt2(200, 88) // dir 3 (S): first free candidate
	if p1 != want {
		t.Fatalf("scan did not skip blocked cells in order: got (%d,%d) want (%d,%d)",
			p1.X.Floor(), p1.Y.Floor(), want.X.Floor(), want.Y.Floor())
	}
}

// Building dies mid-queue: food reservations release, resources stay
// spent (destruction is not a cancel).
func TestProduceBuildingDeathLedger(t *testing.T) {
	w, hall := prodWorld(t)
	w.EnqueueTrain(hall, tFootman)
	w.EnqueueTrain(hall, tWorker)
	t.Logf("before death: %s", qdump(w, hall))
	if w.FoodUsed(0) != 3 || w.Resources(0, 0) != 425 {
		t.Fatalf("setup ledger: food=%d gold=%d", w.FoodUsed(0), w.Resources(0, 0))
	}
	w.KillUnit(hall)
	w.Step()
	t.Logf("after death: gold=%d food=%d/%d produceRows=%d", w.Resources(0, 0), w.FoodUsed(0), w.FoodCap(0), w.Produce.Count())
	if w.FoodUsed(0) != 0 || w.FoodCap(0) != 0 {
		t.Fatalf("reservations leaked: food=%d/%d", w.FoodUsed(0), w.FoodCap(0))
	}
	if w.Resources(0, 0) != 425 {
		t.Fatalf("destruction refunded resources: gold=%d", w.Resources(0, 0))
	}
	if w.Produce.Count() != 0 {
		t.Fatal("produce row leaked")
	}
}

// Twin determinism: identical command sequences produce identical
// state hashes through enqueue/cancel/completion/rally.
func TestProduceDeterminism(t *testing.T) {
	build := func() *World {
		w := NewWorld(Caps{Units: 64})
		w.BindEconomy(2)
		w.BindUnitDefs(prodDefs())
		hall, _ := w.SpawnFromTable(tHall, 0, 0, pt2(200, 200))
		w.resources[0][0] = 500
		nt := data.ResourceNodeType{ID: "mine", Resource: 0, Amount: 1000}
		mine, _ := w.CreateResourceNode(pt2(300, 200), &nt)
		smart := &data.SmartTable{UnitClasses: []string{"basic", "worker"}, Rules: make([][]uint8, data.TargetClassCount)}
		for tc := range smart.Rules {
			smart.Rules[tc] = []uint8{OpMove, OpMove}
		}
		smart.Rules[data.TCResource] = []uint8{OpMove, OpHarvest}
		w.BindSmartTable(smart, []uint8{0, 0, 1})
		w.SetRallyTarget(hall, mine)
		w.EnqueueTrain(hall, tWorker)
		w.EnqueueTrain(hall, tFootman)
		w.EnqueueTrain(hall, tWorker)
		w.CancelTrain(hall, 1)
		for i := 0; i < 300; i++ {
			w.Step()
		}
		return w
	}
	a, b := build(), build()
	reg := NewHashRegistry()
	var sa, sb statehash.Snapshot
	a.HashState(reg, &sa)
	b.HashState(NewHashRegistry(), &sb)
	t.Logf("twin A: %016x  twin B: %016x (gold A=%d B=%d)", sa.Top, sb.Top, a.Resources(0, 0), b.Resources(0, 0))
	if sa.Top != sb.Top {
		t.Fatalf("twin divergence: %016x vs %016x", sa.Top, sb.Top)
	}
	// hash sensitivity: a queue mutation moves the sum
	hall2, _ := a.SpawnFromTable(tHall, 1, 1, pt2(600, 600))
	a.resources[1] = a.resources[1][:2]
	a.resources[1][0] = 100
	if got := a.EnqueueTrain(hall2, tWorker); got != TrainOK {
		t.Fatalf("sensitivity enqueue: %d", got)
	}
	var sa2 statehash.Snapshot
	a.HashState(reg, &sa2)
	if sa2.Top == sa.Top {
		t.Fatal("queue mutation invisible to the state hash")
	}
}

// Save round-trip with live queues: byte state restores, resumes
// identically, and the food ledger recomputes the reservations.
func TestProduceSaveRoundTrip(t *testing.T) {
	w, hall := prodWorld(t)
	w.EnqueueTrain(hall, tFootman)
	w.EnqueueTrain(hall, tWorker)
	w.SetRallyPoint(hall, pt2(500, 500))
	for i := 0; i < 10; i++ {
		w.Step()
	}
	var buf bytes.Buffer
	if err := w.SaveState(&buf, 7); err != nil {
		t.Fatal(err)
	}
	w2 := NewWorld(Caps{Units: 64})
	w2.BindEconomy(2)
	w2.BindUnitDefs(prodDefs())
	if err := w2.LoadState(bytes.NewReader(buf.Bytes()), 7); err != nil {
		t.Fatal(err)
	}
	if w2.FoodUsed(0) != w.FoodUsed(0) {
		t.Fatalf("reservation ledger lost in load: %d vs %d", w2.FoodUsed(0), w.FoodUsed(0))
	}
	reg := NewHashRegistry()
	var sa, sb statehash.Snapshot
	w.HashState(reg, &sa)
	w2.HashState(NewHashRegistry(), &sb)
	t.Logf("saved=%016x loaded=%016x food=%d/%d", sa.Top, sb.Top, w2.FoodUsed(0), w2.FoodCap(0))
	if sa.Top != sb.Top {
		t.Fatalf("load diverged: %016x vs %016x", sa.Top, sb.Top)
	}
	for i := 0; i < 120; i++ {
		w.Step()
		w2.Step()
	}
	w.HashState(reg, &sa)
	w2.HashState(NewHashRegistry(), &sb)
	t.Logf("resumed 120t: orig=%016x loaded=%016x units=%d/%d", sa.Top, sb.Top, w.UnitCount(), w2.UnitCount())
	if sa.Top != sb.Top {
		t.Fatal("resume diverged")
	}

	// queued trains with unbound defs must refuse
	w3 := NewWorld(Caps{Units: 64})
	w3.BindEconomy(2)
	if err := w3.LoadState(bytes.NewReader(buf.Bytes()), 7); err == nil {
		t.Fatal("load with unbound unit defs accepted")
	} else {
		t.Logf("unbound defs refused: %v", err)
	}
}

// R-GC-1: steady-state production ticks allocate nothing.
func TestProduceTickAllocs(t *testing.T) {
	w, hall := prodWorld(t)
	w.resources[0][0] = 100000
	for i := 0; i < ProduceQueueCap; i++ {
		w.EnqueueTrain(hall, tWorker)
	}
	w.Step() // warm
	allocs := testing.AllocsPerRun(100, func() {
		w.Step()
		if r := w.Produce.Row(hall); w.Produce.QCount[r] < ProduceQueueCap {
			w.EnqueueTrain(hall, tWorker) // keep the line hot
		}
	})
	t.Logf("allocs/op with active production: %v (units now %d)", allocs, w.UnitCount())
	if allocs != 0 {
		t.Fatalf("production tick allocates: %v allocs/op", allocs)
	}
}

func BenchmarkProduceTick(b *testing.B) {
	w := NewWorld(Caps{Units: 256})
	w.BindEconomy(2)
	w.BindUnitDefs(prodDefs())
	hall, _ := w.SpawnFromTable(tHall, 0, 0, fixed.Vec2{X: fixed.FromInt(200), Y: fixed.FromInt(200)})
	w.resources[0][0] = 1 << 40
	w.foodCap[0] = 1 << 20 // synthetic headroom: the line never starves
	for i := 0; i < ProduceQueueCap; i++ {
		w.EnqueueTrain(hall, tWorker)
	}
	b.ReportAllocs()
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		w.Step()
		if r := w.Produce.Row(hall); w.Produce.QCount[r] < ProduceQueueCap {
			w.EnqueueTrain(hall, tWorker)
		}
	}
}

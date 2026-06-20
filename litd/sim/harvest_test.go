package sim

// #300 tests. SoT = tick-stamped harvester traces, per-player
// resource counters, node remaining amounts, food ledgers — all
// printed and read.

import (
	"fmt"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/statehash"
)

var workerSpec = data.HarvestSpec{GatherTicks: 10, Capacity: 10, Mask: 0b01} // gold only

// econWorld: bound 2-resource economy.
func econWorld(t *testing.T) *World {
	t.Helper()
	w := NewWorld(Caps{})
	if !w.BindEconomy(2) {
		t.Fatal("BindEconomy failed")
	}
	return w
}

func pt2(x, y int32) fixed.Vec2 { return fixed.Vec2{X: fixed.FromInt(x), Y: fixed.FromInt(y)} }

// addWorker: movable unit with the harvest component, player 0.
func addWorker(t *testing.T, w *World, pos fixed.Vec2) EntityID {
	t.Helper()
	id, ok := w.CreateUnit(pos, 0)
	if !ok || !w.Owners.Add(w.Ents, id, 0, 0, 0) ||
		!w.Orders.Add(w.Ents, id) ||
		!w.Movements.Add(w.Ents, w.Transforms, id, fixed.One*4, 65535) ||
		!w.Harvests.Add(w.Ents, id, &workerSpec) {
		t.Fatal("worker setup failed")
	}
	if !w.AddEcon(id, 1, 0, 0) { // food-cost 1
		t.Fatal("worker econ failed")
	}
	return id
}

// addDepot: stationary own building accepting gold.
func addDepot(t *testing.T, w *World, pos fixed.Vec2, foodProvided uint8) EntityID {
	t.Helper()
	id, ok := w.CreateUnit(pos, 0)
	if !ok || !w.Owners.Add(w.Ents, id, 0, 0, 0) {
		t.Fatal("depot setup failed")
	}
	if !w.AddEcon(id, 0, foodProvided, 0b01) {
		t.Fatal("depot econ failed")
	}
	return id
}

func addMine(t *testing.T, w *World, pos fixed.Vec2, amount int64, exclusive bool) EntityID {
	t.Helper()
	nt := data.ResourceNodeType{ID: "mine", Resource: 0, Amount: amount, Exclusive: exclusive}
	id, ok := w.CreateResourceNode(pos, &nt)
	if !ok {
		t.Fatal("node setup failed")
	}
	return id
}

func hstate(w *World, id EntityID) string {
	hr := w.Harvests.Row(id)
	names := []string{"idle", "to-node", "gather", "return"}
	return fmt.Sprintf("t%-4d %s carried=%d", w.Tick(), names[w.Harvests.State[hr]], w.Harvests.Carried[hr])
}

// Happy path: full cycles accumulate exact amounts; the trace shows
// the MOVE_TO_NODE → GATHER → RETURN → DEPOSIT loop.

// TestBindResourceNodeDefsFSV — #401: the node-type registry resolves codes to
// ids and spawns nodes by id that carry the table's exact Resource/Amount. SoT =
// the stored Nodes component (Remaining/Resource), not the create bool, plus the
// fail-closed guards (unknown code, out-of-range id, length-mismatch rebind).
func TestBindResourceNodeDefsFSV(t *testing.T) {
	w := econWorld(t) // BindEconomy(2): resources 0 and 1 valid
	defs := []data.ResourceNodeType{
		{ID: "goldmine", Resource: 0, Amount: 500},
		{ID: "tree", Resource: 1, Amount: 200, Exclusive: true},
	}
	if !w.BindResourceNodeDefs(defs) {
		t.Fatal("BindResourceNodeDefs rejected a valid table")
	}
	gid, ok := w.ResourceNodeTypeID("goldmine")
	if !ok || gid != 0 {
		t.Fatalf("goldmine id = %d,%v want 0,true", gid, ok)
	}
	if tid, ok := w.ResourceNodeTypeID("tree"); !ok || tid != 1 {
		t.Fatalf("tree id = %d,%v want 1,true", tid, ok)
	}
	if _, ok := w.ResourceNodeTypeID("nope"); ok {
		t.Fatal("unknown code resolved — must be ok=false")
	}

	id, ok := w.CreateResourceNodeByID(pt2(140, 100), gid)
	if !ok {
		t.Fatal("CreateResourceNodeByID(goldmine) failed")
	}
	nr := w.Nodes.Row(id)
	if nr == -1 {
		t.Fatal("spawned node has no Nodes component")
	}
	t.Logf("FSV #401: goldmine node entity=%d Remaining=%d Resource=%d (want 500,0)",
		id, w.Nodes.Remaining[nr], w.Nodes.Resource[nr])
	if w.Nodes.Remaining[nr] != 500 || w.Nodes.Resource[nr] != 0 {
		t.Fatalf("node state wrong: Remaining=%d Resource=%d, want 500,0", w.Nodes.Remaining[nr], w.Nodes.Resource[nr])
	}
	// Fail-closed: an out-of-range id and a length-mismatch rebind both refuse.
	if _, ok := w.CreateResourceNodeByID(pt2(0, 0), 99); ok {
		t.Fatal("out-of-range node id must fail, got ok=true")
	}
	if w.BindResourceNodeDefs(defs[:1]) {
		t.Fatal("rebind to a different length must fail")
	}
}

func TestHarvestCycle(t *testing.T) {
	w := econWorld(t)
	worker := addWorker(t, w, pt2(100, 100))
	mine := addMine(t, w, pt2(140, 100), 35, false)
	addDepot(t, w, pt2(60, 100), 10)
	deposits := 0
	w.RegisterHandler(21, func(w *World, e Event) {
		deposits++
		t.Logf("t%-4d DEPOSIT amount=%d res=%d (worker %d -> depot %d)", w.Tick(), e.Arg>>8, e.Arg&0xff, e.Src, e.Dst)
	})
	w.Subscribe(EvResourceDeposited, 21)
	depleted := uint32(0)
	w.RegisterHandler(22, func(w *World, e Event) { depleted = w.Tick() })
	w.Subscribe(EvResourceDepleted, 22)

	if !w.IssueOrder(worker, Order{Kind: OrderHarvest, Target: mine}, false) {
		t.Fatal("harvest order refused")
	}
	last := ""
	for i := 0; i < 400 && w.Resources(0, 0) < 35; i++ {
		w.Step()
		if s := hstate(w, worker); s[5:] != last {
			t.Log(s)
			last = s[5:]
		}
	}
	t.Logf("counters: gold=%d lumber=%d  deposits=%d  node-depleted@t%d  node alive=%v",
		w.Resources(0, 0), w.Resources(0, 1), deposits, depleted, w.Ents.Alive(mine))
	if w.Resources(0, 0) != 35 {
		t.Fatalf("gold = %d, want 35 (3 full trips + 5 remainder)", w.Resources(0, 0))
	}
	if deposits != 4 || w.Ents.Alive(mine) || depleted == 0 {
		t.Fatalf("deposits=%d mineAlive=%v depleted=%d", deposits, w.Ents.Alive(mine), depleted)
	}
}

// Edge 1: node depletes while OTHER workers target it → deterministic
// re-selection of the nearest survivor by (distSq, entityIndex).
func TestHarvestNodeDepletionReselect(t *testing.T) {
	w := econWorld(t)
	worker := addWorker(t, w, pt2(100, 100))
	near := addMine(t, w, pt2(130, 100), 10, false) // one trip drains it
	farA := addMine(t, w, pt2(170, 100), 100, false)
	addMine(t, w, pt2(180, 100), 100, false) // farther: must NOT be picked
	addDepot(t, w, pt2(60, 100), 0)
	w.IssueOrder(worker, Order{Kind: OrderHarvest, Target: near}, false)
	for i := 0; i < 300 && w.Resources(0, 0) < 20; i++ {
		w.Step()
	}
	hr := w.Harvests.Row(worker)
	t.Logf("after depletion of near mine: worker node=%d (farA=%d) gold=%d", w.Harvests.Node[hr], farA, w.Resources(0, 0))
	if w.Harvests.Node[hr] != farA {
		t.Fatalf("re-selected node %d, want nearest survivor %d", w.Harvests.Node[hr], farA)
	}
}

// Edge 2: depot destroyed while returning → worker re-paths to the
// next depot; with none left it idles HOLDING its cargo.
func TestHarvestDepotDestroyed(t *testing.T) {
	w := econWorld(t)
	worker := addWorker(t, w, pt2(100, 100))
	mine := addMine(t, w, pt2(130, 100), 1000, false)
	d1 := addDepot(t, w, pt2(40, 100), 0)
	d2 := addDepot(t, w, pt2(30, 100), 0) // farther backup
	w.IssueOrder(worker, Order{Kind: OrderHarvest, Target: mine}, false)
	hr := w.Harvests.Row(worker)
	killed := false
	for i := 0; i < 300 && w.Resources(0, 0) == 0; i++ {
		w.Step()
		if !killed && w.Harvests.State[hr] == HReturn {
			w.OnCombatPhase = func(uint32) {}
			w.KillUnit(d1)
			killed = true
			t.Logf("t%d: killed primary depot %d mid-return (worker carrying %d)", w.Tick(), d1, w.Harvests.Carried[hr])
		}
	}
	t.Logf("gold=%d worker depot now=%d (backup=%d)", w.Resources(0, 0), w.Harvests.Depot[hr], d2)
	if w.Resources(0, 0) == 0 {
		t.Fatal("worker never delivered via the backup depot")
	}

	// no depots at all: worker finishes idle, cargo retained
	w2 := econWorld(t)
	lone := addWorker(t, w2, pt2(100, 100))
	mine2 := addMine(t, w2, pt2(130, 100), 1000, false)
	w2.IssueOrder(lone, Order{Kind: OrderHarvest, Target: mine2}, false)
	for i := 0; i < 200; i++ {
		w2.Step()
	}
	hr2 := w2.Harvests.Row(lone)
	t.Logf("no-depot world: state=%d carried=%d order-kind=%d", w2.Harvests.State[hr2], w2.Harvests.Carried[hr2], w2.Orders.Kind[w2.Orders.Row(lone)])
	if w2.Harvests.State[hr2] != HIdle || w2.Harvests.Carried[hr2] != 10 {
		t.Fatalf("worker should idle holding 10, got state=%d carried=%d", w2.Harvests.State[hr2], w2.Harvests.Carried[hr2])
	}
}

// Edge 3: the food ledger and the admission gate — at cap refused, a
// death frees supply, admission succeeds.
func TestHarvestFoodAdmission(t *testing.T) {
	w := econWorld(t)
	addDepot(t, w, pt2(60, 100), 2) // cap 2
	w1 := addWorker(t, w, pt2(100, 100))
	addWorker(t, w, pt2(104, 100))
	t.Logf("ledger: used=%d cap=%d", w.FoodUsed(0), w.FoodCap(0))
	if w.FoodUsed(0) != 2 || w.FoodCap(0) != 2 {
		t.Fatalf("ledger wrong: %d/%d", w.FoodUsed(0), w.FoodCap(0))
	}
	if w.CanAddFood(0, 1) {
		t.Fatal("admission at cap not refused")
	}
	t.Logf("CanAddFood(0,1) at cap = false ✓")
	w.KillUnit(w1)
	w.Step() // phase 7 destroys, ledger releases
	t.Logf("after death: used=%d cap=%d CanAddFood=%v", w.FoodUsed(0), w.FoodCap(0), w.CanAddFood(0, 1))
	if w.FoodUsed(0) != 1 || !w.CanAddFood(0, 1) {
		t.Fatalf("ledger after death: %d/%d", w.FoodUsed(0), w.FoodCap(0))
	}
}

// Edge 4: two workers, one node — shared flag lets both gather;
// exclusive admits one and the other waits its turn.
func TestHarvestExclusiveVsShared(t *testing.T) {
	for _, exclusive := range []bool{false, true} {
		w := econWorld(t)
		wa := addWorker(t, w, pt2(100, 96))
		wb := addWorker(t, w, pt2(100, 104))
		mine := addMine(t, w, pt2(120, 100), 1000, exclusive)
		addDepot(t, w, pt2(60, 100), 0)
		w.IssueOrder(wa, Order{Kind: OrderHarvest, Target: mine}, false)
		w.IssueOrder(wb, Order{Kind: OrderHarvest, Target: mine}, false)
		bothGathering := false
		for i := 0; i < 120; i++ {
			w.Step()
			ra, rb := w.Harvests.Row(wa), w.Harvests.Row(wb)
			if w.Harvests.State[ra] == HGather && w.Harvests.State[rb] == HGather {
				bothGathering = true
			}
		}
		t.Logf("exclusive=%v: bothGatheringSimultaneously=%v gold=%d", exclusive, bothGathering, w.Resources(0, 0))
		if exclusive && bothGathering {
			t.Fatal("exclusive node admitted two gatherers at once")
		}
		if !exclusive && !bothGathering {
			t.Fatal("shared node never had both gathering")
		}
		if w.Resources(0, 0) == 0 {
			t.Fatalf("exclusive=%v: nothing deposited", exclusive)
		}
	}
}

// Edge 5: twin runs produce identical counters and identical full
// state hashes.
func TestHarvestDeterminism(t *testing.T) {
	run := func() (int64, uint64) {
		w := econWorld(t)
		for i := 0; i < 4; i++ {
			addWorker(t, w, pt2(100+4*int32(i), 100))
		}
		m := addMine(t, w, pt2(140, 100), 500, true)
		addDepot(t, w, pt2(60, 100), 0)
		for i := int32(0); i < w.Harvests.Count(); i++ {
			w.IssueOrder(w.Harvests.Entity[i], Order{Kind: OrderHarvest, Target: m}, false)
		}
		for i := 0; i < 600; i++ {
			w.Step()
		}
		reg := NewHashRegistry()
		var snap statehash.Snapshot
		w.HashState(reg, &snap)
		return w.Resources(0, 0), snap.Top
	}
	g1, h1 := run()
	g2, h2 := run()
	t.Logf("run1 gold=%d hash=%016x; run2 gold=%d hash=%016x", g1, h1, g2, h2)
	if g1 != g2 || h1 != h2 {
		t.Fatal("twin harvest runs diverged")
	}
}

// R-GC-1: the harvest drive path allocates nothing at steady state.
func TestHarvestTickAllocs(t *testing.T) {
	w := econWorld(t)
	for i := 0; i < 8; i++ {
		addWorker(t, w, pt2(100+4*int32(i), 100))
	}
	m := addMine(t, w, pt2(140, 100), 1<<40, false)
	addDepot(t, w, pt2(60, 100), 0)
	for i := int32(0); i < w.Harvests.Count(); i++ {
		w.IssueOrder(w.Harvests.Entity[i], Order{Kind: OrderHarvest, Target: m}, false)
	}
	for i := 0; i < 100; i++ {
		w.Step()
	}
	avg := testing.AllocsPerRun(200, func() { w.Step() })
	t.Logf("allocs/tick=%v gold=%d", avg, w.Resources(0, 0))
	if avg != 0 {
		t.Fatalf("allocs/tick = %v, want 0", avg)
	}
	if w.Resources(0, 0) == 0 {
		t.Fatal("degenerate: no harvesting during alloc check")
	}
}

// TestHarvestSharedNodeDepletesOnceFSV (#300 edge): a shared node drained to zero
// by TWO workers gathering on the SAME tick must emit EvResourceDepleted exactly
// ONCE. Bug: the second worker computes take=0 (node already at 0; its kill is
// deferred to phase 7 so the row still resolves) yet still ran the `Remaining==0`
// depletion branch — re-emitting EvResourceDepleted (KillUnit dedups, Emit did
// not) and hauling an empty load that deposits 0. SoT = the EvResourceDepleted
// event count + deposited gold.
func TestHarvestSharedNodeDepletesOnceFSV(t *testing.T) {
	w := econWorld(t)
	wa := addWorker(t, w, pt2(100, 96))
	wb := addWorker(t, w, pt2(100, 104))
	mine := addMine(t, w, pt2(120, 100), 10, false) // shared; exactly one worker's capacity
	addDepot(t, w, pt2(60, 100), 0)
	depleteEvents := 0
	w.RegisterHandler(31, func(w *World, e Event) {
		depleteEvents++
		t.Logf("t%d EvResourceDepleted node=%d res=%d (#%d)", w.Tick(), e.Src, e.Arg, depleteEvents)
	})
	w.Subscribe(EvResourceDepleted, 31)
	w.IssueOrder(wa, Order{Kind: OrderHarvest, Target: mine}, false)
	w.IssueOrder(wb, Order{Kind: OrderHarvest, Target: mine}, false)
	for i := 0; i < 120 && w.Ents.Alive(mine); i++ {
		w.Step()
	}
	t.Logf("FSV #300: node amount 10 drained by 2 shared gatherers → EvResourceDepleted fired %d time(s) (want 1); gold=%d", depleteEvents, w.Resources(0, 0))
	if depleteEvents != 1 {
		t.Fatalf("DUPLICATE-DEPLETED BUG: EvResourceDepleted fired %d times for one exhaustion (want 1) — a second same-tick gatherer with take=0 re-emitted it", depleteEvents)
	}
}

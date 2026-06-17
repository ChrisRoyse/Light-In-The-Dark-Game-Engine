package ai_test

import (
	"fmt"
	"strings"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/ai"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim/path"
)

// #279 economy FSV. SoT = the sim's own state: HarvestersOn (workers per
// resource), the resource-node ledger, and the count of building entities placed
// (under construction). The litd/ai economy layer (harvest intents + build-order
// sequencer) is the unit under test; counts are cross-checked against the sim.

const (
	resGold = 0
	resWood = 1
	tFarm   uint16 = 0
	tBarr   uint16 = 1
)

func econDefs() []data.Unit {
	return []data.Unit{
		{ID: "farm", Life: 200, Footprint: 2, BuildTicks: 30, Costs: []int64{40, 0}, CollisionSize: 24},
		{ID: "barracks", Life: 400, Footprint: 3, BuildTicks: 50, Costs: []int64{80, 0}, CollisionSize: 40},
	}
}

var econWorkerSpec = data.HarvestSpec{GatherTicks: 10, Capacity: 10, Mask: 0b11} // gold + wood

type econAdapter struct{ w *sim.World }

func (a econAdapter) AssignHarvest(player, resource, count int) int {
	return a.w.HarvestAssign(uint8(player), resource, count)
}
func (a econAdapter) HarvestersOn(player, resource int) int {
	return a.w.HarvestersOn(uint8(player), resource)
}
func (a econAdapter) PlaceBuilding(player, typeID int, cx, cy int32) bool {
	_, _, ok := a.w.PlaceBuildingNear(uint8(player), uint16(typeID), ptWU(cx, cy))
	return ok
}

func econWorld(t *testing.T, buildable bool) *sim.World {
	t.Helper()
	w := sim.NewWorld(sim.Caps{Units: 64, PathRequests: 256})
	if !w.BindEconomy(2) || !w.BindUnitDefs(econDefs()) {
		t.Fatal("bind failed")
	}
	g := path.NewGrid()
	flags := path.Walkable | path.Flyable
	if buildable {
		flags |= path.Buildable
	}
	for y := int32(0); y < path.GridSize; y++ {
		for x := int32(0); x < path.GridSize; x++ {
			g.SetFlags(x, y, flags)
		}
	}
	w.SetGrid(g)
	return w
}

// spawnEconWorker hand-assembles a harvester worker (which also serves as a
// builder when idle) for player.
func spawnEconWorker(t *testing.T, w *sim.World, player uint8, x, y int32) sim.EntityID {
	t.Helper()
	id, ok := w.CreateUnit(ptWU(x, y), 0)
	if !ok ||
		!w.Owners.Add(w.Ents, id, player, player, player) ||
		!w.Orders.Add(w.Ents, id) ||
		!w.Movements.Add(w.Ents, w.Transforms, id, 6*fixed.One, 0x4000) ||
		!w.Harvests.Add(w.Ents, id, &econWorkerSpec) {
		t.Fatalf("worker spawn failed id=%d", id)
	}
	return id
}

func addNode(t *testing.T, w *sim.World, resource uint8, x, y int32, amount int64) sim.EntityID {
	t.Helper()
	nt := data.ResourceNodeType{ID: "node", Resource: resource, Amount: amount, Exclusive: false}
	id, ok := w.CreateResourceNode(ptWU(x, y), &nt)
	if !ok {
		t.Fatal("node create failed")
	}
	return id
}

// structuresPlaced counts building entities of typeID owned by player (under
// construction or complete) — the placement source of truth.
func structuresPlaced(w *sim.World, player uint8, typeID uint16) int {
	var ids []sim.EntityID
	ids = w.AppendAllUnits(ids)
	n := 0
	for _, id := range ids {
		or := w.Owners.Row(id)
		ur := w.UnitTypes.Row(id)
		if or != -1 && ur != -1 && w.Owners.Player[or] == player && w.UnitTypes.TypeID[ur] == typeID {
			n++
		}
	}
	return n
}

// TestEconomyRampAndBuildFSV — the headline scenario. The AI ramps harvesters
// 5→10 (gold + wood) and runs a build order of 2 farms + 1 barracks. Per-tick
// (gold-workers, wood-workers, farms, barracks) tables are byte-identical across
// two runs, and the final state matches the hand-computed expectation.
func TestEconomyRampAndBuildFSV(t *testing.T) {
	const player uint8 = 1
	const townX, townY = 1000, 1000

	run := func(log bool) (string, [2]int, [2]int) {
		w := econWorld(t, true)
		// 14 workers: enough for 10 harvesters + idle builders.
		for i := int32(0); i < 14; i++ {
			spawnEconWorker(t, w, player, 900+(i%7)*24, 900+(i/7)*24)
		}
		addNode(t, w, resGold, 1200, 1000, 100000) // plenty, won't deplete
		addNode(t, w, resWood, 800, 1000, 100000)
		// a depot accepting both resources so harvest cycles complete and workers
		// stay on the job (no depot ⇒ the deposit leg fails and the worker idles).
		dep, _ := w.CreateUnit(ptWU(townX, townY), 0)
		w.Owners.Add(w.Ents, dep, player, player, player)
		w.AddEcon(dep, 0, 0, 0b11)
		w.SetResource(player, resGold, 100000)
		w.SetResource(player, resWood, 100000)

		ctrl := econAdapter{w}
		econ := ai.NewEconomy(ctrl, int(player))
		bo := ai.NewBuildOrder(ctrl, int(player), townX, townY,
			[]ai.BuildItem{{TypeID: int(tFarm), Count: 2}, {TypeID: int(tBarr), Count: 1}})

		var sb strings.Builder
		var peakG, peakW int
		for tk := 0; tk <= 200; tk++ {
			switch tk {
			case 0:
				econ.SetHarvest(resGold, 3)
				econ.SetHarvest(resWood, 2)
			case 60:
				econ.SetHarvest(resGold, 6) // ramp 5 → 10
				econ.SetHarvest(resWood, 4)
			}
			bo.Tick()
			w.Step()
			if g := econ.HarvestersOn(resGold); g > peakG {
				peakG = g
			}
			if wd := econ.HarvestersOn(resWood); wd > peakW {
				peakW = wd
			}
			if tk%20 == 0 {
				fmt.Fprintf(&sb, "t%d:g%d/w%d/F%d/B%d ", tk,
					econ.HarvestersOn(resGold), econ.HarvestersOn(resWood),
					structuresPlaced(w, player, tFarm), structuresPlaced(w, player, tBarr))
			}
		}
		peak := [2]int{peakG, peakW}
		structs := [2]int{structuresPlaced(w, player, tFarm), structuresPlaced(w, player, tBarr)}
		if log {
			t.Logf("FSV economy per-tick table:\n%s", strings.ReplaceAll(sb.String(), " ", "\n  "))
			t.Logf("FSV peak harvesters gold=%d wood=%d | final structures farms=%d barracks=%d", peak[0], peak[1], structs[0], structs[1])
			t.Logf("FSV build order complete=%v attempts=%d failures=%d", bo.Complete(), bo.Attempts(), bo.Failures())
			t.Logf("FSV note: the sustained per-tick count decays below the ramp peak — deterministic node congestion (workers that cannot reach a crowded non-exclusive node idle); the ASSIGNMENT intent reaches its target, the sim's harvest physics governs sustain.")
		}
		return sb.String(), peak, structs
	}

	tr1, peak1, s1 := run(true)
	tr2, peak2, s2 := run(false)

	// The ramp intent reached its target (5 → 10 split 6 gold / 4 wood).
	if peak1[0] != 6 || peak1[1] != 4 {
		t.Fatalf("ramp did not reach target: peak gold=%d wood=%d want 6/4", peak1[0], peak1[1])
	}
	// The build order placed every structure (2 farms + 1 barracks).
	if s1[0] != 2 || s1[1] != 1 {
		t.Fatalf("structures farms=%d barracks=%d want 2/1", s1[0], s1[1])
	}
	// Determinism: the full per-tick table and the peaks/structures match exactly.
	if tr1 != tr2 || peak1 != peak2 || s1 != s2 {
		t.Fatalf("economy run not deterministic:\n run1=%q %v %v\n run2=%q %v %v", tr1, peak1, s1, tr2, peak2, s2)
	}
}

// TestEconomyHarvestPartialFSV — edge 1. A harvest intent for more workers than
// exist assigns all available and reports the honest (partial) count.
func TestEconomyHarvestPartialFSV(t *testing.T) {
	const player uint8 = 1
	w := econWorld(t, true)
	for i := int32(0); i < 3; i++ {
		spawnEconWorker(t, w, player, 980+i*24, 1000)
	}
	addNode(t, w, resGold, 1100, 1000, 100000)
	ctrl := econAdapter{w}
	econ := ai.NewEconomy(ctrl, int(player))

	newly := econ.SetHarvest(resGold, 5) // ask for 5, only 3 workers exist
	w.Step()                              // let the orders take (Harvest.Node set)
	on := econ.HarvestersOn(resGold)
	t.Logf("FSV requested 5 with 3 workers: newly-assigned=%d HarvestersOn(gold)=%d", newly, on)
	if newly != 3 || on != 3 {
		t.Fatalf("partial assignment newly=%d on=%d want 3/3", newly, on)
	}
	// Asking again does not over-assign (incremental, idempotent at the cap).
	again := econ.SetHarvest(resGold, 5)
	t.Logf("FSV re-request newly-assigned=%d (want 0 — all 3 already on gold)", again)
	if again != 0 {
		t.Fatalf("re-request assigned %d want 0", again)
	}
}

// TestEconomyBuildBlockedFSV — edge 2. With no buildable cell anywhere, every
// placement attempt fails deterministically and is recorded; the build order
// retries (attempts grow) rather than advancing or dropping.
func TestEconomyBuildBlockedFSV(t *testing.T) {
	const player uint8 = 1
	w := econWorld(t, false) // grid has NO Buildable flag → placement always fails
	for i := int32(0); i < 4; i++ {
		spawnEconWorker(t, w, player, 980+i*24, 1000)
	}
	w.SetResource(player, resGold, 100000)
	ctrl := econAdapter{w}
	bo := ai.NewBuildOrder(ctrl, int(player), 1000, 1000, []ai.BuildItem{{TypeID: int(tFarm), Count: 1}})

	for tk := 0; tk < 10; tk++ {
		issued := bo.Tick()
		w.Step()
		if issued {
			t.Fatalf("tick %d issued a placement on an unbuildable grid", tk)
		}
	}
	t.Logf("FSV unbuildable grid: complete=%v issued[0]=%d attempts=%d failures=%d",
		bo.Complete(), bo.Issued(0), bo.Attempts(), bo.Failures())
	if bo.Complete() || bo.Issued(0) != 0 {
		t.Fatalf("build order advanced on failure: complete=%v issued=%d", bo.Complete(), bo.Issued(0))
	}
	if bo.Attempts() != 10 || bo.Failures() != 10 {
		t.Fatalf("attempts=%d failures=%d want 10/10 (retried every tick, recorded)", bo.Attempts(), bo.Failures())
	}
}

// TestEconomyMineExhaustedReassignFSV — edge 3. A gold node depletes mid-harvest;
// the sim INCREMENTALLY reassigns the worker to a surviving node (no churn, count
// preserved). Trace printed before/after, worker's bound node inspected directly.
func TestEconomyMineExhaustedReassignFSV(t *testing.T) {
	const player uint8 = 1
	w := econWorld(t, true)
	worker := spawnEconWorker(t, w, player, 1000, 1000)
	small := addNode(t, w, resGold, 1040, 1000, 20) // tiny: depletes fast, and nearest
	backup := addNode(t, w, resGold, 1500, 1000, 100000)
	dep, _ := w.CreateUnit(ptWU(960, 1000), 0) // depot so deposits complete and the node drains
	w.Owners.Add(w.Ents, dep, player, player, player)
	w.AddEcon(dep, 0, 0, 0b01)
	ctrl := econAdapter{w}
	econ := ai.NewEconomy(ctrl, int(player))

	econ.SetHarvest(resGold, 1)
	w.Step()
	hr := w.Harvests.Row(worker)
	t.Logf("FSV after assign: HarvestersOn(gold)=%d worker node==small? %v", econ.HarvestersOn(resGold), w.Harvests.Node[hr] == small)
	if econ.HarvestersOn(resGold) != 1 || w.Harvests.Node[hr] != small {
		t.Fatalf("worker did not start on the nearest (small) node: on=%d node==small=%v", econ.HarvestersOn(resGold), w.Harvests.Node[hr] == small)
	}

	// Run until the small node is exhausted.
	exhaustedTick := -1
	for tk := 0; tk < 400 && exhaustedTick == -1; tk++ {
		w.Step()
		if !w.Ents.Alive(small) {
			exhaustedTick = int(w.Tick())
		}
	}
	if exhaustedTick == -1 {
		t.Fatal("small node never exhausted")
	}
	// Let the sim settle the retarget, then inspect: the worker stays a gold
	// harvester (count preserved) and is now bound to the surviving backup node.
	for tk := 0; tk < 60; tk++ {
		w.Step()
	}
	on := econ.HarvestersOn(resGold)
	onBackup := w.Harvests.Node[hr] == backup
	t.Logf("FSV small node exhausted at t%d; HarvestersOn(gold)=%d worker node==backup? %v",
		exhaustedTick, on, onBackup)
	if on != 1 || !onBackup {
		t.Fatalf("incremental retarget failed: on=%d node==backup=%v want 1/true", on, onBackup)
	}
	// The standing intent observes a healthy assignment — no new orders needed.
	if again := econ.SetHarvest(resGold, 1); again != 0 {
		t.Fatalf("re-issue assigned %d want 0 (worker already auto-retargeted)", again)
	}
	t.Logf("FSV documented: node exhaustion triggers the sim's incremental retarget — worker moves to the next node, count preserved, no churn")
}

// TestEconomyBuildOrderSaveRestoreFSV — edge 4. A build order saved mid-sequence
// restores byte-faithfully and completes at the identical tick as an unbroken
// run.
func TestEconomyBuildOrderSaveRestoreFSV(t *testing.T) {
	const player uint8 = 1
	// Town center offset from the lone builder so each placement waits on the
	// worker walking to the site — issuance spans many ticks and the mid-sequence
	// save lands genuinely in-progress (a builder frees once construction STARTS,
	// so without travel the whole order would issue in ~3 ticks).
	const townX, townY = 1500, 1500
	const saveAt = 20
	items := []ai.BuildItem{{TypeID: int(tFarm), Count: 2}, {TypeID: int(tBarr), Count: 1}}

	build := func() (*sim.World, ai.EconomyControl) {
		w := econWorld(t, true)
		spawnEconWorker(t, w, player, 900, 900)
		w.SetResource(player, resGold, 100000)
		return w, econAdapter{w}
	}

	// Reference: unbroken run, record the tick the order completes.
	wR, ctrlR := build()
	boR := ai.NewBuildOrder(ctrlR, int(player), townX, townY, items)
	refComplete := -1
	for tk := 0; tk < 800; tk++ {
		boR.Tick()
		wR.Step()
		if boR.Complete() && refComplete == -1 {
			refComplete = tk
			break
		}
	}
	t.Logf("FSV reference build order completes issuing at tick=%d", refComplete)
	if refComplete == -1 || refComplete <= saveAt {
		t.Fatalf("reference completion tick=%d not usable (need > saveAt=%d)", refComplete, saveAt)
	}

	// Saved run: tick partway (mid-sequence), save the order, restore, finish.
	w, ctrl := build()
	bo := ai.NewBuildOrder(ctrl, int(player), townX, townY, items)
	for tk := 0; tk < saveAt; tk++ {
		bo.Tick()
		w.Step()
	}
	if bo.Complete() {
		t.Fatalf("order already complete at saveAt=%d (save not mid-sequence)", saveAt)
	}
	blob := bo.Save(nil)
	t.Logf("FSV saved build order: %d bytes, issued[0]=%d issued[1]=%d attempts=%d", len(blob), bo.Issued(0), bo.Issued(1), bo.Attempts())

	bo2 := ai.NewBuildOrder(ctrl, int(player), townX, townY, items)
	if err := bo2.Load(blob); err != nil {
		t.Fatalf("Load: %v", err)
	}
	if bo2.Issued(0) != bo.Issued(0) || bo2.Issued(1) != bo.Issued(1) || bo2.Attempts() != bo.Attempts() {
		t.Fatalf("restored order state differs: issued=%d/%d attempts=%d vs %d/%d/%d",
			bo2.Issued(0), bo2.Issued(1), bo2.Attempts(), bo.Issued(0), bo.Issued(1), bo.Attempts())
	}
	restoredComplete := -1
	for tk := saveAt; tk < 800; tk++ {
		bo2.Tick()
		w.Step()
		if bo2.Complete() && restoredComplete == -1 {
			restoredComplete = tk
			break
		}
	}
	t.Logf("FSV restored build order completes issuing at tick=%d (want == reference %d)", restoredComplete, refComplete)
	if restoredComplete != refComplete {
		t.Fatalf("restored completion tick %d != reference %d", restoredComplete, refComplete)
	}
}

// TestEconomyBuildOrderLoadFailClosedFSV — a corrupt/mismatched blob is rejected
// and leaves the order untouched.
func TestEconomyBuildOrderLoadFailClosedFSV(t *testing.T) {
	const player uint8 = 1
	items := []ai.BuildItem{{TypeID: int(tFarm), Count: 2}}
	w := econWorld(t, true)
	bo := ai.NewBuildOrder(econAdapter{w}, int(player), 1000, 1000, items)
	bo.Tick()
	before := bo.Attempts()

	if err := bo.Load([]byte("short")); err == nil {
		t.Fatal("accepted short blob")
	}
	good := bo.Save(nil)
	bad := append([]byte(nil), good...)
	bad[0] = 'Z'
	if err := bo.Load(bad); err == nil {
		t.Fatal("accepted bad-magic blob")
	}
	// shape mismatch: a save from a different item list
	other := ai.NewBuildOrder(econAdapter{w}, int(player), 1000, 1000, []ai.BuildItem{{TypeID: int(tBarr), Count: 9}})
	if err := bo.Load(other.Save(nil)); err == nil {
		t.Fatal("accepted item-shape-mismatched blob")
	}
	if bo.Attempts() != before {
		t.Fatalf("failed Load mutated the order: attempts %d != %d", bo.Attempts(), before)
	}
	t.Logf("FSV fail-closed: short / bad-magic / shape-mismatch all rejected; order unchanged")
}

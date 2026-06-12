package sim

// #301 construction FSV. SoT = building Healths.Life (HP ramp), the
// pathing-grid OccupiedStatic flags (footprint stamp), the player
// resource counters (cost/refund), the BuildStore rows, and the
// construct event log; plus twin-hash + save v9 round-trip.

import (
	"bytes"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim/path"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/statehash"
)

const (
	tcWorker uint16 = 0
	tcTower  uint16 = 1
)

func conDefs() []data.Unit {
	return []data.Unit{
		{ID: "worker", Life: 80, MoveSpeedPerTick: 8 * fixed.One, TurnRatePerTick: 65535, CollisionSize: 16},
		// tower: 2×2 footprint, 20-tick build, 100 gold, 75% refund, 1000 HP.
		{ID: "tower", Life: 1000, CollisionSize: 64, Costs: []int64{100},
			Footprint: 2, BuildTicks: 20, RefundPermille: 750},
	}
}

// conGrid bakes a Walkable|Buildable block over cells [18,46)².
func conGrid() *path.Grid {
	g := path.NewGrid()
	for y := int32(18); y < 46; y++ {
		for x := int32(18); x < 46; x++ {
			g.SetFlags(x, y, path.Walkable|path.Buildable)
		}
	}
	return g
}

func conWorld(t *testing.T) *World {
	t.Helper()
	w := NewWorld(Caps{Units: 64})
	if !w.BindEconomy(1) || !w.BindUnitDefs(conDefs()) {
		t.Fatal("bind failed")
	}
	w.SetGrid(conGrid())
	w.resources[0][0] = 500
	return w
}

// conWorker spawns a player-0 worker at pos (mover, can take orders).
func conWorker(t *testing.T, w *World, pos fixed.Vec2) EntityID {
	t.Helper()
	id, ok := w.SpawnFromTable(tcWorker, 0, 0, pos)
	if !ok {
		t.Fatal("worker spawn")
	}
	return id
}

func bhp(w *World, b EntityID) int64 {
	hr := w.Healths.Row(b)
	if hr == -1 {
		return -1
	}
	return w.Healths.Life[hr].Floor()
}

func cellStamped(w *World, cx, cy int32) bool {
	return w.Grid.FlagsAt(cx, cy)&path.OccupiedStatic != 0
}

// Edge 5 + happy path: a worker builds a tower; cost deducts at start,
// footprint stamps, HP ramps 10%→100% on the table curve.
func TestConstructHappyPathHPRamp(t *testing.T) {
	w := conWorld(t)
	worker := conWorker(t, w, pt2(1000, 950))
	site := pt2(1000, 1000)
	var started, finished []EntityID
	w.RegisterHandler(hA, func(_ *World, e Event) { started = append(started, e.Dst) })
	w.RegisterHandler(hB, func(_ *World, e Event) { finished = append(finished, e.Dst) })
	w.Subscribe(EvConstructStarted, hA)
	w.Subscribe(EvConstructFinished, hB)

	if !w.IssueBuild(worker, tcTower, site) {
		t.Fatal("IssueBuild refused a valid placement")
	}
	w.Step() // arrive (already adjacent) → start; constructionSystem ticks to progress 1
	if len(started) != 1 {
		t.Fatalf("EvConstructStarted count = %d", len(started))
	}
	b := started[0]
	br := w.Build.Row(b)
	if br == -1 {
		t.Fatal("no build row for the started structure")
	}
	if w.Resources(0, 0) != 400 {
		t.Fatalf("cost not deducted at start: gold=%d (want 400)", w.Resources(0, 0))
	}
	// footprint cells: centre cell (31,31) → origin (30,30), 2×2.
	for _, c := range [][2]int32{{30, 30}, {31, 30}, {30, 31}, {31, 31}} {
		if !cellStamped(w, c[0], c[1]) {
			t.Fatalf("footprint cell (%d,%d) not stamped", c[0], c[1])
		}
	}
	t.Logf("t%d started: building=%d gold=%d progress=%d HP=%d/%d", w.Tick(), b.Index(),
		w.Resources(0, 0), w.Build.Progress[br], bhp(w, b), w.Healths.MaxLife[w.Healths.Row(b)].Floor())

	// drive to completion, sampling HP at progress 5 / 10 / 20.
	sampled := map[uint16]int64{}
	for i := 0; i < 30 && len(finished) == 0; i++ {
		p := w.Build.Progress[br]
		if p == 5 || p == 10 {
			sampled[p] = bhp(w, b)
		}
		w.Step()
	}
	sampled[20] = bhp(w, b)
	// curve: HP = Life * (100 + 900*p/20)/1000, Life=1000 → permille value.
	t.Logf("HP ramp: p5=%d (want 325) p10=%d (want 550) p20=%d (want 1000); finished=%d",
		sampled[5], sampled[10], sampled[20], len(finished))
	if sampled[5] != 325 || sampled[10] != 550 || sampled[20] != 1000 {
		t.Fatalf("HP ramp off the curve: p5=%d p10=%d p20=%d (want 325/550/1000)", sampled[5], sampled[10], sampled[20])
	}
	if len(finished) != 1 || finished[0] != b {
		t.Fatalf("EvConstructFinished wrong: %v", finished)
	}
	if w.Build.Builder[br] != 0 {
		t.Fatal("builder not released on completion")
	}
	if w.IsUnderConstruction(b) {
		t.Fatal("finished building still reports under-construction")
	}
}

// Edge 1: placement on an occupied cell refuses deterministically — no
// cost, no order, no building, grid untouched.
func TestConstructPlacementRefusedOccupied(t *testing.T) {
	w := conWorld(t)
	worker := conWorker(t, w, pt2(1000, 950))
	site := pt2(1000, 1000)
	// occupy one footprint cell up front.
	w.Grid.OrFlags(31, 31, path.OccupiedStatic)

	goldBefore := w.Resources(0, 0)
	buildsBefore := w.Build.Count()
	ok := w.IssueBuild(worker, tcTower, site)
	t.Logf("IssueBuild on occupied site -> %v; gold %d->%d builds %d->%d order=%d",
		ok, goldBefore, w.Resources(0, 0), buildsBefore, w.Build.Count(), w.Orders.Kind[w.Orders.Row(worker)])
	if ok {
		t.Fatal("placement on an occupied cell must refuse")
	}
	if w.Resources(0, 0) != goldBefore {
		t.Fatalf("refused placement deducted cost: %d", w.Resources(0, 0))
	}
	if w.Build.Count() != buildsBefore {
		t.Fatal("refused placement spawned a structure")
	}
	if w.Orders.Kind[w.Orders.Row(worker)] == OrderBuild {
		t.Fatal("refused placement still queued a build order")
	}
}

// Edge 2: cancel at ~50% refunds the per-mille, unstamps the grid, and
// destroys the unfinished building.
func TestConstructCancelRefund(t *testing.T) {
	w := conWorld(t)
	worker := conWorker(t, w, pt2(1000, 950))
	site := pt2(1000, 1000)
	var started []EntityID
	w.RegisterHandler(hA, func(_ *World, e Event) { started = append(started, e.Dst) })
	w.Subscribe(EvConstructStarted, hA)
	w.IssueBuild(worker, tcTower, site)
	w.Step()
	b := started[0]
	br := w.Build.Row(b)
	for w.Build.Progress[br] < 10 { // ~50%
		w.Step()
	}
	goldMid := w.Resources(0, 0) // 400 after the 100 spend
	t.Logf("at progress %d: gold=%d HP=%d", w.Build.Progress[br], goldMid, bhp(w, b))
	if !w.CancelConstruction(b) {
		t.Fatal("cancel of a rising building refused")
	}
	// refund = 100 * 750/1000 = 75 → 400 + 75 = 475.
	if w.Resources(0, 0) != 475 {
		t.Fatalf("refund wrong: gold=%d (want 475 = 400 + 75)", w.Resources(0, 0))
	}
	w.Step() // cleanup destroys the building (deferred kill)
	t.Logf("after cancel+cleanup: gold=%d buildingAlive=%v cells(31,31)stamped=%v builds=%d",
		w.Resources(0, 0), w.Ents.Alive(b), cellStamped(w, 31, 31), w.Build.Count())
	if w.Ents.Alive(b) {
		t.Fatal("cancelled building not destroyed")
	}
	for _, c := range [][2]int32{{30, 30}, {31, 30}, {30, 31}, {31, 31}} {
		if cellStamped(w, c[0], c[1]) {
			t.Fatalf("cancel left cell (%d,%d) stamped", c[0], c[1])
		}
	}
	if w.Build.Count() != 0 {
		t.Fatal("cancel left a build row")
	}
}

// Edge 3: a building destroyed mid-construction releases the worker and
// unstamps the grid — with NO refund.
func TestConstructDestroyedMidConstruction(t *testing.T) {
	w := conWorld(t)
	worker := conWorker(t, w, pt2(1000, 950))
	site := pt2(1000, 1000)
	var started []EntityID
	w.RegisterHandler(hA, func(_ *World, e Event) { started = append(started, e.Dst) })
	w.Subscribe(EvConstructStarted, hA)
	w.IssueBuild(worker, tcTower, site)
	w.Step()
	b := started[0]
	br := w.Build.Row(b)
	for w.Build.Progress[br] < 6 {
		w.Step()
	}
	goldBefore := w.Resources(0, 0) // 400 (cost spent, no refund coming)
	w.KillUnit(b)
	w.Step() // cleanup destroys → destroyBuild unstamps + drops row
	t.Logf("after destroy: gold %d->%d (no refund), buildingAlive=%v cell(31,31)=%v builds=%d workerAlive=%v",
		goldBefore, w.Resources(0, 0), w.Ents.Alive(b), cellStamped(w, 31, 31), w.Build.Count(), w.Ents.Alive(worker))
	if w.Resources(0, 0) != goldBefore {
		t.Fatalf("destruction refunded: gold=%d want %d", w.Resources(0, 0), goldBefore)
	}
	if cellStamped(w, 31, 31) {
		t.Fatal("destroyed building left its footprint stamped")
	}
	if w.Build.Count() != 0 {
		t.Fatal("destroyed building left a build row")
	}
	if !w.Ents.Alive(worker) {
		t.Fatal("worker died with the building")
	}
	// worker is free: it accepts a fresh order.
	if !w.IssueOrder(worker, Order{Kind: OrderMove, Point: pt2(800, 800)}, false) {
		t.Fatal("released worker rejected a new order")
	}
}

// Edge 4: two workers ordered onto the same site — the first to start
// stamps; the second fails deterministically. Cost is paid once.
func TestConstructRaceSameSite(t *testing.T) {
	w := conWorld(t)
	w1 := conWorker(t, w, pt2(1000, 950))
	w2 := conWorker(t, w, pt2(1000, 1050))
	site := pt2(1000, 1000)
	var started []EntityID
	var done []int64 // EvOrderDone args
	w.RegisterHandler(hA, func(_ *World, e Event) { started = append(started, e.Dst) })
	w.RegisterHandler(hB, func(_ *World, e Event) { done = append(done, e.Arg) })
	w.Subscribe(EvConstructStarted, hA)
	w.Subscribe(EvOrderDone, hB)
	// both validate at issue (site free for both) and queue OrderBuild.
	if !w.IssueBuild(w1, tcTower, site) || !w.IssueBuild(w2, tcTower, site) {
		t.Fatal("both issues should succeed at issue time")
	}
	w.Step() // ordersSystem resolves both this tick, in row order
	t.Logf("after 1 tick: started=%d builds=%d gold=%d orderDone-args=%v",
		len(started), w.Build.Count(), w.Resources(0, 0), done)
	if len(started) != 1 {
		t.Fatalf("race produced %d structures, want exactly 1", len(started))
	}
	if w.Resources(0, 0) != 400 {
		t.Fatalf("cost paid more than once: gold=%d (want 400)", w.Resources(0, 0))
	}
	// one OrderDone failed (Arg 0) for the loser.
	failed := 0
	for _, a := range done {
		if a == 0 {
			failed++
		}
	}
	if failed < 1 {
		t.Fatalf("loser did not fail deterministically: done args %v", done)
	}
}

// Determinism twin + save v9: a mid-construction world twins and
// round-trips byte-identically (including the re-stamped footprint),
// then resumes the same.
func TestConstructDeterminismAndSave(t *testing.T) {
	build := func() *World {
		w := conWorld(t)
		worker := conWorker(t, w, pt2(1000, 950))
		w.IssueBuild(worker, tcTower, pt2(1000, 1000))
		for i := 0; i < 8; i++ { // mid-construction (progress ~8)
			w.Step()
		}
		return w
	}
	a, b := build(), build()
	var sa, sb statehash.Snapshot
	a.HashState(NewHashRegistry(), &sa)
	b.HashState(NewHashRegistry(), &sb)
	t.Logf("twin A=%016x B=%016x buildRows=%d", sa.Top, sb.Top, a.Build.Count())
	if sa.Top != sb.Top {
		t.Fatal("twin divergence")
	}
	if a.Build.Count() != 1 {
		t.Fatalf("expected one rising structure, got %d", a.Build.Count())
	}
	// progress is real state
	a.Build.Progress[0]++
	var sa2 statehash.Snapshot
	a.HashState(NewHashRegistry(), &sa2)
	if sa2.Top == sa.Top {
		t.Fatal("construction progress invisible to the hash")
	}
	a.Build.Progress[0]--

	var buf bytes.Buffer
	if err := a.SaveState(&buf, 3); err != nil {
		t.Fatal(err)
	}
	w2 := conWorld(t) // fresh grid bake (footprint re-stamped on load)
	if err := w2.LoadState(bytes.NewReader(buf.Bytes()), 3); err != nil {
		t.Fatal(err)
	}
	var sl statehash.Snapshot
	w2.HashState(NewHashRegistry(), &sl)
	t.Logf("loaded=%016x (orig %016x) cell(31,31)stamped=%v", sl.Top, sa.Top, cellStamped(w2, 31, 31))
	if sl.Top != sa.Top {
		t.Fatal("save v9 load diverged")
	}
	if !cellStamped(w2, 31, 31) {
		t.Fatal("footprint not re-stamped on load")
	}
	for i := 0; i < 20; i++ { // resume identically through completion
		a.Step()
		w2.Step()
	}
	a.HashState(NewHashRegistry(), &sa)
	w2.HashState(NewHashRegistry(), &sl)
	if sa.Top != sl.Top {
		t.Fatal("post-load resume diverged")
	}
}

func TestConstructTickAllocs(t *testing.T) {
	w := conWorld(t)
	worker := conWorker(t, w, pt2(1000, 950))
	w.IssueBuild(worker, tcTower, pt2(1000, 1000))
	w.Step() // start
	allocs := testing.AllocsPerRun(50, func() { w.Step() })
	t.Logf("allocs/op advancing construction: %v", allocs)
	if allocs != 0 {
		t.Fatalf("construction tick allocates: %v", allocs)
	}
}

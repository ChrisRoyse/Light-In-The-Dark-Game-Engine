// Package bench holds the headless CI benchmark scenarios (#202,
// ecs-architecture.md §8, milestones.md M3 exits 1+7).
//
// bench_battle_500: 500 units + ~500 live missiles in sustained
// combat — worst-case tick ≤ 10 ms is the CI gate. bench_battle_1000
// runs alongside, results tracked not gated. Per-phase wall time is
// measured through the World.PhaseTrace hook and reported against the
// provisional tick budget split (tick-and-scheduler.md §1):
//
//	input+scripts 1.0 / orders+abilities 1.0 / pathing 2.0 /
//	movement 2.0 / combat+missiles 2.0 / events+cleanup 1.5 /
//	reserve 0.5 ms
//
// Wall clock is measured here and ONLY here — gameplay never reads
// it. The pathing scenes (mass re-path on stamp, 1,000-unit
// shared-goal blob) still stress litd/sim/path directly; World
// services Queue/FlowSet at the head of phase 4 before movement.
package bench

import (
	"os"
	"path/filepath"
	"sort"
	"strings"
	"testing"
	"time"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/obs"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim/path"
)

// tickBudget is the #202 CI gate: no tick may exceed this.
const tickBudget = 10 * time.Millisecond

// warmupTicks runs every scenario to combat steady state (targets
// acquired, missile population saturated) before any measurement.
const warmupTicks = 300

// buildBattle spawns n units in paired firing lines: opposing
// ranged units 30 world units apart with 32-wu weapons, so every
// unit fires from its spawn cell without moving. Projectile speed
// 1 wu/tick over the 30-wu gap makes flight time ≈ cooldown, holding
// the live missile population at ≈ n. MaxLife is set high enough
// that nothing dies inside a 10,000-tick run — sustained combat, no
// population decay.
func buildBattle(tb testing.TB, n int) *sim.World {
	tb.Helper()
	w := sim.NewWorld(sim.Caps{})
	w.SetSeed(0xBA771E)
	if err := w.BindDamageMatrix([][]int32{{1000}}); err != nil {
		tb.Fatal(err)
	}
	weapon := data.Attack{
		AttackType:             0,
		Range:                  fixed.FromInt(32),
		DamageBase:             1,
		Dice:                   1,
		Sides:                  2,
		CooldownTicks:          30,
		DamagePointTicks:       5,
		BackswingTicks:         5,
		ProjectileSpeedPerTick: fixed.One, // 1 wu/tick -> ~30-tick flight
	}
	const rowsPerCol = 100
	pairs := n / 2
	for p := 0; p < pairs; p++ {
		col, row := int32(p/rowsPerCol), int32(p%rowsPerCol)
		y := fixed.FromInt(20 + row*4)
		for side := 0; side < 2; side++ {
			x := fixed.FromInt(20 + col*100 + int32(side)*30)
			id, ok := w.CreateUnit(fixed.Vec2{X: x, Y: y}, 0)
			if !ok {
				tb.Fatalf("unit cap reached at pair %d", p)
			}
			team := uint8(side)
			if !w.Owners.Add(w.Ents, id, team, team, team) ||
				!w.Healths.Add(w.Ents, id, 1_000_000*fixed.One, 0, 0, 0) ||
				!w.Combats.Add(w.Ents, id) ||
				!w.Orders.Add(w.Ents, id) ||
				!w.Movements.Add(w.Ents, w.Transforms, id, fixed.One*7/2, 2048) {
				tb.Fatalf("component add failed for pair %d side %d", p, side)
			}
			if !w.SetWeapon(id, 0, &weapon, 0, data.EffectList{}) {
				tb.Fatalf("weapon set failed for pair %d side %d", p, side)
			}
			w.Combats.AcquisitionRange[w.Combats.Row(id)] = fixed.FromInt(40)
		}
	}
	return w
}

// phaseNames index the per-phase accumulators (1-based like Step).
var phaseNames = [8]string{"", "input", "scripts", "orders", "movement", "combat", "events", "cleanup"}

// phaseTimer turns the PhaseTrace hook (fires at each phase START)
// into per-phase durations: phase i = trace(i+1) − trace(i), phase 7
// closes when the caller stamps stepEnd after Step returns.
type phaseTimer struct {
	last  time.Time
	cur   [8]time.Duration
	total [8]time.Duration
	worst [8]time.Duration
}

func (pt *phaseTimer) attach(w *sim.World) {
	w.PhaseTrace = func(_ uint32, phase int, _ string) {
		now := time.Now()
		if phase > 1 {
			pt.cur[phase-1] = now.Sub(pt.last)
		}
		pt.last = now
	}
}

// stepEnd closes phase 7 and folds the tick into the accumulators.
func (pt *phaseTimer) stepEnd() {
	pt.cur[7] = time.Since(pt.last)
	for i := 1; i <= 7; i++ {
		pt.total[i] += pt.cur[i]
		if pt.cur[i] > pt.worst[i] {
			pt.worst[i] = pt.cur[i]
		}
	}
}

// battleStats is one measured run.
type battleStats struct {
	ticks          int
	worst, median  time.Duration
	timer          phaseTimer
	missileHigh    int32
	eventsDropped  uint64
	worstTickIndex int
	counters       *obs.Counters
	std            obs.StandardCounters
}

func newBattleCounterSet() (*obs.Counters, obs.StandardCounters) {
	c := obs.NewDefaultCounters()
	return c, obs.RegisterStandardCounters(c)
}

// stepBattle is the measured loop: steps the world once per durs
// entry. All scratch is caller-allocated so a benchmark can keep the
// loop alone inside its timed window.
func stepBattle(w *sim.World, durs []time.Duration, st *battleStats) {
	st.timer.attach(w)
	defer func() { w.PhaseTrace = nil }()
	for i := range durs {
		start := time.Now()
		w.Step()
		st.timer.stepEnd()
		durs[i] = time.Since(start)
		st.sampleCounters(w, durs[i])
		if c := w.ProjRender.Count(); c > st.missileHigh {
			st.missileHigh = c
		}
	}
}

func (st *battleStats) sampleCounters(w *sim.World, tickDur time.Duration) {
	if st.counters == nil {
		return
	}
	c := st.counters
	std := st.std
	c.Set(std.SimTickNS, tickDur.Nanoseconds())
	c.Set(std.SimPhaseInputNS, st.timer.cur[1].Nanoseconds())
	c.Set(std.SimPhaseScriptsNS, st.timer.cur[2].Nanoseconds())
	c.Set(std.SimPhaseOrdersNS, st.timer.cur[3].Nanoseconds())
	c.Set(std.SimPhaseMovementNS, st.timer.cur[4].Nanoseconds())
	c.Set(std.SimPhaseCombatNS, st.timer.cur[5].Nanoseconds())
	c.Set(std.SimPhaseEventsNS, st.timer.cur[6].Nanoseconds())
	c.Set(std.SimPhaseCleanupNS, st.timer.cur[7].Nanoseconds())
	c.Set(std.SimPathExpansionsTick, int64(w.PathExpansionsLastTick()))
	c.Set(std.SimPathQueueDepth, int64(w.PathQueueDepth()))
	c.Set(std.SimEntitiesUnitsActive, int64(w.UnitCount()))
	c.Set(std.SimEntitiesMissiles, int64(w.ProjRender.Count()))
	c.Set(std.SimEntitiesBuffs, int64(w.Buffs.Live()))
	c.Sample(w.Tick(), 0)
}

// summarize folds the per-tick durations (sorts durs in place).
func (st *battleStats) summarize(w *sim.World, durs []time.Duration) {
	st.ticks = len(durs)
	for i, d := range durs {
		if d > st.worst {
			st.worst, st.worstTickIndex = d, i
		}
	}
	sort.Slice(durs, func(i, j int) bool { return durs[i] < durs[j] })
	st.median = durs[len(durs)/2]
	st.eventsDropped = w.EventsDropped()
}

// runBattle steps a warmed world `ticks` times with full
// instrumentation and returns the measured run.
func runBattle(w *sim.World, ticks int) battleStats {
	return runBattleWithCounters(w, ticks, nil, obs.StandardCounters{})
}

func runBattleWithCounters(w *sim.World, ticks int, counters *obs.Counters, std obs.StandardCounters) battleStats {
	var st battleStats
	st.counters = counters
	st.std = std
	durs := make([]time.Duration, ticks)
	stepBattle(w, durs, &st)
	st.summarize(w, durs)
	return st
}

func ms(d time.Duration) float64 { return float64(d.Nanoseconds()) / 1e6 }

func abs64(v int64) int64 {
	if v < 0 {
		return -v
	}
	return v
}

func battleCounterExcerpt(s string, n int) string {
	lines := strings.Split(strings.TrimRight(s, "\n"), "\n")
	if len(lines) < n {
		n = len(lines)
	}
	return strings.Join(lines[:n], "\n")
}

// provisionalBudget maps the actual 7 phases onto the §1 split.
// Abilities execute inside phase 5, missiles inside phase 4, and
// pathing is serviced at the head of phase 4 in the shipped tick
// (step.go), so those budget lines merge here.
func logPhaseTable(tb testing.TB, st *battleStats) {
	tb.Helper()
	n := time.Duration(st.ticks)
	tb.Logf("%-22s %12s %12s %12s", "phase", "mean ms", "worst ms", "budget ms")
	line := func(label string, idx []int, budget float64) {
		var tot, worst time.Duration
		for _, i := range idx {
			tot += st.timer.total[i]
			worst += st.timer.worst[i]
		}
		tb.Logf("%-22s %12.4f %12.4f %12.1f", label, ms(tot/n), ms(worst), budget)
	}
	line("input+scripts", []int{1, 2}, 1.0)
	line("orders (+abilities p5)", []int{3}, 1.0)
	line("pathing+movement (+missiles)", []int{4}, 4.0)
	line("combat", []int{5}, 2.0)
	line("events+cleanup", []int{6, 7}, 1.5)
	for i := 1; i <= 7; i++ {
		tb.Logf("  raw phase %d %-9s mean=%.4f worst=%.4f ms",
			i, phaseNames[i], ms(st.timer.total[i]/n), ms(st.timer.worst[i]))
	}
}

func benchBattle(b *testing.B, units int) {
	w := buildBattle(b, units)
	for i := 0; i < warmupTicks; i++ {
		w.Step()
	}
	if w.ProjRender.Count() == 0 {
		b.Fatal("degenerate scene: no missiles in flight after warmup")
	}
	counters, std := newBattleCounterSet()
	var st battleStats
	st.counters = counters
	st.std = std
	durs := make([]time.Duration, b.N)
	b.ResetTimer()
	stepBattle(w, durs, &st)
	b.StopTimer()
	st.summarize(w, durs)
	if p := os.Getenv("LITD_OBS_COUNTER_EXPORT"); p != "" {
		if err := os.MkdirAll(filepath.Dir(p), 0o755); err != nil {
			b.Fatal(err)
		}
		if err := counters.ExportBenchFile(p); err != nil {
			b.Fatal(err)
		}
		b.Logf("counter history exported: %s samples=%d retained=%d", p, counters.TotalSamples(), counters.Len())
	}
	b.ReportMetric(ms(st.worst), "worst-ms/tick")
	b.ReportMetric(ms(st.median), "median-ms/tick")
	b.ReportMetric(float64(st.missileHigh), "missiles-high")
	logPhaseTable(b, &st)
	b.Logf("units=%d ticks=%d worst=%.4f ms median=%.4f ms missiles-high=%d dropped=%d",
		units, st.ticks, ms(st.worst), ms(st.median), st.missileHigh, st.eventsDropped)
}

// BenchmarkBattle500 is the gate scenario (≤10 ms worst tick).
func BenchmarkBattle500(b *testing.B) { benchBattle(b, 500) }

// BenchmarkBattle1000 is tracked, not gated (recommended spec).
func BenchmarkBattle1000(b *testing.B) { benchBattle(b, 1000) }

func TestBattle500CounterExportFSV(t *testing.T) {
	if testing.Short() {
		t.Skip("500-unit counter FSV skipped in -short")
	}
	w := buildBattle(t, 500)
	for i := 0; i < warmupTicks; i++ {
		w.Step()
	}
	counters, std := newBattleCounterSet()
	st := runBattleWithCounters(w, 64, counters, std)
	path := filepath.Join(t.TempDir(), "bench_battle_500.counters")
	if err := counters.ExportBenchFile(path); err != nil {
		t.Fatal(err)
	}
	raw, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	text := string(raw)

	last := counters.Len() - 1
	tickNS, _ := counters.HistoryValue(last, std.SimTickNS)
	unitCount, _ := counters.HistoryValue(last, std.SimEntitiesUnitsActive)
	missileCount, _ := counters.HistoryValue(last, std.SimEntitiesMissiles)
	phaseSum := int64(0)
	for _, id := range []obs.CounterID{
		std.SimPhaseInputNS, std.SimPhaseScriptsNS, std.SimPhaseOrdersNS,
		std.SimPhaseMovementNS, std.SimPhaseCombatNS, std.SimPhaseEventsNS,
		std.SimPhaseCleanupNS,
	} {
		v, _ := counters.HistoryValue(last, id)
		phaseSum += v
	}
	diff := abs64(tickNS - phaseSum)
	t.Logf("SoT %s excerpt:\n%s", path, battleCounterExcerpt(text, 18))
	t.Logf("cross-check: samples=%d tickNS=%d phaseSumNS=%d diffNS=%d units=%d missiles=%d worst=%.4fms median=%.4fms",
		counters.Len(), tickNS, phaseSum, diff, unitCount, missileCount, ms(st.worst), ms(st.median))
	if diff > int64(5*time.Millisecond) {
		t.Fatalf("phase sum and tick total diverged: tick=%d phaseSum=%d diff=%d", tickNS, phaseSum, diff)
	}
	if unitCount != 500 {
		t.Fatalf("unit counter=%d want 500", unitCount)
	}
	if missileCount == 0 {
		t.Fatalf("missile counter stayed zero in sustained battle")
	}
	for _, want := range []string{
		"BenchmarkLITDPerf/sim.tick/",
		"BenchmarkLITDPerf/sim.phase.movement/",
		"BenchmarkLITDPerf/sim.entities.units.active/",
	} {
		if !strings.Contains(text, want) {
			t.Fatalf("export missing %q", want)
		}
	}
}

// TestBattle500TickBudget is the local preflight gate + #202 edge 3:
// a sustained 5,000-tick run where no tick may exceed 10 ms, no pool
// exhausts, and the missile population stays saturated. Because this
// is a wall-clock gate, normal package-parallel `go test ./...` skips
// it; scripts/preflight.sh runs it in isolation with LITD_TICK_GATE=on.
// LITD_TICK_GATE=off still runs the measurement but demotes the
// budget assert to a log line for loaded dev machines.
func TestBattle500TickBudget(t *testing.T) {
	if testing.Short() {
		t.Skip("5,000-tick sustained run skipped in -short")
	}
	gateMode := os.Getenv("LITD_TICK_GATE")
	if gateMode == "" {
		t.Skip("wall-clock tick gate runs only when LITD_TICK_GATE is set; scripts/preflight.sh runs it isolated with LITD_TICK_GATE=on")
	}
	w := buildBattle(t, 500)
	for i := 0; i < warmupTicks; i++ {
		w.Step()
	}
	st := runBattle(w, 5000)
	logPhaseTable(t, &st)
	t.Logf("sustained 5000 ticks: worst=%.4f ms (tick %d) median=%.4f ms", ms(st.worst), st.worstTickIndex, ms(st.median))
	t.Logf("pool high-water: missiles=%d/%d units=%d eventsDropped=%d",
		st.missileHigh, sim.EngineCaps.Projectiles, w.UnitCount(), st.eventsDropped)
	if st.missileHigh < 400 {
		t.Errorf("scene degenerated: missile high-water %d, want ≥400 (sustained 500+500 combat)", st.missileHigh)
	}
	if st.eventsDropped != 0 {
		t.Errorf("event ring overflowed %d times", st.eventsDropped)
	}
	if st.worst > tickBudget {
		msg := "worst tick " + st.worst.String() + " exceeds the " + tickBudget.String() + " gate"
		if gateMode == "off" {
			t.Log("GATE (demoted by LITD_TICK_GATE=off): " + msg)
		} else {
			t.Error(msg)
		}
	}
}

// TestBattle1000Capacities proves the recommended-spec scene fits the
// engine pools with zero growth: backing-array pointers are identical
// before and after 2,000 ticks of 1,000-unit + ~1,000-missile combat.
func TestBattle1000Capacities(t *testing.T) {
	if testing.Short() {
		t.Skip("2,000-tick capacity run skipped in -short")
	}
	w := buildBattle(t, 1000)
	for i := 0; i < warmupTicks; i++ {
		w.Step()
	}
	missileCol := &w.Missiles.Entity[0]
	posCol := &w.Transforms.Pos[0]
	healthCol := &w.Healths.Entity[0]
	st := runBattle(w, 2000)
	t.Logf("1000+1000 tracked: worst=%.4f ms median=%.4f ms missiles-high=%d/%d",
		ms(st.worst), ms(st.median), st.missileHigh, sim.EngineCaps.Projectiles)
	if st.missileHigh < 800 {
		t.Errorf("scene degenerated: missile high-water %d, want ≥800", st.missileHigh)
	}
	if int(st.missileHigh) > sim.EngineCaps.Projectiles {
		t.Errorf("missile high-water %d exceeds pool cap %d", st.missileHigh, sim.EngineCaps.Projectiles)
	}
	if missileCol != &w.Missiles.Entity[0] || posCol != &w.Transforms.Pos[0] || healthCol != &w.Healths.Entity[0] {
		t.Error("backing array reallocated mid-scene (R-GC-1 violation)")
	}
	t.Logf("backing pointers identical pre/post: missiles=%p transforms=%p healths=%p",
		missileCol, posCol, healthCol)
}

// TestBattleAllocsZero: R-GC-1 — zero heap allocations per tick at
// combat steady state, both scene sizes.
func TestBattleAllocsZero(t *testing.T) {
	for _, n := range []int{500, 1000} {
		w := buildBattle(t, n)
		for i := 0; i < warmupTicks; i++ {
			w.Step()
		}
		avg := testing.AllocsPerRun(200, func() { w.Step() })
		if w.ProjRender.Count() == 0 {
			t.Fatalf("n=%d: degenerate scene, no missiles in flight", n)
		}
		t.Logf("n=%d allocs/tick=%v missiles=%d", n, avg, w.ProjRender.Count())
		if avg != 0 {
			t.Errorf("n=%d: allocs/tick = %v, want 0", n, avg)
		}
	}
}

// ---- pathing scenes (litd/sim/path directly — see package doc) ----

// corridorGrid: fully open walkable+buildable board except a wall at
// x=256 pierced by one corridor (y 240..271) every cross-map path
// must thread.
func corridorGrid() (*path.Grid, *path.DilatedSet, *path.HPA) {
	g := path.NewGrid()
	for y := int32(0); y < path.GridSize; y++ {
		for x := int32(0); x < path.GridSize; x++ {
			if x == 256 && (y < 240 || y > 271) {
				continue // the wall
			}
			g.OrFlags(x, y, path.Walkable|path.Buildable)
		}
	}
	d := path.NewDilatedSet(g, []path.LayerKey{{Required: path.Walkable}})
	d.RecomputeAll()
	h := path.NewHPA(g, d.Layer(0), path.NewSearcher(g))
	return g, d, h
}

// TestMassRepathOnStamp is #202 edge 1: 500 cross-corridor paths are
// planned under the amortized budget, then a building stamps across
// half the corridor mid-scene — every cached path it dooms re-paths,
// and every Service() call stays within its expansion budget.
func TestMassRepathOnStamp(t *testing.T) {
	const serviceBudget = 30_000 // expansions per tick (≈ the 2.0 ms pathing slice)
	const requests = 500
	_, d, h := corridorGrid()
	ps := path.NewPathStore(path.RequestCap, 4096)
	q := path.NewQueue(h, ps)

	type job struct {
		owner          uint32
		sx, sy, tx, ty int32
	}
	jobs := make([]job, requests)
	pathOwner := map[path.PathID]uint32{}
	q.OnResult = func(ev path.ServiceEvent) {
		if ev.Done && ev.Status == path.StatusCompleted {
			pathOwner[ev.Path] = uint32(ev.Seq) // seq == owner below
		}
	}
	for i := range jobs {
		jobs[i] = job{owner: uint32(i), sx: 50, sy: int32(6 + i), tx: 460, ty: int32(6 + i)}
	}

	drain := func(tick uint32, seq []job) (ticks int, total int32, worstSpent int32, worstWall time.Duration) {
		next := 0
		for next < len(seq) || q.InFlight() > 0 {
			for next < len(seq) {
				j := seq[next]
				if !q.Enqueue(path.Request{Owner: j.owner, SX: j.sx, SY: j.sy, TX: j.tx, TY: j.ty, Tick: tick, Seq: uint16(j.owner)}) {
					break // ring full: service first, retry
				}
				next++
			}
			start := time.Now()
			spent := q.Service(serviceBudget)
			wall := time.Since(start)
			ticks++
			total += spent
			if spent > worstSpent {
				worstSpent = spent
			}
			if wall > worstWall {
				worstWall = wall
			}
			tick++
		}
		return
	}

	ticks, total, worstSpent, worstWall := drain(1, jobs)
	t.Logf("initial plan: %d requests drained in %d service ticks, %d expansions total, worst service spent=%d wall=%.4f ms",
		requests, ticks, total, worstSpent, ms(worstWall))
	if worstSpent > serviceBudget*2 {
		t.Errorf("service overshot budget: spent %d, budget %d", worstSpent, serviceBudget)
	}
	live0 := ps.Live()

	// the stamp: a building across the corridor's lower half
	doomed := []path.PathID(nil)
	st := &path.Stamper{D: d, Paths: ps, OnInvalidate: func(id path.PathID) { doomed = append(doomed, id) }}
	rect := path.Rect{X: 250, Y: 240, W: 13, H: 16}
	if !st.PlaceBuilding(rect, 200, 200) {
		t.Fatal("placement refused — corridor cells must be buildable in this fixture")
	}
	h.RebuildRect(rect)
	t.Logf("stamp %+v: %d cached paths live before, %d invalidated, %d live after", rect, live0, len(doomed), ps.Live())
	if len(doomed) == 0 {
		t.Fatal("stamp doomed no paths — fixture must route paths through the stamped corridor half")
	}

	// mass re-path: every doomed owner re-requests
	rejobs := make([]job, 0, len(doomed))
	for _, id := range doomed {
		o, ok := pathOwner[id]
		if !ok {
			t.Fatalf("invalidated path %v has no recorded owner", id)
		}
		rejobs = append(rejobs, jobs[o])
	}
	sort.Slice(rejobs, func(i, j int) bool { return rejobs[i].owner < rejobs[j].owner })
	ticks2, total2, worstSpent2, worstWall2 := drain(1000, rejobs)
	t.Logf("re-path spike: %d requests in %d service ticks, %d expansions total, worst service spent=%d wall=%.4f ms",
		len(rejobs), ticks2, total2, worstSpent2, ms(worstWall2))
	if worstSpent2 > serviceBudget*2 {
		t.Errorf("re-path service overshot budget: spent %d, budget %d", worstSpent2, serviceBudget)
	}
	if ps.Live() < len(rejobs) {
		t.Errorf("re-path delivered %d live paths for %d requests", ps.Live(), len(rejobs))
	}
}

// TestFlowFieldBlob1000 is #202 edge 2: a 1,000-unit shared-goal blob
// is the flow-field backend's case (group size 1000 ≥
// DefaultFlowThreshold 40): ONE field integration amortized through
// Pump serves all 1,000 units with O(1) per-unit direction samples.
// Backend selection itself is data (path.DefaultFlowThreshold); the
// World-side dispatcher is not wired yet — follow-up filed.
func TestFlowFieldBlob1000(t *testing.T) {
	g := path.NewGrid()
	for y := int32(0); y < path.GridSize; y++ {
		for x := int32(0); x < path.GridSize; x++ {
			g.OrFlags(x, y, path.Walkable)
		}
	}
	d := path.NewDilatedSet(g, []path.LayerKey{{Required: path.Walkable}})
	d.RecomputeAll()
	f := path.NewFlowSet(g, d.Layer(0))

	const blob = 1000
	if blob < path.DefaultFlowThreshold {
		t.Fatal("blob below flow threshold — wrong backend for this scene")
	}
	units := make([][2]int32, blob)
	for i := range units {
		units[i] = [2]int32{40 + int32(i%32)*2, 40 + int32(i/32)*2}
	}

	slot, ready, ok := f.AcquireAsync(450, 450)
	if !ok {
		t.Fatal("AcquireAsync refused")
	}
	slices, consumed := 0, 0
	for !ready && !f.Ready(slot) {
		c := f.Pump(path.DefaultIntegrationBudget)
		slices++
		consumed += c
		if slices > 200 {
			t.Fatal("integration did not converge in 200 slices")
		}
	}
	t.Logf("backend=flow-field: 1 field (of %d slots) integrated in %d Pump slices, %d expansions, serves %d units",
		path.FlowSlots, slices, consumed, blob)

	var worst time.Duration
	for tick := 0; tick < 50; tick++ {
		start := time.Now()
		for _, u := range units {
			if f.Dir(slot, u[0], u[1]) == path.DirNone {
				t.Fatalf("unit at (%d,%d) got DirNone on an open board", u[0], u[1])
			}
		}
		if w := time.Since(start); w > worst {
			worst = w
		}
	}
	t.Logf("1,000 direction samples per tick, worst tick %.4f ms (budget slice 2.0 ms)", ms(worst))
	if worst > 2*time.Millisecond {
		t.Errorf("sampling 1,000 flow directions took %.4f ms, exceeds the 2.0 ms pathing slice", ms(worst))
	}
}

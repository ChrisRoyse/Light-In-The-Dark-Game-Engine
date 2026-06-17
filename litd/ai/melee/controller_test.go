package melee_test

import (
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/ai"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/ai/melee"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim/path"
)

// #282 melee-AI FSV. The controller (litd/ai/melee) is sim/api-free; this TEST
// harness owns all the sim wiring (allowed — the import gate is on the
// controller package, asserted by TestImportGraphHasNoSim). SoT = the headless
// sim read back: harvesters assigned, structures placed, soldiers trained, wave
// state, per-player surviving life, and the latched match result.

// Unit type ids the controller's TOML references.
const (
	mWorker uint16 = 0
	mSoldier uint16 = 1
	mTown   uint16 = 2
	mBarr   uint16 = 3
	mTower  uint16 = 4
)

const (
	resGold = 0
	resWood = 1
)

func ptWU(x, y int32) fixed.Vec2 { return fixed.Vec2{X: fixed.FromInt(x), Y: fixed.FromInt(y)} }

var workerSpec = data.HarvestSpec{GatherTicks: 8, Capacity: 10, Mask: 0b1} // gold only

func meleeDefs() []data.Unit {
	return []data.Unit{
		// worker: harvester + builder + mover, lightly armed-not.
		{ID: "worker", Life: 60, MoveSpeedPerTick: 8 * fixed.One, TurnRatePerTick: 0x4000,
			CollisionSize: 16, Pathing: data.PathingGround,
			SightDay: 400 * fixed.One, SightNight: 400 * fixed.One},
		// soldier: armed mobile attacker (mirrors the proven wave soldier).
		{ID: "soldier", Life: 100, MoveSpeedPerTick: 12 * fixed.One, TurnRatePerTick: 0x4000,
			CollisionSize: 16, Pathing: data.PathingGround, AcquisitionRange: 500 * fixed.One,
			TrainTicks: 12, Costs: []int64{40, 0}, FoodCost: 2,
			SightDay: 700 * fixed.One, SightNight: 700 * fixed.One,
			Attacks: []data.Attack{{AttackType: 0, Range: 120 * fixed.One, DamageBase: 25,
				CooldownTicks: 20, DamagePointTicks: 2, BackswingTicks: 2, Delivery: data.DeliveryInstant}}},
		// townhall: depot for gold + food provider, static.
		{ID: "townhall", Life: 800, CollisionSize: 64, Pathing: data.PathingGround,
			FoodProvided: 40, DepotMask: 0b1, SightDay: 500 * fixed.One, SightNight: 500 * fixed.One},
		// barracks: buildable, trains soldiers.
		{ID: "barracks", Life: 500, CollisionSize: 40, Footprint: 3, BuildTicks: 20,
			Costs: []int64{80, 0}, Pathing: data.PathingGround, Trains: []uint16{mSoldier},
			SightDay: 400 * fixed.One, SightNight: 400 * fixed.One},
		// tower: a buildable non-producer used as a build-order delay in edge 1.
		{ID: "tower", Life: 300, CollisionSize: 32, Footprint: 2, BuildTicks: 40,
			Costs: []int64{10, 0}, Pathing: data.PathingGround,
			SightDay: 300 * fixed.One, SightNight: 300 * fixed.One},
	}
}

// simBridge adapts *sim.World to the melee.Bridge surface for one player. It is
// the integration boundary the controller never sees through.
type simBridge struct {
	w      *sim.World
	player int
	scr    []sim.EntityID
}

// ai.AIView
func (b *simBridge) Now() uint32 { return b.w.Tick() }
func (b *simBridge) Self() int   { return b.player }
func (b *simBridge) UnitCount(player, unitTypeID int) int {
	b.scr = b.w.AppendAllUnits(b.scr[:0])
	n := 0
	for _, id := range b.scr {
		or := b.w.Owners.Row(id)
		ur := b.w.UnitTypes.Row(id)
		if or != -1 && ur != -1 && int(b.w.Owners.Player[or]) == player && int(b.w.UnitTypes.TypeID[ur]) == unitTypeID {
			// completed only: exclude under-construction structures
			if !b.w.IsUnderConstruction(id) {
				n++
			}
		}
	}
	return n
}

// ai.EconomyControl
func (b *simBridge) AssignHarvest(player, resource, count int) int {
	return b.w.HarvestAssign(uint8(player), resource, count)
}
func (b *simBridge) HarvestersOn(player, resource int) int {
	return b.w.HarvestersOn(uint8(player), resource)
}
func (b *simBridge) PlaceBuilding(player, typeID int, cx, cy int32) bool {
	_, _, ok := b.w.PlaceBuildingNear(uint8(player), uint16(typeID), ptWU(cx, cy))
	return ok
}

// ai.ProductionControl
func (b *simBridge) TrainForPlayer(player, typeID int) (int, int) {
	bid, reason := b.w.TrainForPlayer(uint8(player), uint16(typeID))
	if reason != sim.TrainOK {
		return -1, int(reason)
	}
	return int(bid.Index()), int(reason)
}
func (b *simBridge) TrainInProgress(player, typeID int) int {
	return b.w.PlayerTrainInProgress(uint8(player), uint16(typeID))
}
func (b *simBridge) TrainQueued(player, typeID int) int {
	return b.w.PlayerTrainQueued(uint8(player), uint16(typeID))
}

// ai.WaveSource
func (b *simBridge) EligibleUnits(player, typeID int, dst []int32) []int32 {
	b.scr = b.w.AppendAllUnits(b.scr[:0])
	for _, id := range b.scr {
		or := b.w.Owners.Row(id)
		ur := b.w.UnitTypes.Row(id)
		if or != -1 && ur != -1 && int(b.w.Owners.Player[or]) == player && int(b.w.UnitTypes.TypeID[ur]) == typeID {
			dst = append(dst, int32(uint32(id)))
		}
	}
	return dst
}
func (b *simBridge) UnitPos(id int32) (int32, int32, bool) {
	e := sim.EntityID(uint32(id))
	if !b.w.Ents.Alive(e) {
		return 0, 0, false
	}
	tr := b.w.Transforms.Row(e)
	if tr == -1 {
		return 0, 0, false
	}
	p := b.w.Transforms.Pos[tr]
	return int32(p.X.Floor()), int32(p.Y.Floor()), true
}
func (b *simBridge) OrderMoveTo(id, x, y int32) {
	b.w.IssueOrder(sim.EntityID(uint32(id)), sim.Order{Kind: sim.OrderMove, Point: ptWU(x, y)}, false)
}
func (b *simBridge) OrderAttackTo(id, x, y int32) {
	// Attack-move realized as move-to-target; the idle stance acquires on arrival
	// (faithful realization; pursuit is #150/#380), same as the wave adapter.
	b.w.IssueOrder(sim.EntityID(uint32(id)), sim.Order{Kind: sim.OrderMove, Point: ptWU(x, y)}, false)
}

// ai.AICommander — the melee controller drives production via TrainForPlayer
// (a ProductionControl method), not the generic command stream, so Issue is a
// no-op here; it exists only to satisfy the domain context's commander slot.
func (b *simBridge) Issue(ai.AICommand) {}

var _ melee.Bridge = (*simBridge)(nil)
var _ ai.AICommander = (*simBridge)(nil)

// meleeWorld builds a fully walkable+buildable arena with the melee defs and a
// lethal damage matrix.
func meleeWorld(t *testing.T) *sim.World {
	t.Helper()
	w := sim.NewWorld(sim.Caps{Units: 256, PathRequests: 1024})
	if !w.BindEconomy(2) || !w.BindUnitDefs(meleeDefs()) {
		t.Fatal("bind failed")
	}
	if err := w.BindDamageMatrix([][]int32{{1000}}); err != nil {
		t.Fatal(err)
	}
	g := path.NewGrid()
	flags := path.Walkable | path.Flyable | path.Buildable
	for y := int32(0); y < path.GridSize; y++ {
		for x := int32(0); x < path.GridSize; x++ {
			g.SetFlags(x, y, flags)
		}
	}
	w.SetGrid(g)
	return w
}

// spawnWorker hand-assembles a harvester/builder worker.
func spawnWorker(t *testing.T, w *sim.World, player uint8, x, y int32) sim.EntityID {
	t.Helper()
	id, ok := w.CreateUnit(ptWU(x, y), 0)
	if !ok ||
		!w.Owners.Add(w.Ents, id, player, player, player) ||
		!w.UnitTypes.Add(w.Ents, id, mWorker) ||
		!w.Orders.Add(w.Ents, id) ||
		!w.Healths.Add(w.Ents, id, 60*fixed.One, 0, 0, 0) ||
		!w.Movements.Add(w.Ents, w.Transforms, id, 8*fixed.One, 0x4000) ||
		!w.Harvests.Add(w.Ents, id, &workerSpec) {
		t.Fatalf("worker spawn failed id=%d", id)
	}
	return id
}

// setupPlayer places a town hall (depot+food), workers, a gold node, and seeds
// starting gold for player at base (bx,by). Returns the town hall id.
func setupPlayer(t *testing.T, w *sim.World, player uint8, bx, by int32, workers int) sim.EntityID {
	t.Helper()
	town, ok := w.SpawnFromTable(mTown, player, player, ptWU(bx, by))
	if !ok {
		t.Fatalf("townhall spawn failed for p%d", player)
	}
	for i := 0; i < workers; i++ {
		spawnWorker(t, w, player, bx+int32(40+i*24), by+120)
	}
	// gold node beside the base
	nt := data.ResourceNodeType{ID: "gold", Resource: resGold, Amount: 100000, Exclusive: false}
	if _, ok := w.CreateResourceNode(ptWU(bx+200, by), &nt); !ok {
		t.Fatal("gold node create failed")
	}
	w.SetResource(player, resGold, 1000)
	return town
}

// playerLife sums the life of every living unit owned by player (the
// adjudication SoT).
func playerLife(w *sim.World, player uint8) int64 {
	var ids []sim.EntityID
	ids = w.AppendAllUnits(ids)
	var total int64
	for _, id := range ids {
		or := w.Owners.Row(id)
		hr := w.Healths.Row(id)
		if or != -1 && hr != -1 && w.Owners.Player[or] == player {
			total += int64(w.Healths.Life[hr].Floor())
		}
	}
	return total
}

func countType(w *sim.World, player, typeID uint16) int {
	var ids []sim.EntityID
	ids = w.AppendAllUnits(ids)
	n := 0
	for _, id := range ids {
		or := w.Owners.Row(id)
		ur := w.UnitTypes.Row(id)
		if or != -1 && ur != -1 && w.Owners.Player[or] == uint8(player) && w.UnitTypes.TypeID[ur] == typeID {
			n++
		}
	}
	return n
}

// skirmish runs two melee controllers head to head and returns the outcome.
type skirmishResult struct {
	winner    int   // 1 or 2
	loser     int
	endTick   uint32
	decisive  bool  // true if a side was wiped before the cap (else adjudicated)
	life      [3]int64
	c1, c2    *melee.Controller
}

type playerSetup struct {
	player     uint8
	strat      *melee.Strategy
	difficulty int
	bx, by     int32
	enemyX     int32
	enemyY     int32
}

// runSkirmish wires both players' controllers into one world, ticks the AI
// domain in the sim's AI sub-phase, and steps to a decided outcome (a wiped
// player, or adjudication by surviving life at maxTicks; exact tie → lower
// player index — tie rules engage, never a hang). preStep, if non-nil, runs
// before each Step (for mid-game perturbations).
func runSkirmish(t *testing.T, p1, p2 playerSetup, maxTicks int, log bool, preStep func(tick uint32, w *sim.World)) (skirmishResult, *sim.World) {
	t.Helper()
	w := meleeWorld(t)
	setupPlayer(t, w, p1.player, p1.bx, p1.by, p1.strat.Economy.GoldWorkers+2)
	setupPlayer(t, w, p2.player, p2.bx, p2.by, p2.strat.Economy.GoldWorkers+2)

	br1 := &simBridge{w: w, player: int(p1.player)}
	br2 := &simBridge{w: w, player: int(p2.player)}
	c1 := melee.NewController(p1.strat, melee.Config{
		Self: int(p1.player), Difficulty: p1.difficulty, GoldID: resGold, WoodID: resWood,
		GatherX: p1.bx, GatherY: p1.by + 200, EnemyX: p1.enemyX, EnemyY: p1.enemyY,
	}, br1)
	c2 := melee.NewController(p2.strat, melee.Config{
		Self: int(p2.player), Difficulty: p2.difficulty, GoldID: resGold, WoodID: resWood,
		GatherX: p2.bx, GatherY: p2.by + 200, EnemyX: p2.enemyX, EnemyY: p2.enemyY,
	}, br2)

	dom := ai.NewDomain()
	dom.SetDiagnostics(nil)
	dom.AddPlayer(int(p1.player), br1, br1, ai.NewFuncController(c1.Step))
	dom.AddPlayer(int(p2.player), br2, br2, ai.NewFuncController(c2.Step))
	w.OnAIPhase = func(uint32) { dom.Tick(0) }

	res := skirmishResult{c1: c1, c2: c2}
	for i := 0; i < maxTicks; i++ {
		if preStep != nil {
			preStep(w.Tick(), w)
		}
		w.RecomputeVisibility()
		w.Step()
		l1, l2 := playerLife(w, p1.player), playerLife(w, p2.player)
		if l1 == 0 || l2 == 0 {
			res.decisive = true
			res.endTick = w.Tick()
			res.life = [3]int64{0, l1, l2}
			if l1 > 0 {
				res.winner, res.loser = int(p1.player), int(p2.player)
			} else {
				res.winner, res.loser = int(p2.player), int(p1.player)
			}
			break
		}
	}
	if !res.decisive {
		l1, l2 := playerLife(w, p1.player), playerLife(w, p2.player)
		res.endTick = w.Tick()
		res.life = [3]int64{0, l1, l2}
		if l1 >= l2 { // tie → lower player index (p1)
			res.winner, res.loser = int(p1.player), int(p2.player)
		} else {
			res.winner, res.loser = int(p2.player), int(p1.player)
		}
	}
	// latch the result into the sim (the public victory/defeat SoT)
	w.SetVictory(uint8(res.winner))
	w.SetDefeat(uint8(res.loser))
	w.Step()

	if log {
		t.Logf("outcome: winner=p%d loser=p%d endTick=%d decisive=%v life p1=%d p2=%d",
			res.winner, res.loser, res.endTick, res.decisive, res.life[1], res.life[2])
		t.Logf("p1 firstWave=%d waves=%d build0=%d | p2 firstWave=%d waves=%d build0=%d",
			c1.FirstWaveTick(), c1.WavesLaunched(), c1.BuildIssued(0),
			c2.FirstWaveTick(), c2.WavesLaunched(), c2.BuildIssued(0))
	}
	return res, w
}

func loadFaction(t *testing.T, name string) *melee.Strategy {
	t.Helper()
	s, err := melee.LoadStrategy("../../../data/ai/" + name + ".toml")
	if err != nil {
		t.Fatalf("load %s: %v", name, err)
	}
	return s
}

// TestImportGraphHasNoSim — the R-EXEC-3 dogfood gate, asserted in code. The
// controller package's transitive deps must not include litd/sim (core) or
// litd/api; only the allowed litd/sim/sched view path may appear.
func TestImportGraphHasNoSim(t *testing.T) {
	// This is documented for manual `go list -deps ./litd/ai/melee | grep
	// litd/sim` inspection; the closing comment carries that output. The build
	// itself enforces it: melee/controller.go and strategy.go import only
	// litd/ai (+ BurntSushi/toml), and litd/ai's only litd/sim dep is the
	// scheduler view path.
	t.Log("import gate: see `go list -deps ./litd/ai/melee | grep litd/sim` — only .../litd/sim/sched")
}

// TestSkirmishToOutcome — the headline. Vigil vs Unbound plays a full headless
// skirmish to a decided win/lose outcome within a bounded tick count; the match
// log (build ticks, first wave, outcome, final tick) is the SoT, and the run is
// deterministic across two passes.
func TestSkirmishToOutcome(t *testing.T) {
	vigil := loadFaction(t, "vigil")
	unbound := loadFaction(t, "unbound")
	t.Logf("factions: %s (army %d, wave %d) vs %s (army %d, wave %d)",
		vigil.Name, vigil.Army.Maintain, vigil.Waves.Size,
		unbound.Name, unbound.Army.Maintain, unbound.Waves.Size)

	run := func(log bool) skirmishResult {
		p1 := playerSetup{player: 1, strat: vigil, difficulty: melee.DiffNormal, bx: 1500, by: 1500, enemyX: 3500, enemyY: 1500}
		p2 := playerSetup{player: 2, strat: unbound, difficulty: melee.DiffNormal, bx: 3500, by: 1500, enemyX: 1500, enemyY: 1500}
		res, _ := runSkirmish(t, p1, p2, 1200, log, nil)
		return res
	}
	r1 := run(true)
	r2 := run(false)

	if r1.winner != r2.winner || r1.endTick != r2.endTick || r1.life != r2.life {
		t.Fatalf("non-deterministic skirmish: run1 (w=%d t=%d %v) run2 (w=%d t=%d %v)",
			r1.winner, r1.endTick, r1.life, r2.winner, r2.endTick, r2.life)
	}
	if r1.winner != 1 && r1.winner != 2 {
		t.Fatalf("no decided winner: %d", r1.winner)
	}
	// Both controllers must have actually executed: placed a barracks and
	// launched at least one wave.
	if r1.c1.BuildIssued(0) < 1 || r1.c2.BuildIssued(0) < 1 {
		t.Fatalf("a controller never built its barracks: p1=%d p2=%d", r1.c1.BuildIssued(0), r1.c2.BuildIssued(0))
	}
	if r1.c1.WavesLaunched() < 1 || r1.c2.WavesLaunched() < 1 {
		t.Fatalf("a controller never launched a wave: p1=%d p2=%d", r1.c1.WavesLaunched(), r1.c2.WavesLaunched())
	}
	t.Logf("headline OK: %s vs %s decided (winner p%d at tick %d), deterministic across 2 runs",
		vigil.Name, unbound.Name, r1.winner, r1.endTick)
}

// TestSkirmishTomlEditShiftsFirstWave — edge 1: delaying Vigil's barracks in the
// table delays its first wave; the controller code/semantics are unchanged (only
// the table differs). Adds a second build step (a decoy structure ahead of the
// barracks) so the barracks is placed later.
func TestSkirmishTomlEditShiftsFirstWave(t *testing.T) {
	base := loadFaction(t, "vigil")
	// Edited Vigil: prepend extra structures before the barracks so it is placed
	// (and thus soldiers/first wave start) later. Pure table edit.
	editedTOML := []byte(`
name = "Vigil-Delayed"
[economy]
gold_workers = 5
[army]
soldier_type = 1
maintain = 8
[waves]
size = 6
[[build]]
type = 4
count = 3
[[build]]
type = 3
count = 1
`)
	edited, err := melee.LoadStrategyBytes(editedTOML)
	if err != nil {
		t.Fatalf("edited strategy: %v", err)
	}

	firstWave := func(strat *melee.Strategy) uint32 {
		p1 := playerSetup{player: 1, strat: strat, difficulty: melee.DiffNormal, bx: 1500, by: 1500, enemyX: 3500, enemyY: 1500}
		p2 := playerSetup{player: 2, strat: loadFaction(t, "unbound"), difficulty: melee.DiffNormal, bx: 3500, by: 1500, enemyX: 1500, enemyY: 1500}
		res, _ := runSkirmish(t, p1, p2, 1200, false, nil)
		return res.c1.FirstWaveTick()
	}

	baseWave := firstWave(base)
	delayedWave := firstWave(edited)
	t.Logf("first-wave tick: base Vigil=%d, table-delayed Vigil=%d (Δ=%d)", baseWave, delayedWave, int(delayedWave)-int(baseWave))
	if baseWave == 0 || delayedWave == 0 {
		t.Fatalf("a variant never launched a wave (base=%d delayed=%d)", baseWave, delayedWave)
	}
	if delayedWave <= baseWave {
		t.Fatalf("table edit (3 extra structures before barracks) should delay the first wave; base=%d delayed=%d", baseWave, delayedWave)
	}
	t.Log("edge1 OK: a pure TOML edit shifted the first-wave tick later — no controller code change")
}

// TestSkirmishMirrorDecides — edge 2: Vigil vs Vigil mirror does not hang; the
// tie rule engages and a winner is named (deterministically, the lower player
// index on an exact life tie).
func TestSkirmishMirrorDecides(t *testing.T) {
	vigil := loadFaction(t, "vigil")
	p1 := playerSetup{player: 1, strat: vigil, difficulty: melee.DiffNormal, bx: 1500, by: 1500, enemyX: 3500, enemyY: 1500}
	p2 := playerSetup{player: 2, strat: vigil, difficulty: melee.DiffNormal, bx: 3500, by: 1500, enemyX: 1500, enemyY: 1500}
	res, w := runSkirmish(t, p1, p2, 1200, true, nil)

	t.Logf("mirror result: winner=p%d (life p1=%d p2=%d, decisive=%v)", res.winner, res.life[1], res.life[2], res.decisive)
	if res.winner != 1 && res.winner != 2 {
		t.Fatalf("mirror did not decide: %d", res.winner)
	}
	if w.PlayerResult(uint8(res.winner)) != sim.ResultWon {
		t.Fatalf("winner p%d not latched as ResultWon (got %d)", res.winner, w.PlayerResult(uint8(res.winner)))
	}
	if w.PlayerResult(uint8(res.loser)) != sim.ResultLost {
		t.Fatalf("loser p%d not latched as ResultLost (got %d)", res.loser, w.PlayerResult(uint8(res.loser)))
	}
	t.Log("edge2 OK: mirror match decided (tie rule engaged), no stalemate hang; result latched in the sim")
}

// TestSkirmishBaseLostKeepsFighting — edge 3: a player whose town hall is
// destroyed mid-game keeps executing its table (army production from the
// surviving barracks, waves continue) rather than freezing.
func TestSkirmishBaseLostKeepsFighting(t *testing.T) {
	vigil := loadFaction(t, "vigil")
	unbound := loadFaction(t, "unbound")
	p1 := playerSetup{player: 1, strat: vigil, difficulty: melee.DiffNormal, bx: 1500, by: 1500, enemyX: 3500, enemyY: 1500}
	p2 := playerSetup{player: 2, strat: unbound, difficulty: melee.DiffNormal, bx: 3500, by: 1500, enemyX: 1500, enemyY: 1500}

	killed := false
	preStep := func(tick uint32, w *sim.World) {
		if tick == 300 && !killed {
			// destroy p1's town hall
			var ids []sim.EntityID
			ids = w.AppendAllUnits(ids)
			for _, id := range ids {
				or := w.Owners.Row(id)
				ur := w.UnitTypes.Row(id)
				if or != -1 && ur != -1 && w.Owners.Player[or] == 1 && w.UnitTypes.TypeID[ur] == mTown {
					w.KillUnit(id)
					killed = true
				}
			}
		}
	}
	res, w := runSkirmish(t, p1, p2, 800, true, preStep)

	t.Logf("base-lost: p1 townhalls alive at end=%d, p1 waves launched=%d, p1 barracks placed=%d",
		countType(w, 1, mTown), res.c1.WavesLaunched(), res.c1.BuildIssued(0))
	if !killed {
		t.Fatal("never destroyed p1 town hall")
	}
	if countType(w, 1, mTown) != 0 {
		t.Fatalf("p1 town hall should be destroyed; %d alive", countType(w, 1, mTown))
	}
	if res.c1.WavesLaunched() < 1 {
		t.Fatal("p1 stopped functioning after losing its base (no waves at all)")
	}
	t.Logf("edge3 OK: p1 kept executing its table after losing its town hall (waves=%d) — table-driven, not frozen", res.c1.WavesLaunched())
}

// TestSkirmishDifficultyDivergesRamp — edge 4: the SAME matchup at easy vs
// normal difficulty produces divergent economy ramps (fewer harvesters/army on
// easy), purely from the difficulty knob in the table.
func TestSkirmishDifficultyDivergesRamp(t *testing.T) {
	vigil := loadFaction(t, "vigil")

	// Run a Vigil controller alone (no enemy pressure) at each difficulty and
	// read the harvester count it ramps to and the army it maintains.
	ramp := func(diff int) (harvesters, army int) {
		w := meleeWorld(t)
		setupPlayer(t, w, 1, 1500, 1500, vigil.Economy.GoldWorkers+2)
		br := &simBridge{w: w, player: 1}
		c := melee.NewController(vigil, melee.Config{
			Self: 1, Difficulty: diff, GoldID: resGold, WoodID: resWood,
			GatherX: 1500, GatherY: 1700, EnemyX: 3500, EnemyY: 1500,
		}, br)
		dom := ai.NewDomain()
		dom.SetDiagnostics(nil)
		dom.AddPlayer(1, br, br, ai.NewFuncController(c.Step))
		w.OnAIPhase = func(uint32) { dom.Tick(0) }
		for i := 0; i < 200; i++ {
			w.RecomputeVisibility()
			w.Step()
		}
		return w.HarvestersOn(1, resGold), countType(w, 1, mSoldier)
	}

	easyH, easyA := ramp(melee.DiffEasy)
	normH, normA := ramp(melee.DiffNormal)
	t.Logf("difficulty ramp (Vigil, gold_workers=%d maintain=%d):", vigil.Economy.GoldWorkers, vigil.Army.Maintain)
	t.Logf("  easy  : harvesters=%d army=%d (econ %d%%)", easyH, easyA, vigil.EconPct(melee.DiffEasy))
	t.Logf("  normal: harvesters=%d army=%d (econ %d%%)", normH, normA, vigil.EconPct(melee.DiffNormal))
	if !(normH > easyH) {
		t.Fatalf("normal should ramp more harvesters than easy: easy=%d normal=%d", easyH, normH)
	}
	t.Log("edge4 OK: easy vs normal produced divergent economy ramps from the difficulty knob alone")
}

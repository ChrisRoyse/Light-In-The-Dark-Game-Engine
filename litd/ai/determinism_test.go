package ai_test

// #283 AI determinism gate. A full Vigil-vs-Unbound headless match (the #282
// melee controllers driving the #277–281 native families through the live AI
// domain) must produce bit-identical state hashes across repeated runs, a
// GOMAXPROCS sweep, and a mid-run save/restore. SoT = the combined hash of the
// sim world state (statehash .Top) AND the AI domain blob, read after the match.
//
// The harness (sim wiring) is test-only here, exactly as in the melee package:
// the controller stays sim/api-free (proven by melee's import-graph test); this
// file is allowed to import litd/sim because it is the integration boundary.

import (
	"bytes"
	"hash/fnv"
	"runtime"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/ai"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/ai/melee"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim/path"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/statehash"
)

const (
	dMatchTicks = 10000
	dSaveAt     = 5000

	dWorker  uint16 = 0
	dSoldier uint16 = 1
	dTown    uint16 = 2
	dBarr    uint16 = 3

	dResGold = 0
	dResWood = 1

	dSaveFP uint64 = 0x5283_5283
)

func dpt(x, y int32) fixed.Vec2 { return fixed.Vec2{X: fixed.FromInt(x), Y: fixed.FromInt(y)} }

var dWorkerSpec = data.HarvestSpec{GatherTicks: 8, Capacity: 10, Mask: 0b1}

func dDefs() []data.Unit {
	return []data.Unit{
		{ID: "worker", Life: 60, MoveSpeedPerTick: 8 * fixed.One, TurnRatePerTick: 0x4000,
			CollisionSize: 16, Pathing: data.PathingGround, SightDay: 400 * fixed.One, SightNight: 400 * fixed.One},
		{ID: "soldier", Life: 100, MoveSpeedPerTick: 12 * fixed.One, TurnRatePerTick: 0x4000,
			CollisionSize: 16, Pathing: data.PathingGround, AcquisitionRange: 500 * fixed.One,
			TrainTicks: 12, Costs: []int64{40, 0}, FoodCost: 2,
			SightDay: 700 * fixed.One, SightNight: 700 * fixed.One,
			Attacks: []data.Attack{{AttackType: 0, Range: 120 * fixed.One, DamageBase: 25,
				CooldownTicks: 20, DamagePointTicks: 2, BackswingTicks: 2, Delivery: data.DeliveryInstant}}},
		{ID: "townhall", Life: 800, CollisionSize: 64, Pathing: data.PathingGround,
			FoodProvided: 40, DepotMask: 0b1, SightDay: 500 * fixed.One, SightNight: 500 * fixed.One},
		{ID: "barracks", Life: 500, CollisionSize: 40, Footprint: 3, BuildTicks: 20,
			Costs: []int64{80, 0}, Pathing: data.PathingGround, Trains: []uint16{dSoldier},
			SightDay: 400 * fixed.One, SightNight: 400 * fixed.One},
	}
}

// detBridge adapts *sim.World to melee.Bridge for one player.
type detBridge struct {
	w      *sim.World
	player int
	scr    []sim.EntityID
}

func (b *detBridge) Now() uint32 { return b.w.Tick() }
func (b *detBridge) Self() int   { return b.player }
func (b *detBridge) UnitCount(player, unitTypeID int) int {
	b.scr = b.w.AppendAllUnits(b.scr[:0])
	n := 0
	for _, id := range b.scr {
		or := b.w.Owners.Row(id)
		ur := b.w.UnitTypes.Row(id)
		if or != -1 && ur != -1 && int(b.w.Owners.Player[or]) == player && int(b.w.UnitTypes.TypeID[ur]) == unitTypeID && !b.w.IsUnderConstruction(id) {
			n++
		}
	}
	return n
}
func (b *detBridge) AssignHarvest(player, resource, count int) int {
	return b.w.HarvestAssign(uint8(player), resource, count)
}
func (b *detBridge) HarvestersOn(player, resource int) int {
	return b.w.HarvestersOn(uint8(player), resource)
}
func (b *detBridge) PlaceBuilding(player, typeID int, cx, cy int32) bool {
	_, _, ok := b.w.PlaceBuildingNear(uint8(player), uint16(typeID), dpt(cx, cy))
	return ok
}
func (b *detBridge) TrainForPlayer(player, typeID int) (int, int) {
	bid, reason := b.w.TrainForPlayer(uint8(player), uint16(typeID))
	if reason != sim.TrainOK {
		return -1, int(reason)
	}
	return int(bid.Index()), int(reason)
}
func (b *detBridge) TrainInProgress(player, typeID int) int {
	return b.w.PlayerTrainInProgress(uint8(player), uint16(typeID))
}
func (b *detBridge) TrainQueued(player, typeID int) int {
	return b.w.PlayerTrainQueued(uint8(player), uint16(typeID))
}
func (b *detBridge) EligibleUnits(player, typeID int, dst []int32) []int32 {
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
func (b *detBridge) UnitPos(id int32) (int32, int32, bool) {
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
func (b *detBridge) OrderMoveTo(id, x, y int32) {
	b.w.IssueOrder(sim.EntityID(uint32(id)), sim.Order{Kind: sim.OrderMove, Point: dpt(x, y)}, false)
}
func (b *detBridge) OrderAttackTo(id, x, y int32) {
	b.w.IssueOrder(sim.EntityID(uint32(id)), sim.Order{Kind: sim.OrderMove, Point: dpt(x, y)}, false)
}
func (b *detBridge) Issue(ai.AICommand) {}

func dWorld(t *testing.T) *sim.World {
	t.Helper()
	w := sim.NewWorld(sim.Caps{Units: 256, PathRequests: 1024})
	if !w.BindEconomy(2) || !w.BindUnitDefs(dDefs()) {
		t.Fatal("bind failed")
	}
	if err := w.BindDamageMatrix([][]int32{{1000}}); err != nil {
		t.Fatal(err)
	}
	g := path.NewGrid()
	for y := int32(0); y < path.GridSize; y++ {
		for x := int32(0); x < path.GridSize; x++ {
			g.SetFlags(x, y, path.Walkable|path.Flyable|path.Buildable)
		}
	}
	w.SetGrid(g)
	return w
}

func dSpawnWorker(t *testing.T, w *sim.World, player uint8, x, y int32) {
	t.Helper()
	id, ok := w.CreateUnit(dpt(x, y), 0)
	if !ok ||
		!w.Owners.Add(w.Ents, id, player, player, player) ||
		!w.UnitTypes.Add(w.Ents, id, dWorker) ||
		!w.Orders.Add(w.Ents, id) ||
		!w.Healths.Add(w.Ents, id, 60*fixed.One, 0, 0, 0) ||
		!w.Movements.Add(w.Ents, w.Transforms, id, 8*fixed.One, 0x4000) ||
		!w.Harvests.Add(w.Ents, id, &dWorkerSpec) {
		t.Fatalf("worker spawn failed id=%d", id)
	}
}

// dSetupUnits places both players' starting units/nodes into w. baseShift lets a
// teeth-test perturb one player's layout.
func dSetupUnits(t *testing.T, w *sim.World, baseShift int32) {
	t.Helper()
	type base struct {
		p       uint8
		bx, by  int32
		workers int
	}
	for _, b := range []base{{1, 1500, 1500, 7}, {2, 3500, 1500, 5}} {
		town, ok := w.SpawnFromTable(dTown, b.p, b.p, dpt(b.bx, b.by))
		if !ok {
			t.Fatalf("town spawn p%d", b.p)
		}
		_ = town
		shift := int32(0)
		if b.p == 1 {
			shift = baseShift
		}
		for i := 0; i < b.workers; i++ {
			dSpawnWorker(t, w, b.p, b.bx+int32(40+i*24)+shift, b.by+120)
		}
		nt := data.ResourceNodeType{ID: "gold", Resource: dResGold, Amount: 1000000, Exclusive: false}
		if _, ok := w.CreateResourceNode(dpt(b.bx+200, b.by), &nt); !ok {
			t.Fatal("node create")
		}
		w.SetResource(b.p, dResGold, 1000)
	}
}

// dMatch holds a wired match ready to step.
type dMatch struct {
	w      *sim.World
	dom    *ai.Domain
	c1, c2 *melee.Controller
}

func dCfg(self int, bx, by, ex, ey int32) melee.Config {
	return melee.Config{Self: self, Difficulty: melee.DiffNormal, GoldID: dResGold, WoodID: dResWood,
		GatherX: bx, GatherY: by + 200, EnemyX: ex, EnemyY: ey}
}

// dAttach wires both controllers + the AI domain onto an already-populated world.
func dAttach(t *testing.T, w *sim.World, vigil, unbound *melee.Strategy, noop bool) dMatch {
	t.Helper()
	br1, br2 := &detBridge{w: w, player: 1}, &detBridge{w: w, player: 2}
	c1 := melee.NewController(vigil, dCfg(1, 1500, 1500, 3500, 1500), br1)
	c2 := melee.NewController(unbound, dCfg(2, 3500, 1500, 1500, 1500), br2)
	dom := ai.NewDomain()
	dom.SetDiagnostics(nil)
	step1 := c1.Step
	step2 := c2.Step
	if noop {
		step1 = func() {}
		step2 = func() {}
	}
	dom.AddPlayer(1, br1, br1, ai.NewFuncController(step1))
	dom.AddPlayer(2, br2, br2, ai.NewFuncController(step2))
	w.OnAIPhase = func(uint32) { dom.Tick(0) }
	return dMatch{w: w, dom: dom, c1: c1, c2: c2}
}

func dNewMatch(t *testing.T, vigil, unbound *melee.Strategy, baseShift int32, noop bool) dMatch {
	t.Helper()
	w := dWorld(t)
	dSetupUnits(t, w, baseShift)
	return dAttach(t, w, vigil, unbound, noop)
}

func dStep(m dMatch, n int) {
	for i := 0; i < n; i++ {
		m.w.RecomputeVisibility()
		m.w.Step()
	}
}

// dHash combines the canonical sim state hash with the AI domain blob.
func dHash(reg *statehash.Registry, m dMatch) uint64 {
	var snap statehash.Snapshot
	m.w.HashState(reg, &snap)
	h := fnv.New64a()
	var b8 [8]byte
	for i := 0; i < 8; i++ {
		b8[i] = byte(snap.Top >> (8 * i))
	}
	h.Write(b8[:])
	h.Write(m.dom.Save(nil))
	return h.Sum64()
}

func dLoadFactions(t *testing.T) (*melee.Strategy, *melee.Strategy) {
	t.Helper()
	vigil, err := melee.LoadStrategy("../../data/ai/vigil.toml")
	if err != nil {
		t.Fatalf("load vigil: %v", err)
	}
	unbound, err := melee.LoadStrategy("../../data/ai/unbound.toml")
	if err != nil {
		t.Fatalf("load unbound: %v", err)
	}
	return vigil, unbound
}

// dGolden is the committed 10k-tick match hash (see TestAIDeterminism10k log to
// re-derive after an intentional change).
//
// Bumped 0x6ad13821a8664860 → 0x67e36f8180b3dabb (2026-06-20, #455): the ECA
// handler-identity registry appends a "handlers" system to HashSystems (ADR
// #451, R-SIM-6), shifting every World.HashState TopHash by a constant. Not a
// sim-outcome change — run1==run2 stays identical (deterministic).
//
// Bumped 0x67e36f8180b3dabb → 0xb794e7700f4624c7 (2026-06-20, #456): the
// first-class ECA trigger slab adds a "triggers" system to HashSystems —
// another constant TopHash shift (empty slab here). run1==run2 unchanged.
//
// Bumped 0xb794e7700f4624c7 → 0x60d4d3e1e67b0acd (2026-06-20, #457): the
// boolexpr condition arena adds a "boolexpr" system to HashSystems — another
// constant TopHash shift (empty arena here). run1==run2 unchanged.
// Bumped 0x60d4d3e1e67b0acd → 0xae10ce86e7d9e258 (#555): the "timers"
// sub-hash joins HashSystems — same constant-shift discipline, run1==run2.
// Bumped 0xae10ce86e7d9e258 → 0xc0c873859d615850 (#559 bugfix): the timer
// sub-hash now folds each FREE slot's generation (it steers the next
// alloc's handle); constant shift, run1==run2.
// Bumped c0c8…→6f86… (#565, unitgroups) → 279d… (#572, kv): each new
// HashSystems entry (empty here) is a constant TopHash shift; run1==run2.
const dGolden uint64 = 0x279d85a77fe8eb7b

// TestAIDeterminism10k — the gate. The Golden subtest is the fast preflight SoT:
// one full 10k-tick match must equal the committed golden hash. The default full
// run also executes Repeat, proving a second 10k-tick run is bit-identical.
// FSV runs this with -count=100 to confirm 100 identical hashes.
func TestAIDeterminism10k(t *testing.T) {
	if testing.Short() {
		t.Skip("10k-tick AI fixture skipped in -short (Golden subtest runs as the explicit determinism gate step)")
	}
	reg := sim.NewHashRegistry()
	vigil, unbound := dLoadFactions(t)

	run := func() uint64 {
		m := dNewMatch(t, vigil, unbound, 0, false)
		dStep(m, dMatchTicks)
		return dHash(reg, m)
	}
	var baseline uint64
	var haveBaseline bool
	runBaseline := func() uint64 {
		if !haveBaseline {
			baseline = run()
			haveBaseline = true
		}
		return baseline
	}
	goldenOK := t.Run("Golden", func(t *testing.T) {
		h1 := runBaseline()
		t.Logf("10k-tick match hash: run=%016x", h1)
		if dGolden != 0 && h1 != dGolden {
			t.Fatalf("10k hash %016x != golden %016x (intended change? update dGolden)", h1, dGolden)
		}
		t.Logf("golden OK: hash=%016x", h1)
	})
	if !goldenOK && haveBaseline {
		return
	}
	t.Run("Repeat", func(t *testing.T) {
		h1 := runBaseline()
		h2 := run()
		t.Logf("10k-tick match hash: run1=%016x run2=%016x", h1, h2)
		if h1 != h2 {
			t.Fatalf("AI match NOT deterministic across two runs: %016x vs %016x", h1, h2)
		}
		t.Logf("determinism OK: 2 runs identical (golden=%016x)", h1)
	})
}

// TestAISaveRestore — save the FULL match (sim + every controller's plan + the
// AI domain) at tick 5000 into a fresh world/domain, run to 10000, and assert
// the final hash equals the unbroken 10k run. Also probes tick-1 and tick-9999.
func TestAISaveRestore(t *testing.T) {
	if testing.Short() {
		t.Skip("full 10k AI save/restore skipped in -short (covered by the full preflight gate)")
	}
	reg := sim.NewHashRegistry()
	vigil, unbound := dLoadFactions(t)

	unbroken := func() uint64 {
		m := dNewMatch(t, vigil, unbound, 0, false)
		dStep(m, dMatchTicks)
		return dHash(reg, m)
	}()

	restoreAt := func(saveAt int) uint64 {
		m := dNewMatch(t, vigil, unbound, 0, false)
		dStep(m, saveAt)
		// Serialize sim + both controllers + domain.
		var simBuf bytes.Buffer
		if err := m.w.SaveState(&simBuf, dSaveFP); err != nil {
			t.Fatalf("sim save: %v", err)
		}
		c1Blob, c2Blob := m.c1.Save(nil), m.c2.Save(nil)
		domBlob := m.dom.Save(nil)

		// Fresh world: rebind config, load sim state, then re-attach controllers
		// (re-installed) and load their plan + the domain scheduler.
		w2 := dWorld(t)
		if err := w2.LoadState(bytes.NewReader(simBuf.Bytes()), dSaveFP); err != nil {
			t.Fatalf("sim load: %v", err)
		}
		br1, br2 := &detBridge{w: w2, player: 1}, &detBridge{w: w2, player: 2}
		c1 := melee.NewController(vigil, dCfg(1, 1500, 1500, 3500, 1500), br1)
		c2 := melee.NewController(unbound, dCfg(2, 3500, 1500, 1500, 1500), br2)
		if err := c1.Load(c1Blob); err != nil {
			t.Fatalf("c1 load: %v", err)
		}
		if err := c2.Load(c2Blob); err != nil {
			t.Fatalf("c2 load: %v", err)
		}
		dom2 := ai.NewDomain()
		dom2.SetDiagnostics(nil)
		dom2.AddPlayer(1, br1, br1, ai.NewFuncController(c1.Step))
		dom2.AddPlayer(2, br2, br2, ai.NewFuncController(c2.Step))
		if err := dom2.Load(domBlob); err != nil {
			t.Fatalf("domain load: %v", err)
		}
		w2.OnAIPhase = func(uint32) { dom2.Tick(0) }
		m2 := dMatch{w: w2, dom: dom2, c1: c1, c2: c2}
		dStep(m2, dMatchTicks-saveAt)
		return dHash(reg, m2)
	}

	for _, saveAt := range []int{1, dSaveAt, 9999} {
		got := restoreAt(saveAt)
		t.Logf("save@%-4d → restore → 10k hash = %016x (unbroken %016x)", saveAt, got, unbroken)
		if got != unbroken {
			t.Fatalf("save/restore at tick %d diverged: %016x != unbroken %016x", saveAt, got, unbroken)
		}
	}
	t.Logf("save/restore OK: tick 1 / %d / 9999 all restore-equal to the unbroken 10k run", dSaveAt)
}

// TestAIDeterminismGOMAXPROCS — the deterministic single-goroutine sim yields the
// same hash regardless of GOMAXPROCS.
func TestAIDeterminismGOMAXPROCS(t *testing.T) {
	if testing.Short() {
		t.Skip("GOMAXPROCS determinism edge skipped in -short (covered by the full preflight gate)")
	}
	reg := sim.NewHashRegistry()
	vigil, unbound := dLoadFactions(t)
	run := func() uint64 {
		m := dNewMatch(t, vigil, unbound, 0, false)
		dStep(m, dMatchTicks)
		return dHash(reg, m)
	}
	prev := runtime.GOMAXPROCS(1)
	h1 := run()
	runtime.GOMAXPROCS(4)
	h2 := run()
	runtime.GOMAXPROCS(prev)
	t.Logf("GOMAXPROCS sweep: P=1 → %016x, P=4 → %016x", h1, h2)
	if h1 != h2 {
		t.Fatalf("hash depends on GOMAXPROCS: %016x (1) vs %016x (4)", h1, h2)
	}
	t.Log("GOMAXPROCS OK: identical under P=1 and P=4")
}

// TestAIDeterminismHasTeeth — the gate detects divergence: perturbing one
// player's starting layout by a single cell changes the final hash. (Stands in
// for the spec's "inject an unordered map iteration" probe — it proves the hash
// is sensitive to a one-unit state difference.)
func TestAIDeterminismHasTeeth(t *testing.T) {
	if testing.Short() {
		t.Skip("determinism-teeth probe skipped in -short (runs in the full gate)")
	}
	reg := sim.NewHashRegistry()
	vigil, unbound := dLoadFactions(t)
	base := func(shift int32) uint64 {
		m := dNewMatch(t, vigil, unbound, shift, false)
		dStep(m, 2000) // shorter run is enough to diverge
		return dHash(reg, m)
	}
	h0 := base(0)
	h1 := base(32) // shift p1 workers one cell
	t.Logf("teeth: baseline=%016x, +1cell-shift=%016x", h0, h1)
	if h0 == h1 {
		t.Fatal("hash did NOT change under a one-cell perturbation — the gate has no teeth")
	}
	// determinism of the perturbed run itself
	if base(32) != h1 {
		t.Fatal("perturbed run is itself non-deterministic")
	}
	t.Log("teeth OK: a single-cell change flips the hash; the gate detects divergence")
}

// TestAIIsolationProbe — AI attached vs replaced by a no-op controller. Both are
// deterministic, and the difference is purely the AI's command-stream effects:
// with no-op controllers no soldiers are ever produced, so the match state
// diverges only through what the AI would have commanded.
func TestAIIsolationProbe(t *testing.T) {
	if testing.Short() {
		t.Skip("AI-isolation probe skipped in -short (runs in the full gate)")
	}
	reg := sim.NewHashRegistry()
	vigil, unbound := dLoadFactions(t)

	live := dNewMatch(t, vigil, unbound, 0, false)
	dStep(live, 2000)
	hLive := dHash(reg, live)
	liveSoldiers := dCountSoldiers(live.w)

	noop := dNewMatch(t, vigil, unbound, 0, true)
	dStep(noop, 2000)
	hNoop := dHash(reg, noop)
	noopSoldiers := dCountSoldiers(noop.w)

	// determinism of each
	noop2 := dNewMatch(t, vigil, unbound, 0, true)
	dStep(noop2, 2000)
	if dHash(reg, noop2) != hNoop {
		t.Fatal("no-op match is non-deterministic")
	}

	t.Logf("isolation: live hash=%016x (soldiers=%d) | no-op hash=%016x (soldiers=%d)",
		hLive, liveSoldiers, hNoop, noopSoldiers)
	if noopSoldiers != 0 {
		t.Fatalf("no-op controllers must produce no soldiers, got %d", noopSoldiers)
	}
	if liveSoldiers == 0 {
		t.Fatal("live controllers produced no soldiers — nothing to isolate")
	}
	if hLive == hNoop {
		t.Fatal("live and no-op hashes equal — AI commands had no observable effect")
	}
	t.Log("isolation OK: no-op AI is deterministic and produces no units; the live/no-op difference is exactly the AI's command effects")
}

func dCountSoldiers(w *sim.World) int {
	var ids []sim.EntityID
	ids = w.AppendAllUnits(ids)
	n := 0
	for _, id := range ids {
		ur := w.UnitTypes.Row(id)
		if ur != -1 && w.UnitTypes.TypeID[ur] == dSoldier {
			n++
		}
	}
	return n
}

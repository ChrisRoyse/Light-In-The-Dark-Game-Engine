package litd

// #281 live AI-domain FSV. SoT = the headless sim read back after stepping:
// footman ENTITIES owned by the AI player (counted straight from the sim
// stores), the AI context's scheduler clock (Domain.Context(p).Now()), and a
// deterministic world fingerprint over all entities. Proves the public
// AttachAI/PauseAI surface drives a real, isolated, deterministic AI domain —
// not a recorded stub.

import (
	"fmt"
	"hash/fnv"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
)

const (
	lvFootman uint16 = 0 // 50 gold, 2 food, 40 train-ticks
	lvBarrack uint16 = 1 // trains footmen, provides 20 food
)

func lvDefs() []data.Unit {
	return []data.Unit{
		{ID: "footman", Life: 100, CollisionSize: 16, Costs: []int64{50, 0}, TrainTicks: 40, FoodCost: 2},
		{ID: "barracks", Life: 1000, CollisionSize: 64, FoodProvided: 20, Trains: []uint16{lvFootman}},
	}
}

func ptAt(x, y int32) fixed.Vec2 { return fixed.Vec2{X: fixed.FromInt(x), Y: fixed.FromInt(y)} }

// lvGame builds a headless game with an economy, the footman/barracks defs, a
// barracks for the AI player, and gold to spare.
func lvGame(t *testing.T, aiPlayer uint8) *Game {
	t.Helper()
	w := sim.NewWorld(sim.Caps{Units: 64})
	if !w.BindEconomy(4) || !w.BindUnitDefs(lvDefs()) {
		t.Fatal("bind failed")
	}
	if _, ok := w.SpawnFromTable(lvBarrack, aiPlayer, aiPlayer, ptAt(200, 200)); !ok {
		t.Fatal("barracks spawn failed")
	}
	w.SetResource(aiPlayer, 0, 500) // gold, not the limiter
	return newGame(w)
}

// liveFootmen counts completed footman entities owned by player (the sim SoT).
func liveFootmen(g *Game, player uint8) int {
	var ids []sim.EntityID
	ids = g.w.AppendAllUnits(ids)
	n := 0
	for _, id := range ids {
		or := g.w.Owners.Row(id)
		ur := g.w.UnitTypes.Row(id)
		if or == -1 || ur == -1 {
			continue
		}
		if g.w.Owners.Player[or] == player && g.w.UnitTypes.TypeID[ur] == lvFootman {
			n++
		}
	}
	return n
}

// worldFingerprint hashes every entity's (id, owner, type) — a deterministic
// SoT digest for cross-run equality.
func worldFingerprint(g *Game) uint64 {
	var ids []sim.EntityID
	ids = g.w.AppendAllUnits(ids) // ascending → stable
	h := fnv.New64a()
	for _, id := range ids {
		or := g.w.Owners.Row(id)
		ur := g.w.UnitTypes.Row(id)
		owner, typ := -1, -1
		if or != -1 {
			owner = int(g.w.Owners.Player[or])
		}
		if ur != -1 {
			typ = int(g.w.UnitTypes.TypeID[ur])
		}
		fmt.Fprintf(h, "id%d/p%d/t%d;", uint32(id), owner, typ)
	}
	return h.Sum64()
}

// trainAI trains its type up to target, then idles. It counts its own ticks so
// a test can prove whether/when it ran.
type trainAI struct {
	typeID, target int
	delayTicks     int // train only once its own tick count reaches this
	ticks          int
	issued         int
	trace          []string
}

func (c *trainAI) Tick(view AIView, cmd AICommander) {
	c.ticks++
	if c.ticks < c.delayTicks {
		return
	}
	if c.issued >= c.target {
		return
	}
	have := view.OwnUnitCount(c.typeID)
	if cmd.Train(c.typeID) {
		c.issued++
		c.trace = append(c.trace, fmt.Sprintf("tick%d TRAIN#%d (own=%d diff=%d)",
			c.ticks, c.issued, have, int(view.Difficulty())))
	} else {
		c.trace = append(c.trace, fmt.Sprintf("tick%d REFUSED (own=%d)", c.ticks, have))
	}
}

// TestAIHooksLiveTrainsUnitFSV — the headline. Attach a controller that trains
// one footman; step the headless sim; the footman appears in the sim (SoT), and
// the controller's command trace + final count are identical across two runs.
func TestAIHooksLiveTrainsUnitFSV(t *testing.T) {
	const player uint8 = 1

	run := func(log bool) (footmen int, trace []string, fp uint64) {
		g := lvGame(t, player)
		ctrl := &trainAI{typeID: int(lvFootman), target: 1, delayTicks: 1}
		g.AttachAI(g.Player(int(player)), ctrl, DifficultyNormal)

		if g.aiDomain == nil || g.aiDomain.Context(int(player)) == nil {
			t.Fatal("AttachAI did not install a live AI context")
		}
		t.Logf("BEFORE: footmen=%d attached=%v", liveFootmen(g, player), g.IsAIPlayer(g.Player(int(player))))

		for i := 0; i < 60; i++ {
			g.w.Step()
		}
		footmen = liveFootmen(g, player)
		if log {
			t.Logf("AFTER 60 ticks: footmen=%d ctxNow=%d trace=%v",
				footmen, g.aiDomain.Context(int(player)).Now(), ctrl.trace)
		}
		return footmen, ctrl.trace, worldFingerprint(g)
	}

	f1, tr1, fp1 := run(true)
	f2, tr2, fp2 := run(false)

	if f1 != 1 {
		t.Fatalf("expected exactly 1 trained footman in the sim, got %d", f1)
	}
	if f1 != f2 || fp1 != fp2 || fmt.Sprint(tr1) != fmt.Sprint(tr2) {
		t.Fatalf("non-deterministic: run1 (n=%d fp=%x %v) run2 (n=%d fp=%x %v)", f1, fp1, tr1, f2, fp2, tr2)
	}
	t.Logf("headline OK: 1 footman trained via live AI domain (SoT), deterministic across 2 runs (fp=%x)", fp1)
}

// TestAIHooksLiveAttachTwiceReplacesFSV — edge 1: attaching again replaces the
// controller wholesale. The first controller stops being ticked the moment the
// second is attached (documented behavior: replace, not refuse), deterministic.
func TestAIHooksLiveAttachTwiceReplacesFSV(t *testing.T) {
	const player uint8 = 2
	g := lvGame(t, player)
	c1 := &trainAI{typeID: int(lvFootman), target: 0, delayTicks: 1} // never trains; just counts ticks
	g.AttachAI(g.Player(int(player)), c1, DifficultyNormal)

	for i := 0; i < 10; i++ {
		g.w.Step()
	}
	c1AtReplace := c1.ticks
	t.Logf("c1 ticked %d times before replace", c1AtReplace)
	if c1AtReplace == 0 {
		t.Fatal("c1 never ticked before replace")
	}

	c2 := &trainAI{typeID: int(lvFootman), target: 0, delayTicks: 1}
	g.AttachAI(g.Player(int(player)), c2, DifficultyInsane) // replace
	if g.aiControllers[player] != c2 {
		t.Fatal("controller not replaced at api layer")
	}
	if g.AIDifficulty(g.Player(int(player))) != DifficultyInsane {
		t.Fatal("difficulty not updated on replace")
	}

	for i := 0; i < 10; i++ {
		g.w.Step()
	}
	t.Logf("AFTER replace + 10 ticks: c1.ticks=%d (frozen) c2.ticks=%d (running)", c1.ticks, c2.ticks)
	if c1.ticks != c1AtReplace {
		t.Fatalf("c1 kept ticking after replace: %d → %d (a hook was left behind)", c1AtReplace, c1.ticks)
	}
	if c2.ticks == 0 {
		t.Fatal("replacement c2 never ticked")
	}
	t.Log("edge1 OK: AttachAI twice replaces wholesale — old controller frozen, no leftover hook")
}

// TestAIHooksLivePauseShiftsFSV — edge 2: pausing the AI for 100 ticks shifts
// its scheduler clock and its decisions by EXACTLY 100 ticks (wakes shifted, not
// dropped). The controller trains on its Nth tick; pausing for 100 delays the
// footman's appearance by exactly 100 sim ticks. SoT = footman-appearance tick
// and the AI context clock.
func TestAIHooksLivePauseShiftsFSV(t *testing.T) {
	const player uint8 = 1
	const delay = 20       // controller trains on its 20th AI tick (well after the pause begins)
	const totalTicks = 200 // run both scenarios the same length so clocks compare at a common tick

	// Runs totalTicks sim ticks; returns the sim tick the footman first appears
	// and the AI context clock at the END (a common sim tick for both runs).
	appearance := func(pauseAt, pauseDur int) (appearTick int, ctxNowAtEnd uint32) {
		g := lvGame(t, player)
		ctrl := &trainAI{typeID: int(lvFootman), target: 1, delayTicks: delay}
		g.AttachAI(g.Player(int(player)), ctrl, DifficultyNormal)
		paused := false
		for tick := 1; tick <= totalTicks; tick++ {
			if pauseDur > 0 && tick == pauseAt {
				g.PauseAI(g.Player(int(player)), true)
				paused = true
			}
			if paused && tick == pauseAt+pauseDur {
				g.PauseAI(g.Player(int(player)), false)
				paused = false
			}
			g.w.Step()
			if appearTick == 0 && liveFootmen(g, player) > 0 {
				appearTick = tick
			}
		}
		return appearTick, g.aiDomain.Context(int(player)).Now()
	}

	baseTick, baseNow := appearance(0, 0)       // no pause
	pausedTick, pausedNow := appearance(5, 100) // pause at tick 5 for 100 ticks (before the 20th AI tick)
	t.Logf("pause-shift table (both run %d sim ticks):", totalTicks)
	t.Logf("  unbroken : footman appears at sim tick %3d, AI ctx clock at end = %3d", baseTick, baseNow)
	t.Logf("  paused100: footman appears at sim tick %3d, AI ctx clock at end = %3d", pausedTick, pausedNow)
	t.Logf("  delta    : appearance +%d ticks, ctx clock lag %d", pausedTick-baseTick, int(baseNow)-int(pausedNow))

	if baseTick == 0 || pausedTick == 0 {
		t.Fatalf("footman never appeared (base=%d paused=%d)", baseTick, pausedTick)
	}
	if pausedTick-baseTick != 100 {
		t.Fatalf("pause should shift the footman appearance by exactly 100 ticks; got %d", pausedTick-baseTick)
	}
	if int(baseNow)-int(pausedNow) != 100 {
		t.Fatalf("AI context clock should lag by exactly 100 after a 100-tick pause; got %d", int(baseNow)-int(pausedNow))
	}
	t.Log("edge2 OK: 100-tick pause shifted both the AI clock and the decision by exactly 100 ticks (no drop)")
}

// TestAIHooksLiveSaveResumeFSV — edge 3: the attached AI state (scheduler)
// serializes; round-tripping the domain mid-run and re-installing the controller
// resumes it so the final sim state is identical to an unbroken run. SoT = final
// world fingerprint.
func TestAIHooksLiveSaveResumeFSV(t *testing.T) {
	const player uint8 = 1
	const saveAt = 10

	build := func() (*Game, *trainAI) {
		g := lvGame(t, player)
		ctrl := &trainAI{typeID: int(lvFootman), target: 2, delayTicks: 3}
		g.AttachAI(g.Player(int(player)), ctrl, DifficultyNormal)
		return g, ctrl
	}

	// Unbroken baseline.
	gA, _ := build()
	for i := 0; i < 120; i++ {
		gA.w.Step()
	}
	baseFP := worldFingerprint(gA)
	baseN := liveFootmen(gA, player)
	t.Logf("unbroken: footmen=%d fp=%x", baseN, baseFP)

	// Round-trip the AI domain at saveAt, then continue on the same world.
	gB, ctrlB := build()
	var blob []byte
	for i := 0; i < 120; i++ {
		if i == saveAt {
			blob = gB.aiDomain.Save(nil)
			// Tear the domain down and rebuild it from the blob: a fresh domain
			// with the controller re-installed (controller identity is code, not
			// serialized state — the caller re-attaches), then Load restores the
			// scheduler continuations.
			gB.aiDomain = nil
			gB.w.OnAIPhase = nil
			gB.ensureAIDomain()
			gB.installAIContext(player, ctrlB)
			if err := gB.aiDomain.Load(blob); err != nil {
				t.Fatalf("domain Load: %v", err)
			}
			t.Logf("t%d: domain saved (%d bytes), torn down, rebuilt + reloaded", i, len(blob))
		}
		gB.w.Step()
	}
	resFP := worldFingerprint(gB)
	resN := liveFootmen(gB, player)
	t.Logf("save/resume: footmen=%d fp=%x", resN, resFP)

	if resN != baseN || resFP != baseFP {
		t.Fatalf("resumed run diverged: unbroken (n=%d fp=%x) vs resumed (n=%d fp=%x)", baseN, baseFP, resN, resFP)
	}
	t.Logf("edge3 OK: AI domain save/restore mid-run resumed identically (footmen=%d, fp=%x)", resN, resFP)
}

// TestAIHooksLiveDefeatedNoOpFSV — edge 4: AttachAI to a defeated player is a
// no-op (no context, not marked AI). SoT = domain context absence + IsAIPlayer.
func TestAIHooksLiveDefeatedNoOpFSV(t *testing.T) {
	const player uint8 = 3
	g := lvGame(t, player)

	// Defeat the player and let phase 6 latch it.
	if !g.w.SetDefeat(player) {
		t.Fatal("SetDefeat refused")
	}
	g.w.Step()
	t.Logf("player result after defeat = %d (want %d=Lost)", g.w.PlayerResult(player), sim.ResultLost)
	if g.w.PlayerResult(player) != sim.ResultLost {
		t.Fatal("defeat did not latch")
	}

	ctrl := &trainAI{typeID: int(lvFootman), target: 1, delayTicks: 1}
	g.AttachAI(g.Player(int(player)), ctrl, DifficultyNormal)

	t.Logf("after AttachAI to defeated: IsAIPlayer=%v contextInstalled=%v",
		g.IsAIPlayer(g.Player(int(player))), g.aiDomain != nil && g.aiDomain.Context(int(player)) != nil)
	if g.IsAIPlayer(g.Player(int(player))) {
		t.Fatal("AttachAI to a defeated player marked it AI — must be a no-op")
	}
	if g.aiDomain != nil && g.aiDomain.Context(int(player)) != nil {
		t.Fatal("AttachAI to a defeated player installed a context — must be a no-op")
	}
	t.Log("edge4 OK: AttachAI to a defeated player is a no-op")
}

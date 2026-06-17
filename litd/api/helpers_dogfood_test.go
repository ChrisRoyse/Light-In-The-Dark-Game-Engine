package litd_test

// D4 helpers dogfood FSV (#258). These tests live in the EXTERNAL litd_test
// package precisely to prove the dogfood claim from the consumer's seat:
// they import litd/api/helpers (which imports only the public litd surface)
// and exercise it exactly as a game developer would. The Source of Truth is
// always the sim state read back after each helper call (unit/resource/
// destructable stores, the scheduler sleep queue) — never the helper's
// return value. A *litd.Game is obtained via the test seam
// litd.NewGameForTest (export_test.go), since no public constructor exists
// yet (#201).

import (
	"testing"
	"time"

	litd "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api/helpers"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim/path"
)

// dogfoodDefs binds the unit codes the melee tables reference plus a plain
// soldier, so g.UnitType(code) resolves and CreateUnit spawns.
func dogfoodDefs() []data.Unit {
	return []data.Unit{
		{ID: "htow", Life: 1500, FoodProvided: 12, DepotMask: 0b1},
		{ID: "hpea", Life: 220},
		{ID: "hfoo", Life: 420},
		{ID: "ugol", Life: 1500, FoodProvided: 11, DepotMask: 0b1},
		{ID: "uaco", Life: 220},
		{ID: "ushd", Life: 340},
	}
}

// dogfoodGame builds a headless world with a walkable grid, bound unit defs,
// and a player economy, returning the world (for SoT reads) and the Game.
func dogfoodGame(t *testing.T) (*sim.World, *litd.Game) {
	t.Helper()
	w := sim.NewWorld(sim.Caps{Units: 64})
	grid := path.NewGrid()
	for y := int32(10); y < 90; y++ {
		for x := int32(10); x < 90; x++ {
			grid.SetFlags(x, y, path.Walkable)
		}
	}
	w.SetGrid(grid)
	if !w.BindEconomy(4) || !w.BindUnitDefs(dogfoodDefs()) {
		t.Fatal("dogfoodGame: BindEconomy/BindUnitDefs failed")
	}
	return w, litd.NewGameForTest(w)
}

func cell(cx, cy int32) litd.Vec2 { return litd.Vec2{X: float64(cx*32 + 16), Y: float64(cy*32 + 16)} }

func stepWorld(w *sim.World, n int) {
	for i := 0; i < n; i++ {
		w.Step()
	}
}

// TestHelpersPolledWaitDogfoodFSV — helpers.PolledWait suspends the running
// thread (delegating to the #377 surface) and the JASS guard holds.
// SoT: scheduler PendingSleepers + observed resume tick.
func TestHelpersPolledWaitDogfoodFSV(t *testing.T) {
	w, g := dogfoodGame(t)

	var ticks []uint32
	g.Run(func(*litd.Thread) {
		ticks = append(ticks, w.Tick()) // tick 0
		helpers.PolledWait(0)           // edge (1): guard, same-tick, no record
		ticks = append(ticks, w.Tick()) // still tick 0
		helpers.PolledWait(100 * time.Millisecond)
		ticks = append(ticks, w.Tick()) // resume tick 2
	})
	t.Logf("FSV PolledWait after Run: ticks=%v pendingSleepers=%d suspended=%d", ticks, w.Sched.PendingSleepers(), g.SuspendedThreadCount())
	if len(ticks) != 2 || ticks[0] != 0 || ticks[1] != 0 {
		t.Fatalf("PolledWait(0) suspended or skipped: ticks=%v, want [0 0]", ticks)
	}
	if w.Sched.PendingSleepers() != 1 {
		t.Fatalf("after the positive wait, pendingSleepers=%d, want 1", w.Sched.PendingSleepers())
	}
	stepWorld(w, 2)
	t.Logf("FSV PolledWait after 2 ticks: ticks=%v pendingSleepers=%d", ticks, w.Sched.PendingSleepers())
	if len(ticks) != 3 || ticks[2] != 2 {
		t.Fatalf("PolledWait(100ms) resumed at ticks=%v, want third entry tick 2", ticks)
	}

	// Edge: positive wait outside any thread is a loud fail-closed panic
	// (no thread to suspend) — never a silent skip.
	func() {
		defer func() {
			if r := recover(); r == nil {
				t.Fatal("helpers.PolledWait(d>0) outside a thread did not panic")
			} else {
				t.Logf("FSV PolledWait outside thread panicked as required: %v", r)
			}
		}()
		helpers.PolledWait(time.Second)
	}()
	// But the guard wait outside a thread is still a clean no-op.
	helpers.PolledWait(0)
}

// TestHelpersCreateUnitsDogfoodFSV — CreateUnits spawns exactly n units of
// the right type/owner; n=0 yields an empty non-nil slice (edge 3).
// SoT: the sim unit store (owner + type counts), not the returned slice len.
func TestHelpersCreateUnitsDogfoodFSV(t *testing.T) {
	w, g := dogfoodGame(t)
	p := g.Player(1)
	foo := g.UnitType("hfoo")

	// Edge (3): n=0 — empty, non-nil, nothing spawned.
	before := len(w.AppendAllUnits(nil))
	zero := helpers.CreateUnits(g, 0, p, foo, cell(30, 30), litd.Deg(0))
	t.Logf("FSV CreateUnits(0): slice=%v isNil=%v simUnitsBefore=%d simUnitsAfter=%d", zero, zero == nil, before, len(w.AppendAllUnits(nil)))
	if zero == nil || len(zero) != 0 {
		t.Fatalf("CreateUnits(0) = %v (nil=%v), want empty non-nil slice", zero, zero == nil)
	}
	if len(w.AppendAllUnits(nil)) != before {
		t.Fatal("CreateUnits(0) spawned a unit")
	}

	// Happy path: n=3.
	wantType, ok := w.UnitTypeID("hfoo")
	if !ok {
		t.Fatal("hfoo not bound")
	}
	got := helpers.CreateUnits(g, 3, p, foo, cell(30, 30), litd.Deg(0))
	owned := 0
	for _, id := range w.AppendAllUnits(nil) {
		if r := w.Owners.Row(id); r >= 0 && w.Owners.Player[r] == 1 {
			if tr := w.UnitTypes.Row(id); tr >= 0 && w.UnitTypes.TypeID[tr] == wantType {
				owned++
			}
		}
	}
	t.Logf("FSV CreateUnits(3): returned=%d sim hfoo owned-by-p1=%d", len(got), owned)
	if len(got) != 3 {
		t.Fatalf("CreateUnits(3) returned %d handles, want 3", len(got))
	}
	if owned != 3 {
		t.Fatalf("sim shows %d hfoo units for player 1, want 3 (SoT)", owned)
	}
	for i, u := range got {
		if !u.Valid() {
			t.Fatalf("unit %d invalid", i)
		}
	}
}

// TestHelpersWeightedChoiceDogfoodFSV — WeightedChoice draws from the sim
// PRNG; degenerate inputs are deterministic (edge 4); the distribution
// honors the weights and is reproducible from a fixed seed.
func TestHelpersWeightedChoiceDogfoodFSV(t *testing.T) {
	_, g := dogfoodGame(t)

	// Edge (4): all-zero weights — defined, deterministic, PRNG untouched.
	z1 := helpers.WeightedChoice(g, []int{0, 0, 0})
	z2 := helpers.WeightedChoice(g, []int{0, 0, 0})
	empty := helpers.WeightedChoice(g, nil)
	t.Logf("FSV WeightedChoice degenerate: allZero run1=%d run2=%d empty=%d (want -1,-1,-1)", z1, z2, empty)
	if z1 != -1 || z2 != -1 || empty != -1 {
		t.Fatalf("degenerate WeightedChoice = (%d,%d,%d), want all -1", z1, z2, empty)
	}

	// Only index 1 has positive weight → always 1, never consuming a draw
	// that could pick a zero-weight slot.
	for i := 0; i < 20; i++ {
		if got := helpers.WeightedChoice(g, []int{0, 5, 0}); got != 1 {
			t.Fatalf("single-positive-weight choice = %d, want 1", got)
		}
	}

	// Distribution + reproducibility: weights [1,3] over a fixed seed must
	// (a) be byte-identical across two fresh same-seed worlds, and (b) favor
	// index 1 roughly 3:1.
	seq := func() ([]int, [2]int) {
		w := sim.NewWorld(sim.Caps{Units: 1})
		gg := litd.NewGameForTest(w)
		gg.SetRandomSeed(0x5151)
		var s []int
		var counts [2]int
		for i := 0; i < 400; i++ {
			c := helpers.WeightedChoice(gg, []int{1, 3})
			s = append(s, c)
			counts[c]++
		}
		return s, counts
	}
	s1, c1 := seq()
	s2, c2 := seq()
	t.Logf("FSV WeightedChoice [1,3] x400: counts=%v (idx1/idx0=%.2f, want ~3); two-run identical=%v", c1, float64(c1[1])/float64(c1[0]), equalInts(s1, s2))
	if !equalInts(s1, s2) {
		t.Fatal("WeightedChoice not reproducible from a fixed seed (PRNG determinism broken)")
	}
	ratio := float64(c1[1]) / float64(c1[0])
	if ratio < 2.3 || ratio > 3.9 {
		t.Fatalf("WeightedChoice [1,3] ratio idx1/idx0 = %.2f, want ~3 (2.3..3.9)", ratio)
	}
	if c2[0]+c2[1] != 400 {
		t.Fatalf("draw count mismatch: %v", c2)
	}
}

// TestHelpersRandomItemTypeDogfoodFSV — resolves a drawn code to a bound
// ItemType; empty list → null; deterministic from a fixed seed.
func TestHelpersRandomItemTypeDogfoodFSV(t *testing.T) {
	g := litd.NewGameForTest(mustItemWorld(t))

	if it := helpers.RandomItemType(g, nil); !it.IsZero() {
		t.Fatal("RandomItemType(empty) must be the null ItemType")
	}
	g.SetRandomSeed(7)
	a := helpers.RandomItemType(g, []string{"pmana", "phea"})
	g2 := litd.NewGameForTest(mustItemWorld(t))
	g2.SetRandomSeed(7)
	b := helpers.RandomItemType(g2, []string{"pmana", "phea"})
	t.Logf("FSV RandomItemType: a.IsZero=%v b.IsZero=%v sameSeedEqual=%v", a.IsZero(), b.IsZero(), a == b)
	if a.IsZero() {
		t.Fatal("RandomItemType resolved to null for a bound code")
	}
	if a != b {
		t.Fatal("RandomItemType not deterministic from a fixed seed")
	}
}

// TestHelpersGateElevatorDogfoodFSV — a Gate blocks pathing over its cell; an
// Elevator does not. SoT: the pathing grid CellWalkable flag.
func TestHelpersGateElevatorDogfoodFSV(t *testing.T) {
	w, g := dogfoodGame(t)
	const gx, gy = 40, 40
	const ex, ey = 42, 42

	beforeGate := w.Grid.CellWalkable(gx, gy)
	gate := helpers.Gate(g, helpers.WidgetOptions{Type: 1, Pos: cell(gx, gy), Life: 100, Footprint: 1})
	afterGate := w.Grid.CellWalkable(gx, gy)

	beforeElev := w.Grid.CellWalkable(ex, ey)
	elev := helpers.Elevator(g, helpers.WidgetOptions{Type: 2, Pos: cell(ex, ey), Life: 100})
	afterElev := w.Grid.CellWalkable(ex, ey)

	t.Logf("FSV Gate cell walkable: before=%v after=%v (want true→false); Elevator: before=%v after=%v (want true→true)",
		beforeGate, afterGate, beforeElev, afterElev)
	if !gate.Valid() || !elev.Valid() {
		t.Fatal("Gate/Elevator returned invalid handles")
	}
	if !beforeGate || afterGate {
		t.Fatalf("Gate did not block its cell: before=%v after=%v", beforeGate, afterGate)
	}
	if !beforeElev || !afterElev {
		t.Fatalf("Elevator wrongly blocked its cell: before=%v after=%v", beforeElev, afterElev)
	}
	// Killing the gate frees the cell again (the destructable is the SoT).
	gate.Kill()
	t.Logf("FSV Gate after Kill: cellWalkable=%v (want true)", w.Grid.CellWalkable(gx, gy))
	if !w.Grid.CellWalkable(gx, gy) {
		t.Fatal("killing the gate must free its pathing cell")
	}
}

// TestHelpersTransmitDogfoodFSV — Transmit prefixes the speaker and routes a
// timed message to each recipient via the UI sink (the resolved event SoT).
func TestHelpersTransmitDogfoodFSV(t *testing.T) {
	_, g := dogfoodGame(t)
	var got []litd.UIMessageEvent
	g.OnUI(func(ev litd.UIMessageEvent) { got = append(got, ev) })

	to := []litd.Player{g.Player(0), g.Player(1)}
	helpers.Transmit(g, to, "Commander", "Hold the line.", 8*time.Second)
	t.Logf("FSV Transmit: events=%d first=%+v", len(got), firstUI(got))
	if len(got) != 2 {
		t.Fatalf("Transmit produced %d UI events, want 2 (one per recipient)", len(got))
	}
	if got[0].Text != "Commander: Hold the line." {
		t.Fatalf("Transmit text = %q, want speaker-prefixed line", got[0].Text)
	}
	if got[0].Duration != 8 {
		t.Fatalf("Transmit duration = %v, want 8s", got[0].Duration)
	}
	// Empty recipients → no-op.
	got = got[:0]
	helpers.Transmit(g, nil, "X", "y", time.Second)
	if len(got) != 0 {
		t.Fatalf("Transmit(nil recipients) emitted %d events, want 0", len(got))
	}
}

// --- small external-test utilities ---

func equalInts(a, b []int) bool {
	if len(a) != len(b) {
		return false
	}
	for i := range a {
		if a[i] != b[i] {
			return false
		}
	}
	return true
}

func firstUI(s []litd.UIMessageEvent) litd.UIMessageEvent {
	if len(s) == 0 {
		return litd.UIMessageEvent{}
	}
	return s[0]
}

func dogfoodItemDefs() []data.Item {
	return []data.Item{{ID: "pmana", Charges: 1}, {ID: "phea", Charges: 1}}
}

func mustItemWorld(t *testing.T) *sim.World {
	t.Helper()
	w := sim.NewWorld(sim.Caps{Units: 1})
	if !w.BindItemDefs(dogfoodItemDefs()) {
		t.Fatal("BindItemDefs failed")
	}
	return w
}

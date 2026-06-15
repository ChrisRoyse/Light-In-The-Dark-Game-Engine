package litd

// #229 destructable + doodad public-API FSV. SoT = the sim state read back
// after each API call (g.w.Destructable*/Doodads.Count/Grid cell flags) —
// proving the public verbs write real, hashed, replay-safe sim state.

import (
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim/path"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/statehash"
)

func destWorld() *sim.World {
	w := sim.NewWorld(sim.Caps{Units: 8})
	g := path.NewGrid()
	for y := int32(18); y < 46; y++ {
		for x := int32(18); x < 46; x++ {
			g.SetFlags(x, y, path.Walkable)
		}
	}
	w.SetGrid(g)
	return w
}

func cellVec(cx, cy int32) Vec2 {
	return Vec2{X: float64(cx*32 + 16), Y: float64(cy*32 + 16)}
}

// TestDestructableLifecycleFSV — Create/SetLife/Kill/Resurrect write real sim
// state, and Kill frees the pathing cell the API claims to.
func TestDestructableLifecycleFSV(t *testing.T) {
	w := destWorld()
	g := newGame(w)
	const cx, cy = 30, 30

	d := g.CreateDestructable(DestructableOptions{Type: 7, Pos: cellVec(cx, cy), Life: 100, BlocksPathing: true, Footprint: 1})
	if !d.Valid() {
		t.Fatal("CreateDestructable returned invalid handle")
	}
	t.Logf("FSV after create: simLife=%d simDead=%v cellWalkable=%v (want 100/false/false)",
		w.DestructableLife(d.id), w.DestructableDead(d.id), w.Grid.CellWalkable(cx, cy))
	if d.Life() != 100 || d.MaxLife() != 100 || d.Dead() {
		t.Fatalf("api readback wrong: life=%d max=%d dead=%v", d.Life(), d.MaxLife(), d.Dead())
	}
	if w.Grid.CellWalkable(cx, cy) {
		t.Fatal("blocking destructable should occupy its cell")
	}

	// SetLife clamps to MaxLife.
	d.SetLife(500)
	t.Logf("FSV SetLife(500) -> sim=%d (want clamp 100)", w.DestructableLife(d.id))
	if d.Life() != 100 {
		t.Fatalf("SetLife not clamped: %d", d.Life())
	}

	d.SetInvulnerable(true)
	if !w.DestructableInvulnerable(d.id) {
		t.Fatal("SetInvulnerable did not write sim state")
	}

	d.Kill()
	t.Logf("FSV after Kill: simDead=%v cellWalkable=%v (want true/true)", w.DestructableDead(d.id), w.Grid.CellWalkable(cx, cy))
	if !d.Dead() || !w.Grid.CellWalkable(cx, cy) {
		t.Fatal("Kill must set dead and free the pathing cell")
	}

	d.Resurrect()
	if d.Dead() || d.Life() != 100 || w.Grid.CellWalkable(cx, cy) {
		t.Fatal("Resurrect must revive at full life and re-block")
	}

	// edge: zero-value handle verbs are safe no-ops.
	var zero Destructable
	zero.Kill()
	zero.SetLife(5)
	if zero.Life() != 0 || zero.Dead() || zero.Valid() {
		t.Fatal("zero-value Destructable must be a safe no-op")
	}
}

// TestDoodadPromotionFSV — issue edges (1)/(2): untouched doodads cost 0 sim
// entities; the first touch promotes (+1), a second touch promotes nothing.
func TestDoodadPromotionFSV(t *testing.T) {
	w := destWorld()
	g := newGame(w)

	// Edge (1): hold 1,000 doodad handles, touch none — 0 sim entities.
	for i := 0; i < 1000; i++ {
		_ = g.Doodad(i)
	}
	t.Logf("FSV untouched: promoted doodad count=%d (want 0)", w.Doodads.Count())
	if w.Doodads.Count() != 0 {
		t.Fatalf("untouched doodads must cost 0 sim entities, got %d", w.Doodads.Count())
	}

	// Edge (2): first touch promotes (+1).
	d := g.Doodad(42)
	if d.Promoted() {
		t.Fatal("doodad should not be promoted before first touch")
	}
	d.SetAnimation(3)
	t.Logf("FSV after first touch: count=%d promoted=%v (want 1/true)", w.Doodads.Count(), d.Promoted())
	if w.Doodads.Count() != 1 || !d.Promoted() {
		t.Fatalf("first touch must promote exactly one: count=%d", w.Doodads.Count())
	}

	// Second touch of the SAME doodad promotes nothing (+0).
	d.Show(false)
	g.Doodad(42).SetAnimation(9)
	t.Logf("FSV after second touch: count=%d (want still 1)", w.Doodads.Count())
	if w.Doodads.Count() != 1 {
		t.Fatalf("re-touch must not promote again: count=%d", w.Doodads.Count())
	}

	// A different doodad promotes a new row (+1).
	g.Doodad(7).Show(true)
	if w.Doodads.Count() != 2 {
		t.Fatalf("distinct doodad must promote: count=%d", w.Doodads.Count())
	}

	// edge: invalid handle is a safe no-op (no promotion).
	g.Doodad(-1).Show(true)
	if w.Doodads.Count() != 2 {
		t.Fatalf("invalid doodad must not promote: count=%d", w.Doodads.Count())
	}
}

// TestDoodadPromotionOrderDeterministic — issue edge (3): two API runs that
// touch doodads in identical order produce identical state hashes.
func TestDoodadPromotionOrderDeterministic(t *testing.T) {
	run := func() uint64 {
		w := destWorld()
		g := newGame(w)
		g.Doodad(5).Show(false)
		g.Doodad(2).SetAnimation(1)
		g.Doodad(9).Show(true)
		reg := sim.NewHashRegistry()
		var snap statehash.Snapshot
		w.HashState(reg, &snap)
		return snap.Top
	}
	a, b := run(), run()
	t.Logf("FSV doodad promotion-order determinism: run1=%016x run2=%016x", a, b)
	if a != b {
		t.Fatalf("identical promotion order diverged: %016x vs %016x", a, b)
	}
}

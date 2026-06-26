package render_test

// Real-sim end-to-end FSV for the fog texture (#159). This drives an actual
// sim.World — not a synthetic FogGridSource — through render.FogTexture and
// verifies the resulting buffer bytes against the sim's own FogStateAt truth.
// It is the integration counterpart to fog_test.go: there the input is a fake
// grid; here the source of truth is the live sim store, read back two ways
// (sim state and render luminance) and required to agree at every cell.

import (
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/render"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim/path"
)

func fogWorld(t *testing.T) *sim.World {
	t.Helper()
	w := sim.NewWorld(sim.Caps{Units: 64})
	g := path.NewGrid()
	for y := int32(0); y < path.GridSize; y++ {
		for x := int32(0); x < path.GridSize; x++ {
			g.SetFlags(x, y, path.Walkable|path.Buildable|path.Flyable)
		}
	}
	w.SetGrid(g)
	if !w.BindUnitDefs([]data.Unit{
		{ID: "scout", Life: 100, SightDay: fixed.FromInt(360), SightNight: fixed.FromInt(160), CollisionSize: 16, Pathing: data.PathingGround},
	}) {
		t.Fatal("BindUnitDefs failed")
	}
	w.SetTimeOfDay(12 * fixed.One)
	w.SuspendTimeOfDay(true)
	return w
}

func fogSpawn(t *testing.T, w *sim.World, player uint8, cellX, cellY int32) sim.EntityID {
	t.Helper()
	id, ok := w.CreateUnit(sim.CellCenter(cellY*path.GridSize+cellX), 0)
	if !ok ||
		!w.Owners.Add(w.Ents, id, player, player, player) ||
		!w.UnitTypes.Add(w.Ents, id, 0) ||
		!w.Collisions.Add(w.Ents, id, 1, sim.PathGround) ||
		!w.Healths.Add(w.Ents, id, 100*fixed.One, 0, 0, 0) {
		t.Fatalf("spawn failed id=%d ok=%v", id, ok)
	}
	return id
}

func fogMove(t *testing.T, w *sim.World, id sim.EntityID, cellX, cellY int32) {
	t.Helper()
	r := w.Transforms.Row(id)
	if r == -1 {
		t.Fatalf("entity %d has no transform", id)
	}
	w.Transforms.Pos[r] = sim.CellCenter(cellY*path.GridSize + cellX)
}

// expectedLum maps a sim fog state to the luminance render must produce.
func expectedLum(state uint8) uint8 {
	switch state {
	case sim.FogVisible:
		return render.FogVisibleLum
	case sim.FogExplored:
		return render.FogExploredLum
	default:
		return render.FogHiddenLum
	}
}

// TestFogRealSimRevealLeaveFSV — scout reveals its cell, then leaves. The
// render buffer must mirror the sim grid exactly at every sampled fog cell,
// before and after, for the reveal-then-leave transition (the spec's core
// scenario). blend=1 isolates the mapping from temporal smoothing.
func TestFogRealSimRevealLeaveFSV(t *testing.T) {
	w := fogWorld(t)
	const player uint8 = 0
	scout := fogSpawn(t, w, player, 40, 40)

	originFx, originFy := int32(40)/sim.FogCellPathingSize, int32(40)/sim.FogCellPathingSize
	farFx, farFy := int32(80)/sim.FogCellPathingSize, int32(80)/sim.FogCellPathingSize

	f := render.NewFogTexture(1)

	// BEFORE any vision recompute: grid all hidden → buffer all hidden.
	w.RecomputeVisibility()
	f.Update(w, 1<<player)
	simOrigin := w.FogStateAt(player, originFx, originFy)
	bufOrigin := f.At(int(originFx), int(originFy))
	t.Logf("FSV reveal origin fog(%d,%d) sim=%d buf=%d wantBuf=%d", originFx, originFy, simOrigin, bufOrigin, expectedLum(simOrigin))
	if simOrigin != sim.FogVisible {
		t.Fatalf("scout cell should be visible, sim=%d", simOrigin)
	}
	if bufOrigin != expectedLum(simOrigin) {
		t.Fatalf("origin buf=%d want %d (sim state %d)", bufOrigin, expectedLum(simOrigin), simOrigin)
	}

	// Far cell out of sight → hidden in both sim and buffer.
	simFar := w.FogStateAt(player, farFx, farFy)
	bufFar := f.At(int(farFx), int(farFy))
	t.Logf("FSV reveal far fog(%d,%d) sim=%d buf=%d", farFx, farFy, simFar, bufFar)
	if simFar != sim.FogHidden || bufFar != render.FogHiddenLum {
		t.Fatalf("far cell should be hidden: sim=%d buf=%d", simFar, bufFar)
	}

	// Move scout to the far cell, recompute. Origin becomes EXPLORED (seen
	// before), far becomes VISIBLE. Buffer must follow.
	fogMove(t, w, scout, 80, 80)
	w.RecomputeVisibility()
	f.Update(w, 1<<player)

	simOrigin2 := w.FogStateAt(player, originFx, originFy)
	bufOrigin2 := f.At(int(originFx), int(originFy))
	simFar2 := w.FogStateAt(player, farFx, farFy)
	bufFar2 := f.At(int(farFx), int(farFy))
	t.Logf("FSV leave origin sim=%d buf=%d wantBuf=%d | far sim=%d buf=%d wantBuf=%d",
		simOrigin2, bufOrigin2, expectedLum(simOrigin2), simFar2, bufFar2, expectedLum(simFar2))
	if simOrigin2 != sim.FogExplored {
		t.Fatalf("origin should be explored after leaving, sim=%d", simOrigin2)
	}
	if bufOrigin2 != render.FogExploredLum {
		t.Fatalf("origin buf=%d want explored %d", bufOrigin2, render.FogExploredLum)
	}
	if simFar2 != sim.FogVisible || bufFar2 != render.FogVisibleLum {
		t.Fatalf("far should be visible: sim=%d buf=%d", simFar2, bufFar2)
	}
}

// TestFogRealSimFullGridConsistencyFSV — across the ENTIRE 128² buffer, every
// cell's luminance must equal expectedLum(sim.FogStateAt) for that cell. This
// is the exhaustive SoT cross-check: render bytes vs sim grid, no sampling.
func TestFogRealSimFullGridConsistencyFSV(t *testing.T) {
	w := fogWorld(t)
	const player uint8 = 0
	fogSpawn(t, w, player, 64, 64)
	fogSpawn(t, w, player, 16, 96)
	w.RecomputeVisibility()

	f := render.NewFogTexture(1)
	f.Update(w, 1<<player)

	var hidden, explored, visible, mismatch int
	for y := int32(0); y < int32(f.Size()); y++ {
		for x := int32(0); x < int32(f.Size()); x++ {
			st := w.FogStateAt(player, x, y)
			want := expectedLum(st)
			got := f.At(int(x), int(y))
			switch st {
			case sim.FogVisible:
				visible++
			case sim.FogExplored:
				explored++
			default:
				hidden++
			}
			if got != want {
				if mismatch < 5 {
					t.Errorf("cell(%d,%d) sim=%d buf=%d want %d", x, y, st, got, want)
				}
				mismatch++
			}
		}
	}
	t.Logf("FSV full-grid consistency hidden=%d explored=%d visible=%d mismatch=%d (total=%d)",
		hidden, explored, visible, mismatch, f.Size()*f.Size())
	if mismatch != 0 {
		t.Fatalf("render fog buffer disagrees with sim grid in %d cells", mismatch)
	}
	if visible == 0 {
		t.Fatalf("two scouts should make some cells visible, got 0")
	}
}

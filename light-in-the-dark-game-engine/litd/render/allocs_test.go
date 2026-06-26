package render

// #151 consolidated zero-alloc frame gate. The per-subsystem *ZeroAllocFSV
// tests (sync, precull, batching, instances, fog, anim, billboards, minimap,
// vfx, decals, camera, daynight, stats) each prove one stage allocates nothing;
// this test proves the composed frame path — frame-sync, pre-cull, batch-build,
// per-instance buffers (uniforms), and fog update — allocates nothing when the
// stages run together in one steady frame at the worst-case workload (500
// visible units), catching allocation that only appears in the COMPOSITION
// (shared scratch reused across stages). HUD-update and audio-dispatch 0-alloc
// are gated in their own packages (litd/render/hud, litd/audio); the whole-path
// guarantee is the union.
//
// GL submission is not exercised: every allocating-prone step (mirror, cull,
// visible staging, instance fill, fog blend) runs CPU-side and needs no GL
// context. Spawn / map-load / resize are explicitly allowed to allocate
// (Batching §10.4); this gate is steady-state only — every buffer is pre-warmed
// before the measured run.

import (
	"testing"

	litasset "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/asset"
	"github.com/g3n/engine/core"
	"github.com/g3n/engine/geometry"
	"github.com/g3n/engine/graphic"
	"github.com/g3n/engine/material"
	"github.com/g3n/engine/math32"
)

func TestFrameSyncBatchPathZeroAllocFSV(t *testing.T) {
	const n = 500

	// Stage 1 fixture: a 500-slot snapshot for the frame-sync (mirror) step.
	snap := &Snapshot{
		Present:   make([]bool, n),
		EntityKey: make([]uint32, n),
		Model:     make([]ModelID, n),
	}
	for i := 0; i < n; i++ {
		snap.Present[i] = true
		snap.EntityKey[i] = uint32(i + 1)
		snap.Model[i] = ModelID(i%5 + 1)
	}
	mirror := make([]MirrorEntry, 0, n)

	// Stage 2 fixture: a ground-footprint culler over the same 500 positions.
	var culler GroundFootprintCuller
	culler.SetFootprint(squareFootprint(4000), 100)
	culler.Reserve(n)
	xs := make([]float32, n)
	zs := make([]float32, n)
	for i := 0; i < n; i++ {
		xs[i] = float32(i*8 - 2000)
		zs[i] = float32((i%50)*80 - 2000)
	}

	// Stage 3 fixture: a material batcher holding 500 meshes; the per-frame
	// step re-stages the visible set into frame scratch.
	batcher := NewMaterialBatcher("frame", 1, n)
	key := BatchMaterialKey{Atlas: "vigil.atlas.png", Preset: litasset.AtlasPresetHigh, Shader: "standard"}
	mat := material.NewStandard(&math32.Color{R: 1, G: 1, B: 1})
	geom := geometry.NewBox(1, 1, 1)
	nodes := make([]core.INode, 0, n)
	for i := 0; i < n; i++ {
		mesh := graphic.NewMesh(geom, nil)
		if _, _, err := batcher.Add(key, mat, mesh); err != nil {
			t.Fatalf("batcher add %d: %v", i, err)
		}
		nodes = append(nodes, mesh)
	}

	// Stage 4 fixture: a per-instance transform/team-color buffer (the "uniforms"
	// step) for the same 500 units.
	imesh := graphic.NewInstancedMesh(geometry.NewPlane(1, 1), material.NewBasic(), 0)
	ibuf, err := NewInstanceBuffer(imesh, n)
	if err != nil {
		t.Fatalf("instance buffer: %v", err)
	}
	if err := ibuf.SetCount(n); err != nil {
		t.Fatalf("instance setcount: %v", err)
	}
	transforms := make([]math32.Matrix4, n)
	for i := range transforms {
		transforms[i].MakeTranslation(float32(i%40), 0, float32(i/40))
	}

	// Stage 5 fixture: a fog texture updated from a stable grid each frame.
	grid := newFakeFogGrid()
	for i := int32(0); i < FogTexSize; i++ {
		grid.set(0, i, i, fogStateVisible)
		grid.set(0, i, (i+1)%FogTexSize, fogStateExplored)
	}
	fog := NewFogTexture(1)
	fog.Update(grid, 1<<0) // warm + converge

	// frame runs the composed steady path once: sync -> pre-cull -> batch build
	// -> per-instance buffer (uniforms) -> fog update.
	var syncErr, cullVis, cullCull = error(nil), 0, 0
	frame := func() {
		var err error
		mirror, err = snap.MirrorEntries(mirror)
		if err != nil {
			syncErr = err
		}
		vis, cull := culler.Cull(xs, zs)
		cullVis, cullCull = len(vis), len(cull)
		batcher.ResetFrameVisible()
		for _, node := range nodes {
			if err := batcher.StageVisible(node); err != nil {
				panic(err)
			}
		}
		ibuf.BeginFrame()
		for i := range transforms {
			if err := ibuf.SetInstance(i, &transforms[i], i%TeamColorSlots); err != nil {
				panic(err)
			}
		}
		fog.Update(grid, 1<<0)
	}

	frame() // warm every buffer
	if syncErr != nil {
		t.Fatalf("warm frame sync: %v", syncErr)
	}

	allocs := testing.AllocsPerRun(500, frame)
	isnap := ibuf.Snapshot(0, 12, 999)
	t.Logf("FSV composed frame zero-alloc: %d units -> mirror=%d cull(vis=%d,cull=%d) batchVisible=%d instances=%d fog=%dB allocs/op=%v",
		n, len(mirror), cullVis, cullCull, batcher.Snapshot().FrameVisibleCount, isnap.Count, len(fog.buf), allocs)

	if allocs != 0 {
		t.Fatalf("composed frame path (sync+precull+batch+uniforms+fog) allocates %v/op, want 0", allocs)
	}
	// Sanity: the composed path actually did the work (not a no-op zero).
	if len(mirror) != n || batcher.Snapshot().FrameVisibleCount != n || isnap.Count != n {
		t.Fatalf("frame produced wrong counts: mirror=%d visible=%d instances=%d, want %d", len(mirror), batcher.Snapshot().FrameVisibleCount, isnap.Count, n)
	}
	if cullVis+cullCull != n {
		t.Fatalf("cull dropped entities: vis=%d cull=%d, want sum %d", cullVis, cullCull, n)
	}
}

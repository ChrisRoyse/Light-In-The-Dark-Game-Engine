package render

import (
	"strings"
	"testing"

	litasset "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/asset"
	"github.com/g3n/engine/core"
	"github.com/g3n/engine/geometry"
	"github.com/g3n/engine/graphic"
	"github.com/g3n/engine/material"
	"github.com/g3n/engine/math32"
)

func TestMaterialBatcherGroupsSharedMaterialsFSV(t *testing.T) {
	b := NewMaterialBatcher("units", 2, 8)
	matA := material.NewStandard(&math32.Color{R: 0.8, G: 0.2, B: 0.1})
	matB := material.NewStandard(&math32.Color{R: 0.1, G: 0.2, B: 0.8})
	keyA := BatchMaterialKey{Atlas: "vigil.atlas.png", Preset: litasset.AtlasPresetHigh, Shader: "standard"}
	keyB := BatchMaterialKey{Atlas: "ember.atlas.png", Preset: litasset.AtlasPresetHigh, Shader: "standard"}
	geom := geometry.NewBox(1, 1, 1)

	for i, tc := range []struct {
		key BatchMaterialKey
		mat material.IMaterial
	}{
		{keyA, matA},
		{keyB, matB},
		{keyA, matA},
		{keyB, matB},
	} {
		mesh := graphic.NewMesh(geom, material.NewStandard(&math32.Color{R: float32(i+1) / 10, G: 0.4, B: 0.6}))
		group, rebind, err := b.Add(tc.key, tc.mat, mesh)
		if err != nil {
			t.Fatal(err)
		}
		t.Logf("FSV batch add i=%d group=%s order=%d BEFORE=%+v AFTER=%+v", i, tc.key.Atlas, group.RenderOrder, rebind.Before, rebind.After)
		if rebind.Before.MaterialInstances != 1 || rebind.After.MaterialInstances != 1 {
			t.Fatalf("mesh should rebind from one unique material to one shared material: %+v", rebind)
		}
	}

	snap := b.Snapshot()
	t.Logf("FSV batch grouping AFTER snapshot=%+v rootChildren=%d", snap, len(b.Root.Children()))
	if snap.GroupCount != 2 || snap.Entities != 4 || snap.Graphics != 4 || snap.GraphicMaterials != 4 || snap.MaterialInstances != 2 {
		t.Fatalf("batch snapshot wrong: %+v", snap)
	}
	if len(b.Root.Children()) != 2 || snap.Groups[0].Entities != 2 || snap.Groups[1].Entities != 2 {
		t.Fatalf("groups not parented by material: children=%d groups=%+v", len(b.Root.Children()), snap.Groups)
	}
	if snap.Groups[0].RenderOrder == snap.Groups[1].RenderOrder {
		t.Fatalf("material groups should get distinct render-order buckets: %+v", snap.Groups)
	}
}

func TestRebindSubtreeMaterialFSV(t *testing.T) {
	root := core.NewNode()
	root.SetName("imported-glb")
	meshA := graphic.NewMesh(geometry.NewBox(1, 1, 1), material.NewStandard(&math32.Color{R: 1, G: 0, B: 0}))
	meshB := graphic.NewMesh(geometry.NewSphere(0.5, 8, 8), material.NewStandard(&math32.Color{R: 0, G: 0, B: 1}))
	root.Add(meshA)
	root.Add(meshB)
	before := CountMaterialInstances(root)

	shared := material.NewStandard(&math32.Color{R: 0.9, G: 0.9, B: 0.9})
	rebind, err := RebindSubtreeMaterial(root, shared)
	if err != nil {
		t.Fatal(err)
	}
	after := CountMaterialInstances(root)
	t.Logf("FSV import rebind BEFORE=%+v REBIND=%+v AFTER=%+v", before, rebind, after)
	if before.MaterialInstances != 2 || rebind.Before != before || after.MaterialInstances != 1 || rebind.After != after {
		t.Fatalf("subtree material rebind did not collapse imported materials: before=%+v rebind=%+v after=%+v", before, rebind, after)
	}
	for _, child := range root.Children() {
		igr := child.(graphic.IGraphic)
		if got := igr.GetGraphic().Materials()[0].IMaterial().GetMaterial(); got != shared.GetMaterial() {
			t.Fatalf("child not rebound to shared material: got=%p want=%p", got, shared.GetMaterial())
		}
	}
}

func TestMaterialBatcherRejectsCloneFSV(t *testing.T) {
	b := NewMaterialBatcher("clone-check", 1, 2)
	key := BatchMaterialKey{Atlas: "vigil.atlas.png", Preset: litasset.AtlasPresetHigh, Shader: "standard"}
	shared := material.NewStandard(&math32.Color{R: 1, G: 1, B: 1})
	if _, _, err := b.Add(key, shared, graphic.NewMesh(geometry.NewBox(1, 1, 1), nil)); err != nil {
		t.Fatal(err)
	}
	before := b.Snapshot()
	clone := material.NewStandard(&math32.Color{R: 1, G: 1, B: 1})
	_, _, err := b.Add(key, clone, graphic.NewMesh(geometry.NewBox(1, 1, 1), nil))
	after := b.Snapshot()
	t.Logf("FSV clone assert BEFORE=%+v AFTER=%+v err=%v", before, after, err)
	if err == nil || !strings.Contains(err.Error(), "different material instance") {
		t.Fatalf("clone material was not rejected: err=%v", err)
	}
	if after.Entities != before.Entities || after.MaterialInstances != before.MaterialInstances {
		t.Fatalf("failed clone add mutated batch state: before=%+v after=%+v", before, after)
	}
	if snap, err := AssertMaterialInstanceCeiling(b.Root, 0); err == nil {
		t.Fatalf("ceiling 0 accepted live material instances? snap=%+v", snap)
	} else {
		t.Logf("FSV material ceiling BEFORE max=0 snapshot=%+v AFTER err=%v", snap, err)
	}
}

func TestMaterialBatcherEdgesFSV(t *testing.T) {
	mat := material.NewStandard(&math32.Color{R: 1, G: 1, B: 1})
	key := BatchMaterialKey{Atlas: "vigil.atlas.png", Preset: litasset.AtlasPresetHigh, Shader: "standard"}
	var nilBatcher *MaterialBatcher
	if _, _, err := nilBatcher.Add(key, mat, graphic.NewMesh(geometry.NewBox(1, 1, 1), nil)); err == nil {
		t.Fatal("nil batcher accepted")
	} else {
		t.Logf("FSV nil batcher AFTER err=%v", err)
	}
	b := NewMaterialBatcher("edges", 1, 1)
	if _, _, err := b.Add(BatchMaterialKey{Preset: litasset.AtlasPresetHigh, Shader: "standard"}, mat, graphic.NewMesh(geometry.NewBox(1, 1, 1), nil)); err == nil {
		t.Fatal("empty atlas key accepted")
	} else {
		t.Logf("FSV empty atlas key BEFORE=%+v AFTER err=%v", b.Snapshot(), err)
	}
	if _, _, err := b.Add(BatchMaterialKey{Atlas: "vigil.atlas.png", Preset: litasset.AtlasPreset("tiny"), Shader: "standard"}, mat, graphic.NewMesh(geometry.NewBox(1, 1, 1), nil)); err == nil {
		t.Fatal("invalid preset accepted")
	} else {
		t.Logf("FSV invalid preset BEFORE=%+v AFTER err=%v", b.Snapshot(), err)
	}
	if _, _, err := b.Add(key, nil, graphic.NewMesh(geometry.NewBox(1, 1, 1), nil)); err == nil {
		t.Fatal("nil material accepted")
	} else {
		t.Logf("FSV nil material BEFORE=%+v AFTER err=%v", b.Snapshot(), err)
	}
	beforeNoGraphics := b.Snapshot()
	if _, _, err := b.Add(key, mat, core.NewNode()); err == nil {
		t.Fatal("node without graphics accepted")
	} else {
		t.Logf("FSV no-graphics node BEFORE=%+v AFTER err=%v", b.Snapshot(), err)
	}
	afterNoGraphics := b.Snapshot()
	if afterNoGraphics.GroupCount != beforeNoGraphics.GroupCount || afterNoGraphics.Entities != beforeNoGraphics.Entities || afterNoGraphics.MaterialInstances != beforeNoGraphics.MaterialInstances {
		t.Fatalf("no-graphics failure mutated batcher: before=%+v after=%+v", beforeNoGraphics, afterNoGraphics)
	}
	if err := b.StageVisible(nil); err == nil {
		t.Fatal("nil visible node accepted")
	} else {
		t.Logf("FSV nil visible node AFTER err=%v", err)
	}
}

func TestMaterialBatcherFrameScratchZeroAllocFSV(t *testing.T) {
	b := NewMaterialBatcher("steady", 1, 100)
	key := BatchMaterialKey{Atlas: "vigil.atlas.png", Preset: litasset.AtlasPresetHigh, Shader: "standard"}
	mat := material.NewStandard(&math32.Color{R: 1, G: 1, B: 1})
	nodes := make([]core.INode, 0, 100)
	geom := geometry.NewBox(1, 1, 1)
	for i := 0; i < 100; i++ {
		mesh := graphic.NewMesh(geom, nil)
		if _, _, err := b.Add(key, mat, mesh); err != nil {
			t.Fatal(err)
		}
		nodes = append(nodes, mesh)
	}
	allocs := testing.AllocsPerRun(1000, func() {
		b.ResetFrameVisible()
		for _, node := range nodes {
			if err := b.StageVisible(node); err != nil {
				panic(err)
			}
		}
	})
	t.Logf("FSV steady batch frame BEFORE cap=%d nodes=%d AFTER visible=%d allocs/run=%.3f", cap(b.frameVisible), len(nodes), b.Snapshot().FrameVisibleCount, allocs)
	if allocs != 0 {
		t.Fatalf("steady-state visible staging allocated: %.3f", allocs)
	}
	if b.Snapshot().FrameVisibleCount != 100 {
		t.Fatalf("visible frame list not populated: %+v", b.Snapshot())
	}
	if err := b.StageVisible(graphic.NewMesh(geom, nil)); err == nil {
		t.Fatal("visible capacity overflow accepted")
	} else {
		t.Logf("FSV visible capacity overflow BEFORE count=%d cap=%d AFTER err=%v", b.Snapshot().FrameVisibleCount, b.Snapshot().FrameVisibleCapacity, err)
	}
}

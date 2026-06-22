package render

import (
	"testing"

	"github.com/g3n/engine/geometry"
	"github.com/g3n/engine/graphic"
	"github.com/g3n/engine/material"
	"github.com/g3n/engine/math32"
)

func TestInstanceBufferFSV(t *testing.T) {
	mesh := graphic.NewInstancedMesh(geometry.NewPlane(1, 1), material.NewBasic(), 0)
	buf, err := NewInstanceBuffer(mesh, 4)
	if err != nil {
		t.Fatal(err)
	}
	if err := buf.SetCount(3); err != nil {
		t.Fatal(err)
	}
	for i, slot := range []int{0, 1, NeutralTeamSlot} {
		var m math32.Matrix4
		m.MakeTranslation(float32(i*2), 0, float32(-i))
		if err := buf.SetInstance(i, &m, slot); err != nil {
			t.Fatal(err)
		}
	}
	snap := buf.Snapshot(0, 1, 2)
	t.Logf("FSV instance buffer AFTER snapshot=%+v", snap)
	if snap.Count != 3 || snap.Capacity != 4 || snap.UpdateBytes != 3*InstanceUpdateBytes {
		t.Fatalf("snapshot counters wrong: %+v", snap)
	}
	if snap.MeshTransformBytes != 3*InstanceTransformBytes || snap.MeshTeamColorBytes != 3*InstanceTeamColorBytes {
		t.Fatalf("mesh byte counters wrong: %+v", snap)
	}
	if len(snap.Samples) != 3 || snap.Samples[1].Slot != 1 || snap.Samples[1].X != 2 || snap.Samples[1].Z != -1 {
		t.Fatalf("samples wrong: %+v", snap.Samples)
	}
	want, _ := TeamColor(1)
	var got math32.Color
	mesh.InstanceTeamColor(1, &got)
	if got != want {
		t.Fatalf("mesh team color readback = %+v, want %+v", got, want)
	}
}

func TestInstanceBufferEdgesFSV(t *testing.T) {
	if _, err := NewInstanceBuffer(nil, 1); err == nil {
		t.Fatal("nil mesh accepted")
	} else {
		t.Logf("FSV nil mesh AFTER err=%v", err)
	}
	if _, err := NewInstanceBuffer(graphic.NewInstancedMesh(geometry.NewPlane(1, 1), material.NewBasic(), 0), -1); err == nil {
		t.Fatal("negative capacity accepted")
	} else {
		t.Logf("FSV negative capacity AFTER err=%v", err)
	}

	mesh := graphic.NewInstancedMesh(geometry.NewPlane(1, 1), material.NewBasic(), 0)
	buf, err := NewInstanceBuffer(mesh, 2)
	if err != nil {
		t.Fatal(err)
	}
	before := buf.Snapshot()
	if err := buf.SetCount(3); err == nil {
		t.Fatal("overflow count accepted")
	} else {
		t.Logf("FSV overflow count BEFORE=%+v AFTER err=%v snapshot=%+v", before, err, buf.Snapshot())
	}
	if buf.Snapshot().Count != before.Count {
		t.Fatalf("overflow count mutated state: before=%+v after=%+v", before, buf.Snapshot())
	}
	if err := buf.SetCount(1); err != nil {
		t.Fatal(err)
	}
	var m math32.Matrix4
	m.Identity()
	if err := buf.SetInstance(1, &m, 0); err == nil {
		t.Fatal("out-of-range instance accepted")
	} else {
		t.Logf("FSV bad index BEFORE count=%d AFTER err=%v", buf.Snapshot().Count, err)
	}
	if err := buf.SetInstance(0, nil, 0); err == nil {
		t.Fatal("nil transform accepted")
	} else {
		t.Logf("FSV nil transform AFTER err=%v", err)
	}
	before = buf.Snapshot(0)
	if err := buf.SetInstance(0, &m, TeamColorSlots); err == nil {
		t.Fatal("invalid team slot accepted")
	} else {
		t.Logf("FSV invalid team BEFORE=%+v AFTER err=%v snapshot=%+v", before, err, buf.Snapshot(0))
	}
	if buf.Snapshot(0).UpdateBytes != before.UpdateBytes {
		t.Fatalf("invalid team mutated update bytes: before=%+v after=%+v", before, buf.Snapshot(0))
	}
	if err := buf.SetTeamColorZone(TeamColorZone{MinU: 0.7, MinV: 0, MaxU: 0.4, MaxV: 1}); err == nil {
		t.Fatal("invalid zone accepted")
	} else {
		t.Logf("FSV invalid zone AFTER err=%v", err)
	}
}

func TestInstanceBufferSteadyZeroAllocFSV(t *testing.T) {
	const n = 1000
	mesh := graphic.NewInstancedMesh(geometry.NewPlane(1, 1), material.NewBasic(), 0)
	buf, err := NewInstanceBuffer(mesh, n)
	if err != nil {
		t.Fatal(err)
	}
	if err := buf.SetCount(n); err != nil {
		t.Fatal(err)
	}
	transforms := make([]math32.Matrix4, n)
	for i := range transforms {
		transforms[i].MakeTranslation(float32(i%40), 0, float32(i/40))
	}
	allocs := testing.AllocsPerRun(200, func() {
		buf.BeginFrame()
		for i := range transforms {
			if err := buf.SetInstance(i, &transforms[i], i%TeamColorSlots); err != nil {
				panic(err)
			}
		}
	})
	snap := buf.Snapshot(0, 12, 999)
	t.Logf("FSV instance buffer steady AFTER snapshot=%+v allocs/run=%.3f", snap, allocs)
	if allocs != 0 {
		t.Fatalf("steady instance updates allocated: %.3f", allocs)
	}
	if snap.Count != n || snap.UpdateBytes != n*InstanceUpdateBytes {
		t.Fatalf("steady snapshot wrong: %+v", snap)
	}
}

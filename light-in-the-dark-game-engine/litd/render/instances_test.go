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

func TestRigidOnlyInstancingPlanFSV(t *testing.T) {
	plan, err := PlanRigidOnlyInstancing(440, 12, 60)
	if err != nil {
		t.Fatal(err)
	}
	t.Logf("FSV rigid-only floor battle500 AFTER plan=%+v", plan)
	if plan.Variant != InstancingVariantRigidOnlyFloor || !plan.RigidInstanced || plan.SkinnedInstanced {
		t.Fatalf("policy flags wrong: %+v", plan)
	}
	if plan.BaselineWorldDraws != 500 || plan.FloorWorldDraws != 72 || plan.RecoveredDraws != 428 {
		t.Fatalf("battle500 draw math wrong: %+v", plan)
	}
	if plan.Classes[0].FloorDraws != 12 || plan.Classes[1].FloorDraws != 60 {
		t.Fatalf("class draw math wrong: %+v", plan.Classes)
	}
	if SkinnedInstancingFollowupIssue != 308 {
		t.Fatalf("skinned follow-up issue constant = %d, want 308", SkinnedInstancingFollowupIssue)
	}
	if plan.SkinnedFollowupIssue != SkinnedInstancingFollowupIssue {
		t.Fatalf("skinned follow-up issue not recorded: %+v", plan)
	}

	stress, err := PlanRigidOnlyInstancing(880, 12, 120)
	if err != nil {
		t.Fatal(err)
	}
	t.Logf("FSV rigid-only floor battle1000 AFTER plan=%+v", stress)
	if stress.BaselineWorldDraws != 1000 || stress.FloorWorldDraws != 132 || stress.RecoveredDraws != 868 {
		t.Fatalf("battle1000 draw math wrong: %+v", stress)
	}
}

func TestRigidOnlyInstancingPlanEdgesFSV(t *testing.T) {
	empty, err := PlanRigidOnlyInstancing(0, 0, 0)
	if err != nil {
		t.Fatal(err)
	}
	t.Logf("FSV rigid-only floor empty BEFORE zero counts AFTER plan=%+v", empty)
	if empty.BaselineWorldDraws != 0 || empty.FloorWorldDraws != 0 || empty.RigidInstanced {
		t.Fatalf("empty plan wrong: %+v", empty)
	}

	moreTypes, err := PlanRigidOnlyInstancing(3, 9, 2)
	if err != nil {
		t.Fatal(err)
	}
	t.Logf("FSV rigid-only floor more-types AFTER plan=%+v", moreTypes)
	if moreTypes.FloorWorldDraws != 5 || moreTypes.Classes[0].FloorDraws != 3 {
		t.Fatalf("more-types plan should cap rigid draws at instance count: %+v", moreTypes)
	}

	cases := []struct {
		name    string
		rigid   int
		types   int
		skinned int
	}{
		{name: "negative-rigid", rigid: -1, types: 1, skinned: 0},
		{name: "negative-types", rigid: 1, types: -1, skinned: 0},
		{name: "negative-skinned", rigid: 1, types: 1, skinned: -1},
		{name: "rigid-without-type", rigid: 1, types: 0, skinned: 0},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			if got, err := PlanRigidOnlyInstancing(tc.rigid, tc.types, tc.skinned); err == nil {
				t.Fatalf("invalid plan accepted: %+v", got)
			} else {
				t.Logf("FSV rigid-only floor invalid %s AFTER err=%v", tc.name, err)
			}
		})
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

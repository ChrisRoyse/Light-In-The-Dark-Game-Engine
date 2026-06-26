package render

import (
	"fmt"

	"github.com/g3n/engine/graphic"
	"github.com/g3n/engine/math32"
)

const (
	InstanceTransformFloats = 16
	InstanceTeamColorFloats = 3
	InstanceTransformBytes  = InstanceTransformFloats * 4
	InstanceTeamColorBytes  = InstanceTeamColorFloats * 4
	InstanceUpdateBytes     = InstanceTransformBytes + InstanceTeamColorBytes
)

const SkinnedInstancingFollowupIssue = 308

type InstancingVariant string

const (
	InstancingVariantRigidOnlyFloor InstancingVariant = "rigid-only-floor"
)

type InstancingPolicySnapshot struct {
	Variant              InstancingVariant        `json:"variant"`
	RigidInstanced       bool                     `json:"rigidInstanced"`
	SkinnedInstanced     bool                     `json:"skinnedInstanced"`
	SkinnedStrategy      string                   `json:"skinnedStrategy"`
	SkinnedFollowupIssue int                      `json:"skinnedFollowupIssue"`
	BaselineWorldDraws   int                      `json:"baselineWorldDraws"`
	FloorWorldDraws      int                      `json:"floorWorldDraws"`
	RecoveredDraws       int                      `json:"recoveredDraws"`
	Classes              []InstancingClassSummary `json:"classes"`
}

type InstancingClassSummary struct {
	Class         string `json:"class"`
	Instances     int    `json:"instances"`
	ModelTypes    int    `json:"modelTypes"`
	BaselineDraws int    `json:"baselineDraws"`
	FloorDraws    int    `json:"floorDraws"`
	Instanced     bool   `json:"instanced"`
}

func PlanRigidOnlyInstancing(rigidInstances, rigidModelTypes, skinnedInstances int) (InstancingPolicySnapshot, error) {
	if rigidInstances < 0 {
		return InstancingPolicySnapshot{}, fmt.Errorf("rigid instance count is negative")
	}
	if rigidModelTypes < 0 {
		return InstancingPolicySnapshot{}, fmt.Errorf("rigid model type count is negative")
	}
	if skinnedInstances < 0 {
		return InstancingPolicySnapshot{}, fmt.Errorf("skinned instance count is negative")
	}
	if rigidInstances > 0 && rigidModelTypes == 0 {
		return InstancingPolicySnapshot{}, fmt.Errorf("rigid model type count is zero for %d rigid instances", rigidInstances)
	}

	rigidFloorDraws := rigidModelTypes
	if rigidFloorDraws > rigidInstances {
		rigidFloorDraws = rigidInstances
	}
	rigidBaselineDraws := rigidInstances
	baseline := rigidBaselineDraws + skinnedInstances
	floor := rigidFloorDraws + skinnedInstances
	return InstancingPolicySnapshot{
		Variant:              InstancingVariantRigidOnlyFloor,
		RigidInstanced:       rigidInstances > 0,
		SkinnedInstanced:     false,
		SkinnedStrategy:      "per-draw via AnimDriver until GLB skinning sink and VAT/cohort evaluation land",
		SkinnedFollowupIssue: SkinnedInstancingFollowupIssue,
		BaselineWorldDraws:   baseline,
		FloorWorldDraws:      floor,
		RecoveredDraws:       baseline - floor,
		Classes: []InstancingClassSummary{
			{
				Class:         "rigid",
				Instances:     rigidInstances,
				ModelTypes:    rigidModelTypes,
				BaselineDraws: rigidBaselineDraws,
				FloorDraws:    rigidFloorDraws,
				Instanced:     rigidInstances > 0,
			},
			{
				Class:         "skinned",
				Instances:     skinnedInstances,
				ModelTypes:    skinnedInstances,
				BaselineDraws: skinnedInstances,
				FloorDraws:    skinnedInstances,
				Instanced:     false,
			},
		},
	}, nil
}

type InstanceBufferSnapshot struct {
	Count              int              `json:"count"`
	Capacity           int              `json:"capacity"`
	UpdateBytes        int              `json:"updateBytes"`
	TotalUpdateBytes   uint64           `json:"totalUpdateBytes"`
	TransformBytes     int              `json:"transformBytes"`
	TeamColorBytes     int              `json:"teamColorBytes"`
	MeshTransformBytes int              `json:"meshTransformBytes"`
	MeshTeamColorBytes int              `json:"meshTeamColorBytes"`
	Samples            []InstanceSample `json:"samples,omitempty"`
}

type InstanceSample struct {
	Index int        `json:"index"`
	Slot  int        `json:"slot"`
	Color [3]float32 `json:"color"`
	X     float32    `json:"x"`
	Y     float32    `json:"y"`
	Z     float32    `json:"z"`
}

type InstanceBuffer struct {
	mesh       *graphic.InstancedMesh
	count      int
	transforms []math32.Matrix4
	teamSlots  []int
	teamColors []math32.Color

	updateBytes      int
	totalUpdateBytes uint64
}

func NewInstanceBuffer(mesh *graphic.InstancedMesh, capacity int) (*InstanceBuffer, error) {
	if mesh == nil || mesh.GetNode() == nil {
		return nil, fmt.Errorf("instance buffer mesh is nil")
	}
	if capacity < 0 {
		return nil, fmt.Errorf("instance buffer capacity is negative")
	}
	buf := &InstanceBuffer{
		mesh:       mesh,
		transforms: make([]math32.Matrix4, capacity),
		teamSlots:  make([]int, capacity),
		teamColors: make([]math32.Color, capacity),
	}
	var ident math32.Matrix4
	ident.Identity()
	neutral, _ := TeamColor(NeutralTeamSlot)
	for i := 0; i < capacity; i++ {
		buf.transforms[i] = ident
		buf.teamSlots[i] = NeutralTeamSlot
		buf.teamColors[i] = neutral
	}
	mesh.SetInstanceCount(capacity)
	for i := 0; i < capacity; i++ {
		mesh.SetInstanceMatrix(i, &buf.transforms[i])
		mesh.SetInstanceTeamColor(i, &buf.teamColors[i])
	}
	mesh.SetInstanceCount(0)
	return buf, nil
}

func (b *InstanceBuffer) Mesh() *graphic.InstancedMesh {
	if b == nil {
		return nil
	}
	return b.mesh
}

func (b *InstanceBuffer) BeginFrame() {
	if b == nil {
		return
	}
	b.updateBytes = 0
}

func (b *InstanceBuffer) SetCount(count int) error {
	if b == nil || b.mesh == nil {
		return fmt.Errorf("instance buffer is nil")
	}
	if count < 0 || count > len(b.transforms) {
		return fmt.Errorf("instance count %d outside capacity %d", count, len(b.transforms))
	}
	b.mesh.SetInstanceCount(count)
	b.count = count
	return nil
}

func (b *InstanceBuffer) SetInstance(index int, transform *math32.Matrix4, teamSlot int) error {
	if b == nil || b.mesh == nil {
		return fmt.Errorf("instance buffer is nil")
	}
	if index < 0 || index >= b.count {
		return fmt.Errorf("instance index %d outside count %d", index, b.count)
	}
	if transform == nil {
		return fmt.Errorf("instance transform is nil")
	}
	color, err := TeamColor(teamSlot)
	if err != nil {
		return err
	}

	b.transforms[index] = *transform
	b.teamSlots[index] = teamSlot
	b.teamColors[index] = color
	b.mesh.SetInstanceMatrix(index, transform)
	b.mesh.SetInstanceTeamColor(index, &color)
	b.updateBytes += InstanceUpdateBytes
	b.totalUpdateBytes += InstanceUpdateBytes
	return nil
}

func (b *InstanceBuffer) SetTeamColorZone(zone TeamColorZone) error {
	if b == nil || b.mesh == nil {
		return fmt.Errorf("instance buffer is nil")
	}
	if zone.MinU < 0 || zone.MinV < 0 || zone.MaxU > 1 || zone.MaxV > 1 || zone.MinU >= zone.MaxU || zone.MinV >= zone.MaxV {
		return fmt.Errorf("invalid team-color zone %+v", zone)
	}
	b.mesh.SetTeamColorZone(zone.MinU, zone.MinV, zone.MaxU, zone.MaxV)
	return nil
}

func (b *InstanceBuffer) SetPresentationScalars(hitFlash, fadeAlpha, fogDim float32, enabled bool) {
	if b == nil || b.mesh == nil {
		return
	}
	b.mesh.SetPresentationScalars(clamp01(hitFlash), clamp01(fadeAlpha), clamp01(fogDim), enabled)
}

func (b *InstanceBuffer) Snapshot(samples ...int) InstanceBufferSnapshot {
	if b == nil {
		return InstanceBufferSnapshot{}
	}
	snap := InstanceBufferSnapshot{
		Count:            b.count,
		Capacity:         len(b.transforms),
		UpdateBytes:      b.updateBytes,
		TotalUpdateBytes: b.totalUpdateBytes,
		TransformBytes:   b.count * InstanceTransformBytes,
		TeamColorBytes:   b.count * InstanceTeamColorBytes,
	}
	if b.mesh != nil {
		snap.MeshTransformBytes = b.mesh.InstanceBufferBytes()
		snap.MeshTeamColorBytes = b.mesh.InstanceTeamColorBufferBytes()
	}
	for _, index := range samples {
		if index < 0 || index >= b.count {
			continue
		}
		color := b.teamColors[index]
		mat := b.transforms[index]
		snap.Samples = append(snap.Samples, InstanceSample{
			Index: index,
			Slot:  b.teamSlots[index],
			Color: [3]float32{color.R, color.G, color.B},
			X:     mat[12],
			Y:     mat[13],
			Z:     mat[14],
		})
	}
	return snap
}

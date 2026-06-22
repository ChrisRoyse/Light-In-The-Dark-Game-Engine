package render

import (
	"fmt"

	litasset "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/asset"
	"github.com/g3n/engine/core"
	"github.com/g3n/engine/graphic"
	"github.com/g3n/engine/material"
)

// BatchMaterialKey is the render-layer batch key for one shared material.
type BatchMaterialKey struct {
	Atlas  string               `json:"atlas"`
	Preset litasset.AtlasPreset `json:"preset"`
	Shader string               `json:"shader"`
}

type MaterialInstanceSnapshot struct {
	Graphics          int `json:"graphics"`
	GraphicMaterials  int `json:"graphicMaterials"`
	MaterialInstances int `json:"materialInstances"`
}

type MaterialRebindSnapshot struct {
	Before MaterialInstanceSnapshot `json:"before"`
	After  MaterialInstanceSnapshot `json:"after"`
}

type MaterialBatchGroupSnapshot struct {
	Key              BatchMaterialKey `json:"key"`
	RenderOrder      int              `json:"renderOrder"`
	Entities         int              `json:"entities"`
	Graphics         int              `json:"graphics"`
	GraphicMaterials int              `json:"graphicMaterials"`
}

type MaterialBatcherSnapshot struct {
	Name                 string                       `json:"name"`
	Groups               []MaterialBatchGroupSnapshot `json:"groups"`
	GroupCount           int                          `json:"groupCount"`
	Entities             int                          `json:"entities"`
	Graphics             int                          `json:"graphics"`
	GraphicMaterials     int                          `json:"graphicMaterials"`
	MaterialInstances    int                          `json:"materialInstances"`
	FrameVisibleCount    int                          `json:"frameVisibleCount"`
	FrameVisibleCapacity int                          `json:"frameVisibleCapacity"`
}

type MaterialBatchGroup struct {
	Key              BatchMaterialKey
	Node             *core.Node
	Material         material.IMaterial
	RenderOrder      int
	Entities         int
	Graphics         int
	GraphicMaterials int
}

type MaterialBatcher struct {
	name         string
	Root         *core.Node
	groups       map[BatchMaterialKey]*MaterialBatchGroup
	order        []BatchMaterialKey
	frameVisible []core.INode
}

func NewMaterialBatcher(name string, groupCapacity, visibleCapacity int) *MaterialBatcher {
	if groupCapacity < 0 {
		groupCapacity = 0
	}
	if visibleCapacity < 0 {
		visibleCapacity = 0
	}
	root := core.NewNode()
	root.SetName(name)
	return &MaterialBatcher{
		name:         name,
		Root:         root,
		groups:       make(map[BatchMaterialKey]*MaterialBatchGroup, groupCapacity),
		order:        make([]BatchMaterialKey, 0, groupCapacity),
		frameVisible: make([]core.INode, 0, visibleCapacity),
	}
}

func (b *MaterialBatcher) Add(key BatchMaterialKey, mat material.IMaterial, node core.INode) (*MaterialBatchGroup, MaterialRebindSnapshot, error) {
	if b == nil || b.Root == nil {
		return nil, MaterialRebindSnapshot{}, fmt.Errorf("material batcher is nil")
	}
	if err := key.validate(); err != nil {
		return nil, MaterialRebindSnapshot{}, err
	}
	if mat == nil || mat.GetMaterial() == nil {
		return nil, MaterialRebindSnapshot{}, fmt.Errorf("batch material is nil")
	}
	if node == nil || node.GetNode() == nil {
		return nil, MaterialRebindSnapshot{}, fmt.Errorf("batch node is nil")
	}

	group := b.groups[key]
	if group != nil && (group.Material == nil || group.Material.GetMaterial() != mat.GetMaterial()) {
		return nil, MaterialRebindSnapshot{}, fmt.Errorf("batch key %s/%s/%s already owns a different material instance", key.Atlas, key.Preset, key.Shader)
	}

	rebind, err := RebindSubtreeMaterial(node, mat)
	if err != nil {
		return nil, MaterialRebindSnapshot{}, err
	}

	if group == nil {
		groupNode := core.NewNode()
		groupNode.SetName("batch:" + key.Atlas + ":" + string(key.Preset) + ":" + key.Shader)
		group = &MaterialBatchGroup{
			Key:         key,
			Node:        groupNode,
			Material:    mat,
			RenderOrder: len(b.order),
		}
		b.groups[key] = group
		b.order = append(b.order, key)
		b.Root.Add(groupNode)
	}
	SetRenderOrderSubtree(node, group.RenderOrder)
	group.Node.Add(node)
	group.Entities++
	group.Graphics += rebind.After.Graphics
	group.GraphicMaterials += rebind.After.GraphicMaterials
	return group, rebind, nil
}

func (b *MaterialBatcher) ResetFrameVisible() {
	if b == nil {
		return
	}
	b.frameVisible = b.frameVisible[:0]
}

func (b *MaterialBatcher) StageVisible(node core.INode) error {
	if b == nil {
		return fmt.Errorf("material batcher is nil")
	}
	if node == nil || node.GetNode() == nil {
		return fmt.Errorf("visible batch node is nil")
	}
	if len(b.frameVisible) == cap(b.frameVisible) {
		return fmt.Errorf("visible batch capacity exceeded: cap=%d", cap(b.frameVisible))
	}
	b.frameVisible = append(b.frameVisible, node)
	return nil
}

func (b *MaterialBatcher) Snapshot() MaterialBatcherSnapshot {
	if b == nil {
		return MaterialBatcherSnapshot{}
	}
	snap := MaterialBatcherSnapshot{
		Name:                 b.name,
		GroupCount:           len(b.order),
		FrameVisibleCount:    len(b.frameVisible),
		FrameVisibleCapacity: cap(b.frameVisible),
	}
	for _, key := range b.order {
		group := b.groups[key]
		if group == nil {
			continue
		}
		snap.Groups = append(snap.Groups, MaterialBatchGroupSnapshot{
			Key:              group.Key,
			RenderOrder:      group.RenderOrder,
			Entities:         group.Entities,
			Graphics:         group.Graphics,
			GraphicMaterials: group.GraphicMaterials,
		})
		snap.Entities += group.Entities
		snap.Graphics += group.Graphics
		snap.GraphicMaterials += group.GraphicMaterials
	}
	if b.Root != nil {
		snap.MaterialInstances = CountMaterialInstances(b.Root).MaterialInstances
	}
	return snap
}

func RebindSubtreeMaterial(node core.INode, mat material.IMaterial) (MaterialRebindSnapshot, error) {
	if node == nil || node.GetNode() == nil {
		return MaterialRebindSnapshot{}, fmt.Errorf("rebind node is nil")
	}
	if mat == nil || mat.GetMaterial() == nil {
		return MaterialRebindSnapshot{}, fmt.Errorf("rebind material is nil")
	}
	before := CountMaterialInstances(node)
	graphics := 0
	traverseNode(node, func(n core.INode) {
		igr, ok := n.(graphic.IGraphic)
		if !ok {
			return
		}
		gr := igr.GetGraphic()
		gr.ClearMaterials()
		gr.AddMaterial(igr, mat, 0, 0)
		graphics++
	})
	if graphics == 0 {
		return MaterialRebindSnapshot{}, fmt.Errorf("rebind subtree contains no graphics")
	}
	after := CountMaterialInstances(node)
	return MaterialRebindSnapshot{Before: before, After: after}, nil
}

func SetRenderOrderSubtree(node core.INode, order int) int {
	if node == nil || node.GetNode() == nil {
		return 0
	}
	graphics := 0
	traverseNode(node, func(n core.INode) {
		if igr, ok := n.(graphic.IGraphic); ok {
			igr.GetGraphic().SetRenderOrder(order)
			graphics++
		}
	})
	return graphics
}

func CountMaterialInstances(node core.INode) MaterialInstanceSnapshot {
	if node == nil || node.GetNode() == nil {
		return MaterialInstanceSnapshot{}
	}
	seen := make(map[*material.Material]struct{})
	snap := MaterialInstanceSnapshot{}
	traverseNode(node, func(n core.INode) {
		igr, ok := n.(graphic.IGraphic)
		if !ok {
			return
		}
		snap.Graphics++
		for _, gm := range igr.GetGraphic().Materials() {
			snap.GraphicMaterials++
			if gm.IMaterial() == nil || gm.IMaterial().GetMaterial() == nil {
				continue
			}
			seen[gm.IMaterial().GetMaterial()] = struct{}{}
		}
	})
	snap.MaterialInstances = len(seen)
	return snap
}

func AssertMaterialInstanceCeiling(node core.INode, max int) (MaterialInstanceSnapshot, error) {
	snap := CountMaterialInstances(node)
	if max < 0 {
		return snap, fmt.Errorf("material instance ceiling must be non-negative")
	}
	if snap.MaterialInstances > max {
		return snap, fmt.Errorf("material instance ceiling exceeded: got %d want <= %d", snap.MaterialInstances, max)
	}
	return snap, nil
}

func (k BatchMaterialKey) validate() error {
	if k.Atlas == "" {
		return fmt.Errorf("batch material key atlas is empty")
	}
	if k.Preset == "" {
		return fmt.Errorf("batch material key preset is empty")
	}
	if _, err := k.Preset.Size(); err != nil {
		return fmt.Errorf("batch material key preset invalid: %w", err)
	}
	if k.Shader == "" {
		return fmt.Errorf("batch material key shader is empty")
	}
	return nil
}

func traverseNode(node core.INode, visit func(core.INode)) {
	visit(node)
	for _, child := range node.Children() {
		traverseNode(child, visit)
	}
}

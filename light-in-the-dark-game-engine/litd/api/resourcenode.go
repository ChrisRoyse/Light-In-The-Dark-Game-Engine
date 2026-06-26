package litd

// Resource-node spawn surface (#401). A resource node (gold mine, harvestable
// tree) is a sim entity carrying a Nodes component; on the public surface it is
// a Unit handle, like any other entity. ResourceNodeType is the id-ref to a
// bound node definition (DefineResourceNodes), mirroring UnitType.

// ResourceNodeType references a bound resource-node definition. The zero value
// is the null type (CreateResourceNode reports invalid on it).
type ResourceNodeType struct {
	ref uint16 // nodeTypeID + 1; 0 = null
}

// IsZero reports whether this is the null resource-node type.
func (t ResourceNodeType) IsZero() bool { return t.ref == 0 }

// Valid reports a non-null resource-node type. Together with IsZero this
// satisfies api.Handle, so a captured ResourceNodeType round-trips through the
// handle-marshal seam (#489).
func (t ResourceNodeType) Valid() bool { return t.ref != 0 }

// ResourceNodeType resolves a node code (data.ResourceNodeType.ID) to its ref.
// Returns the null type for an unknown code or before DefineResourceNodes.
func (g *Game) ResourceNodeType(code string) ResourceNodeType {
	if g == nil || g.w == nil {
		return ResourceNodeType{}
	}
	if id, ok := g.w.ResourceNodeTypeID(code); ok {
		return ResourceNodeType{ref: id + 1}
	}
	return ResourceNodeType{}
}

// CreateResourceNode spawns a resource node of type typ at pos, returning its
// Unit handle (the zero Unit on failure — null/unknown type, the economy not
// defined, a node Resource index past the resource count, or the entity cap
// reached). The node is harvestable by the sim harvest system (#401).
func (g *Game) CreateResourceNode(typ ResourceNodeType, pos Vec2) Unit {
	if g == nil || g.w == nil {
		return Unit{}
	}
	if typ.IsZero() {
		g.reportInvalid("Game.CreateResourceNode (null ResourceNodeType)")
		return Unit{}
	}
	id, ok := g.w.CreateResourceNodeByID(vec(pos), typ.ref-1)
	if !ok {
		g.reportInvalid("Game.CreateResourceNode (spawn failed: economy undefined, bad resource index, or entity cap)")
		return Unit{}
	}
	return Unit{id: id, g: g}
}

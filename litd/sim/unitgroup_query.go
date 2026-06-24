package sim

// Group query-fills (#563): populate a group from a spatial or predicate
// query, reusing the deterministic collision-grid enumerators in
// queries.go (row-major cell visitation, then ascending entity-id order).
// The visitation order IS the resulting group order, so two runs of the
// same scene fill identically (R-UGR-5). All fills clear the target group
// first and are zero-alloc: candidates land in the preallocated
// w.grpScratch buffer, then matching units are appended to the group.

import "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"

// QueryMask filters candidate units during a GroupFill. The zero value
// matches every live unit. Relation bits (Enemy/Ally) are evaluated
// against OfPlayer; class bits are tri-state via the *Only / Exclude*
// pairs. A candidate is kept only if it satisfies every set constraint
// (AND). Max > 0 truncates the result to the first Max matches in
// visitation order (deterministic); Max <= 0 means no limit.
type QueryMask struct {
	OfPlayer uint8 // reference player for Enemy/Ally (else ignored)
	Enemy    bool  // keep only units enemy to OfPlayer
	Ally     bool  // keep only units allied to OfPlayer

	StructuresOnly bool // keep only structures
	ExcludeStruct  bool // drop structures
	FlyingOnly     bool // keep only flying units
	ExcludeFlying  bool // drop flying units

	Max int32 // > 0: cap the fill at this many members
}

// matches reports whether id passes the mask. id is assumed a live owned
// unit (the enumerators guarantee it).
func (w *World) matches(id EntityID, m QueryMask) bool {
	if m.Enemy || m.Ally {
		or := w.Owners.Row(id)
		if or == -1 {
			return false
		}
		p := w.Owners.Player[or]
		if m.Enemy && !w.IsEnemy(m.OfPlayer, p) {
			return false
		}
		if m.Ally && !w.IsAlly(m.OfPlayer, p) {
			return false
		}
	}
	if m.StructuresOnly && !w.UnitIsStructure(id) {
		return false
	}
	if m.ExcludeStruct && w.UnitIsStructure(id) {
		return false
	}
	if m.FlyingOnly && !w.UnitIsFlying(id) {
		return false
	}
	if m.ExcludeFlying && w.UnitIsFlying(id) {
		return false
	}
	return true
}

// fillFrom clears g, then appends every candidate in w.grpScratch that
// passes the mask, honoring Max. Returns the count added. Stale g ⇒ 0.
// grpScratch is expected to already hold the query result.
func (w *World) fillFrom(g GroupID, m QueryMask) int32 {
	if !w.Groups.Alive(g) {
		return 0
	}
	w.Groups.GroupClear(g)
	var n int32
	for _, id := range w.grpScratch {
		if m.Max > 0 && n >= m.Max {
			break
		}
		if !w.matches(id, m) {
			continue
		}
		if w.Groups.GroupAdd(g, id) {
			n++
		}
	}
	return n
}

// GroupFillRadius fills g with the units within radius of center that pass
// mask, in ascending entity-id order. Returns the count added.
func (w *World) GroupFillRadius(g GroupID, center fixed.Vec2, radius fixed.F64, m QueryMask) int32 {
	w.grpScratch = w.AppendUnitsInRange(w.grpScratch[:0], center, radius)
	return w.fillFrom(g, m)
}

// GroupFillRect fills g with the units inside the rect that pass mask.
func (w *World) GroupFillRect(g GroupID, minx, miny, maxx, maxy fixed.F64, m QueryMask) int32 {
	w.grpScratch = w.AppendUnitsInRect(w.grpScratch[:0], minx, miny, maxx, maxy)
	return w.fillFrom(g, m)
}

// GroupFillOwner fills g with every live unit owned by player that passes
// mask, ascending entity-id order. Iterates the Owners store directly (no
// grid), so it is the whole-map owner enumeration.
func (w *World) GroupFillOwner(g GroupID, player uint8, m QueryMask) int32 {
	dst := w.grpScratch[:0]
	o := w.Owners
	for i := int32(0); i < o.count; i++ {
		if o.Player[i] != player {
			continue
		}
		id := o.Entity[i]
		if w.Ents.Alive(id) {
			dst = append(dst, id)
		}
	}
	sortByIndexAsc(dst, 0)
	w.grpScratch = dst
	return w.fillFrom(g, m)
}

// GroupFillType fills g with every live unit of the given unit-type id
// that passes mask, ascending entity-id order. Whole-map enumeration.
func (w *World) GroupFillType(g GroupID, typeID uint16, m QueryMask) int32 {
	dst := w.grpScratch[:0]
	o := w.Owners
	for i := int32(0); i < o.count; i++ {
		id := o.Entity[i]
		if !w.Ents.Alive(id) {
			continue
		}
		ut := w.UnitTypes.Row(id)
		if ut == -1 || w.UnitTypes.TypeID[ut] != typeID {
			continue
		}
		dst = append(dst, id)
	}
	sortByIndexAsc(dst, 0)
	w.grpScratch = dst
	return w.fillFrom(g, m)
}

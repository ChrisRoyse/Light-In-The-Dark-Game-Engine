package litd

import (
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
)

// Persistent unit groups (PRD2 02, #566): the public surface over the sim
// GroupStore. Group is a value handle — {*Game, GroupID} — that resolves
// through the generation-checked store, so a handle to a destroyed or
// recycled group is detectably stale and every method is a safe no-op on
// it (R-API-5). Unlike the snapshot-slice spatial queries (queries.go),
// a Group is a DURABLE, serializable set: it survives save/load (#565)
// and auto-prunes dead members (#564).

// Group is a value handle over a sim GroupID.
type Group struct {
	id sim.GroupID
	g  *Game
}

// NewGroup creates an empty persistent group. The returned handle is
// zero-value (Valid()==false) if the group pool is exhausted.
func (g *Game) NewGroup() Group {
	if g == nil || g.w == nil {
		return Group{}
	}
	return Group{id: g.w.Groups.CreateGroup(), g: g}
}

// Valid reports whether the group still exists.
func (gr Group) Valid() bool {
	return gr.g != nil && gr.g.w != nil && gr.g.w.Groups.Alive(gr.id)
}

// IsZero reports whether this is the zero-value handle.
func (gr Group) IsZero() bool { return gr == Group{} }

// Destroy frees the group. Idempotent; stale ⇒ no-op.
func (gr Group) Destroy() {
	if gr.g != nil && gr.g.w != nil {
		gr.g.w.Groups.DestroyGroup(gr.id)
	}
}

// -- membership -------------------------------------------------------

// Add inserts u (unique, insertion-ordered). No-op if already present,
// stale, or the group's capacity is full.
func (gr Group) Add(u Unit) {
	if gr.g != nil && gr.g.w != nil {
		gr.g.w.Groups.GroupAdd(gr.id, u.id)
	}
}

// Remove deletes u by fast swap (reorders survivors).
func (gr Group) Remove(u Unit) {
	if gr.g != nil && gr.g.w != nil {
		gr.g.w.Groups.GroupRemove(gr.id, u.id)
	}
}

// RemoveOrdered deletes u preserving insertion order (stable, O(n)).
func (gr Group) RemoveOrdered(u Unit) {
	if gr.g != nil && gr.g.w != nil {
		gr.g.w.Groups.GroupRemoveOrdered(gr.id, u.id)
	}
}

// Clear empties the group without destroying it.
func (gr Group) Clear() {
	if gr.g != nil && gr.g.w != nil {
		gr.g.w.Groups.GroupClear(gr.id)
	}
}

// Count returns the member count (0 if stale).
func (gr Group) Count() int {
	if gr.g == nil || gr.g.w == nil {
		return 0
	}
	return int(gr.g.w.Groups.GroupCount(gr.id))
}

// Contains reports whether u is a member.
func (gr Group) Contains(u Unit) bool {
	return gr.g != nil && gr.g.w != nil && gr.g.w.Groups.GroupContains(gr.id, u.id)
}

// First returns the oldest member (deterministic), or the zero Unit if
// the group is empty or stale.
func (gr Group) First() Unit {
	if gr.g == nil || gr.g.w == nil {
		return Unit{}
	}
	id := gr.g.w.Groups.GroupFirst(gr.id)
	if id == 0 {
		return Unit{}
	}
	return Unit{id: id, g: gr.g}
}

// Each visits members in insertion order. It is safe to Remove the
// current unit inside fn (the count is snapshotted; see GroupEach).
func (gr Group) Each(fn func(u Unit)) {
	if gr.g == nil || gr.g.w == nil || fn == nil {
		return
	}
	gr.g.w.Groups.GroupEach(gr.id, func(id sim.EntityID) {
		fn(Unit{id: id, g: gr.g})
	})
}

// -- set algebra (write into the receiver) ----------------------------
//
// The receiver must be distinct from both sources (an aliasing call is a
// no-op on the sim side, leaving the sources intact).

// Union sets the receiver to a ∪ b.
func (gr Group) Union(a, b Group) {
	if gr.g != nil && gr.g.w != nil {
		gr.g.w.Groups.GroupUnion(gr.id, a.id, b.id)
	}
}

// Intersect sets the receiver to a ∩ b.
func (gr Group) Intersect(a, b Group) {
	if gr.g != nil && gr.g.w != nil {
		gr.g.w.Groups.GroupIntersect(gr.id, a.id, b.id)
	}
}

// Difference sets the receiver to a ∖ b.
func (gr Group) Difference(a, b Group) {
	if gr.g != nil && gr.g.w != nil {
		gr.g.w.Groups.GroupDifference(gr.id, a.id, b.id)
	}
}

// CopyFrom sets the receiver to a copy of src.
func (gr Group) CopyFrom(src Group) {
	if gr.g != nil && gr.g.w != nil {
		gr.g.w.Groups.GroupCopy(gr.id, src.id)
	}
}

// -- query-fill (clears the receiver first) ---------------------------

// TriState is a three-way class filter for Query: include any, only the
// class, or exclude the class. The zero value is TriAny (no filter).
type TriState uint8

const (
	TriAny     TriState = iota // no constraint
	TriOnly                    // keep only units of this class
	TriExclude                 // drop units of this class
)

// Query is the option struct for the Fill* verbs (R-API-4): every field's
// zero value is the permissive default, so Query{} matches every live
// unit. Enemy/Ally are evaluated relative to the bound player handle.
type Query struct {
	AliveOnly  bool     // accepted for parity; fills already return only live units
	Enemy      Player   // if bound, keep only enemies of this player
	Ally       Player   // if bound, keep only allies of this player
	Structures TriState // building filter
	Flying     TriState // air-pathing filter
	Max        int      // > 0: cap results (deterministic, by visit order)
}

func triBits(t TriState) (only, exclude bool) {
	return t == TriOnly, t == TriExclude
}

// toMask lowers a Query to the sim QueryMask.
func (q Query) toMask() sim.QueryMask {
	m := sim.QueryMask{Max: int32(q.Max)}
	switch {
	case q.Enemy.g != nil:
		m.OfPlayer = uint8(q.Enemy.idx)
		m.Enemy = true
	case q.Ally.g != nil:
		m.OfPlayer = uint8(q.Ally.idx)
		m.Ally = true
	}
	m.StructuresOnly, m.ExcludeStruct = triBits(q.Structures)
	m.FlyingOnly, m.ExcludeFlying = triBits(q.Flying)
	return m
}

// FillRadius replaces the group's contents with the units within radius
// of center that pass q (ascending entity-id order). Returns the count.
func (gr Group) FillRadius(center Vec2, radius float64, q Query) int {
	if gr.g == nil || gr.g.w == nil {
		return 0
	}
	return int(gr.g.w.GroupFillRadius(gr.id, fixed.Vec2{X: fromFloat(center.X), Y: fromFloat(center.Y)}, fromFloat(radius), q.toMask()))
}

// FillRect replaces the group's contents with the units inside the rect.
func (gr Group) FillRect(min, max Vec2, q Query) int {
	if gr.g == nil || gr.g.w == nil {
		return 0
	}
	return int(gr.g.w.GroupFillRect(gr.id, fromFloat(min.X), fromFloat(min.Y), fromFloat(max.X), fromFloat(max.Y), q.toMask()))
}

// FillOwner replaces the group's contents with player p's units.
func (gr Group) FillOwner(p Player, q Query) int {
	if gr.g == nil || gr.g.w == nil {
		return 0
	}
	return int(gr.g.w.GroupFillOwner(gr.id, uint8(p.idx), q.toMask()))
}

// FillType replaces the group's contents with units of type t.
func (gr Group) FillType(t UnitType, q Query) int {
	if gr.g == nil || gr.g.w == nil || t.ref == 0 {
		return 0
	}
	return int(gr.g.w.GroupFillType(gr.id, t.ref-1, q.toMask()))
}

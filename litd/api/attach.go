package litd

// Handle attachment (#242; hashtable-and-gamecache.md). The common JASS
// idiom is to key a hashtable by GetHandleId(unit) to attach arbitrary
// data to a unit. That collapses to a type-parametric Attachment[V] keyed
// by the unit handle itself. Because the key carries the unit's
// generation, a stale handle to a recycled slot never collides with the
// new occupant — Get on a dead/stale unit returns zero+false, and the
// recycled unit starts clean (R-API-5). This is the generation-safe
// replacement for GetHandleId arithmetic.

import "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"

// Attachment binds values of type V to units. Create with NewAttachment.
// Script-side state (not hashed); a map keyed by the generation-stamped
// EntityID gives O(1), recycle-safe lookup.
type Attachment[V any] struct {
	g *Game
	m map[sim.EntityID]V
}

// NewAttachment returns an empty Attachment[V] bound to g. It is a
// package function (not a Game method) because Go methods cannot take
// type parameters.
func NewAttachment[V any](g *Game) *Attachment[V] {
	return &Attachment[V]{g: g, m: make(map[sim.EntityID]V)}
}

// Valid reports whether the attachment is usable (made by NewAttachment,
// not the nil/zero value). Every noun handle exposes Valid() bool (R-API-5).
func (a *Attachment[V]) Valid() bool { return a != nil && a.g != nil && a.m != nil }

// Set attaches v to u. No-op on an invalid or foreign unit, so a stale
// handle cannot write into a recycled slot's data. JASS: SaveX(GetHandleId(u)).
func (a *Attachment[V]) Set(u Unit, v V) {
	if a == nil || a.m == nil || u.g != a.g || !u.Valid() {
		return
	}
	a.m[u.id] = v
}

// Get returns the value attached to u and whether one is present. A stale
// or recycled handle is invalid → zero+false, never the previous
// occupant's data. JASS: LoadX(GetHandleId(u)) + HaveSavedX.
func (a *Attachment[V]) Get(u Unit) (V, bool) {
	var zero V
	if a == nil || a.m == nil || u.g != a.g || !u.Valid() {
		return zero, false
	}
	v, ok := a.m[u.id]
	return v, ok
}

// Has reports whether u has an attached value.
func (a *Attachment[V]) Has(u Unit) bool {
	_, ok := a.Get(u)
	return ok
}

// Remove drops u's attachment. JASS: RemoveSavedX.
func (a *Attachment[V]) Remove(u Unit) {
	if a == nil || a.m == nil {
		return
	}
	delete(a.m, u.id)
}

// ID returns a stable, opaque identifier for the unit handle — the
// generation-stamped entity id. Distinct live units have distinct IDs; a
// recycled slot yields a different ID than the handle it replaced. The
// replacement for GetHandleId (whose arithmetic is not portable). 0 for
// an invalid handle.
func (u Unit) ID() uint32 {
	if !u.Valid() {
		return 0
	}
	return uint32(u.id)
}

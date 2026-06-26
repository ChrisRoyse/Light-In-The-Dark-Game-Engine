package litd

// Hashtables → generics (#242; hashtable-and-gamecache.md). The JASS
// hashtable is an 80-native type matrix: Save{Integer,Real,Boolean,
// String,UnitHandle,…} × {Load, Have, Remove} keyed by a (parentKey,
// childKey) integer pair. The whole matrix collapses (D3) onto one
// type-parametric Table[V] with a comma-ok Get — the type is the type
// parameter, not a native suffix, so there is exactly one of each verb.
//
// A Table is script-side convenience state (like triggers/forces): it is
// not part of the sim state hash. Deterministic tables that must survive
// save/load belong to the Lua persistence layer (#270); a Go Table is for
// in-process bookkeeping and uses a map purely for O(1) lookup — its
// iteration order is never exposed (R-SIM-2), so the map is safe here.

// tableKey is the (parent, child) composite key. Two ints, no boxing.
type tableKey struct{ parent, child int64 }

// Table is a generic two-key store. V is stored by value (no interface
// boxing per write). Create with NewTable.
type Table[V any] struct {
	m map[tableKey]V
}

// NewTable returns an empty Table[V]. The whole Save*/Load* hashtable
// matrix collapses onto this one generic type. JASS: InitHashtable.
// JASS: InitHashtable
func NewTable[V any]() *Table[V] {
	return &Table[V]{m: make(map[tableKey]V)}
}

// Set stores v under (parent, child). JASS: SaveInteger/SaveReal/
// Save*Handle/… — all one method here. No-op on a nil table.
// JASS: SaveInteger
func (t *Table[V]) Set(parent, child int, v V) {
	if t == nil || t.m == nil {
		return
	}
	t.m[tableKey{int64(parent), int64(child)}] = v
}

// Get returns the value at (parent, child) and whether it was present —
// the comma-ok D5 collapse of the LoadX + HaveSavedX pair. The zero V and
// false on a missing key or a nil table. JASS: LoadInteger + HaveSavedInteger.
// JASS: LoadInteger
func (t *Table[V]) Get(parent, child int) (V, bool) {
	var zero V
	if t == nil || t.m == nil {
		return zero, false
	}
	v, ok := t.m[tableKey{int64(parent), int64(child)}]
	return v, ok
}

// Has reports whether (parent, child) is set. JASS: HaveSavedX.
// JASS: HaveSavedInteger
func (t *Table[V]) Has(parent, child int) bool {
	if t == nil || t.m == nil {
		return false
	}
	_, ok := t.m[tableKey{int64(parent), int64(child)}]
	return ok
}

// Remove deletes (parent, child). JASS: RemoveSavedX. No-op if absent.
// JASS: RemoveSavedInteger
func (t *Table[V]) Remove(parent, child int) {
	if t == nil || t.m == nil {
		return
	}
	delete(t.m, tableKey{int64(parent), int64(child)})
}

// RemoveParent deletes every child under parent. JASS: FlushChildHashtable.
// JASS: FlushChildHashtable
func (t *Table[V]) RemoveParent(parent int) {
	if t == nil || t.m == nil {
		return
	}
	p := int64(parent)
	for k := range t.m {
		if k.parent == p {
			delete(t.m, k)
		}
	}
}

// Clear empties the table. JASS: FlushParentHashtable.
// JASS: FlushParentHashtable
func (t *Table[V]) Clear() {
	if t == nil || t.m == nil {
		return
	}
	for k := range t.m {
		delete(t.m, k)
	}
}

// Len returns the number of stored entries.
func (t *Table[V]) Len() int {
	if t == nil {
		return 0
	}
	return len(t.m)
}

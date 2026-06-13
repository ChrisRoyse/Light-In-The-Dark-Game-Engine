package sim

// HiddenStore (#217): the per-unit "hidden" bit — WC3's ShowUnit(u, false) /
// IsUnitHidden. A hidden unit still exists in the sim (orders, life, position
// all persist); it is only suppressed from rendering and selection. The flag
// has no sim-system consumer today (render/enumeration gating lands with those
// subsystems), but scripts read it back via IsUnitHidden and it must persist
// across save/load, so it is deterministic state.
//
// Modelled as a sparse PRESENCE store: a row exists iff the unit is hidden.
// Almost every unit is visible, so the common case costs zero rows. Mirrors
// UserDataStore minus the value column — membership is the whole signal.
// T2 pattern — see store_unittype.go.

type HiddenStore struct {
	Entity []EntityID

	rowOf []int32
	count int32
}

func NewHiddenStore(rowCap, entityCap int) *HiddenStore {
	if rowCap <= 0 || entityCap <= 0 || rowCap > entityCap {
		panic("sim: store caps must satisfy 0 < rowCap <= entityCap")
	}
	s := &HiddenStore{
		Entity: make([]EntityID, rowCap),
		rowOf:  make([]int32, entityCap),
	}
	for i := range s.rowOf {
		s.rowOf[i] = -1
	}
	return s
}

// set adds id to the hidden set (idempotent). Returns false only when the
// store is full or the handle index is out of range.
func (s *HiddenStore) set(id EntityID) bool {
	idx := id.Index()
	if int(idx) >= len(s.rowOf) {
		return false
	}
	if s.rowOf[idx] != -1 {
		return true // already hidden
	}
	if int(s.count) == len(s.Entity) {
		return false
	}
	r := s.count
	s.Entity[r] = id
	s.rowOf[idx] = r
	s.count++
	return true
}

// Remove drops id from the hidden set (swap-down). Returns false if absent.
func (s *HiddenStore) Remove(id EntityID) bool {
	idx := id.Index()
	if idx >= uint32(len(s.rowOf)) {
		return false
	}
	r := s.rowOf[idx]
	if r == -1 {
		return false
	}
	last := s.count - 1
	if r != last {
		moved := s.Entity[last]
		s.Entity[r] = moved
		s.rowOf[moved.Index()] = r
	}
	s.rowOf[idx] = -1
	s.count--
	return true
}

func (s *HiddenStore) Row(id EntityID) int32 {
	if int(id.Index()) >= len(s.rowOf) {
		return -1
	}
	return s.rowOf[id.Index()]
}

func (s *HiddenStore) Count() int32 { return s.count }

// IsUnitHidden reports whether the unit is currently suppressed from render and
// selection. False for an invalid/absent unit (the safe default — a missing
// unit is not "hidden", it simply is not there).
func (w *World) IsUnitHidden(id EntityID) bool {
	return w.Hiddens.Row(id) != -1
}

// ShowUnit sets the unit's hidden bit: ShowUnit(id, true) reveals (drops the
// row), ShowUnit(id, false) hides (adds it). No-op on a dead unit. Returns
// false on a dead unit or a full store while hiding.
func (w *World) ShowUnit(id EntityID, show bool) bool {
	if !w.Ents.Alive(id) {
		return false
	}
	if show {
		w.Hiddens.Remove(id) // already-visible Remove is a harmless false
		return true
	}
	return w.Hiddens.set(id)
}

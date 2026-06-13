package sim

// maxUnitNameLen bounds a per-instance name override on save load (fail-closed
// against a corrupt/hostile file). Generous for any real display name.
const maxUnitNameLen = 1024

// UnitNameStore (#217): per-instance name overrides — WC3's BlzSetUnitName. Most
// units use their type's proper name (GetUnitName), so overrides are rare; this
// is a sparse, lazily-allocated value store (a row exists only once a unit is
// renamed). Mirrors UserDataStore with a string value. Deterministic (scripts
// can read names back) and save-persisted.
type UnitNameStore struct {
	Name   []string
	Entity []EntityID

	rowOf []int32
	count int32
}

func NewUnitNameStore(rowCap, entityCap int) *UnitNameStore {
	if rowCap <= 0 || entityCap <= 0 || rowCap > entityCap {
		panic("sim: store caps must satisfy 0 < rowCap <= entityCap")
	}
	s := &UnitNameStore{
		Name:   make([]string, rowCap),
		Entity: make([]EntityID, rowCap),
		rowOf:  make([]int32, entityCap),
	}
	for i := range s.rowOf {
		s.rowOf[i] = -1
	}
	return s
}

// set assigns name to id, lazily allocating a row on first override. False only
// when the store is full or the handle index is out of range.
func (s *UnitNameStore) set(id EntityID, name string) bool {
	idx := id.Index()
	if int(idx) >= len(s.rowOf) {
		return false
	}
	if r := s.rowOf[idx]; r != -1 {
		s.Name[r] = name
		return true
	}
	if int(s.count) == len(s.Name) {
		return false
	}
	r := s.count
	s.Name[r] = name
	s.Entity[r] = id
	s.rowOf[idx] = r
	s.count++
	return true
}

func (s *UnitNameStore) Remove(id EntityID) bool {
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
		s.Name[r] = s.Name[last]
		moved := s.Entity[last]
		s.Entity[r] = moved
		s.rowOf[moved.Index()] = r
	}
	s.Name[last] = "" // release the moved-away string
	s.rowOf[idx] = -1
	s.count--
	return true
}

func (s *UnitNameStore) Row(id EntityID) int32 {
	if int(id.Index()) >= len(s.rowOf) {
		return -1
	}
	return s.rowOf[id.Index()]
}

func (s *UnitNameStore) Count() int32 { return s.count }

// unitNameOverride returns the per-instance name override and whether one is set.
func (w *World) unitNameOverride(id EntityID) (string, bool) {
	if r := w.UnitNames.Row(id); r != -1 {
		return w.UnitNames.Name[r], true
	}
	return "", false
}

// SetUnitName sets a per-instance display name override (BlzSetUnitName),
// shadowing the type's proper name. No-op on a dead unit. Returns false on a
// dead unit or a full store.
func (w *World) SetUnitName(id EntityID, name string) bool {
	if !w.Ents.Alive(id) {
		return false
	}
	return w.UnitNames.set(id, name)
}

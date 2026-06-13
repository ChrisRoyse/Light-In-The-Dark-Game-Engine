package sim

// UserDataStore (#217): the per-unit custom integer ("custom value") that
// scripts attach to a unit — WC3's GetUnitUserData/SetUnitUserData. It has no
// sim-system consumer; it is script bookkeeping that must still be deterministic
// (scripts branch on it) and persist across save/load. Sparse and lazy: a row
// exists only once a non-default value is set; an absent unit reads 0. T2
// pattern — see store_unittype.go.

type UserDataStore struct {
	Value  []int32
	Entity []EntityID

	rowOf []int32
	count int32
}

func NewUserDataStore(rowCap, entityCap int) *UserDataStore {
	if rowCap <= 0 || entityCap <= 0 || rowCap > entityCap {
		panic("sim: store caps must satisfy 0 < rowCap <= entityCap")
	}
	s := &UserDataStore{
		Value:  make([]int32, rowCap),
		Entity: make([]EntityID, rowCap),
		rowOf:  make([]int32, entityCap),
	}
	for i := range s.rowOf {
		s.rowOf[i] = -1
	}
	return s
}

// set assigns v to id, lazily allocating a row on first use. Returns false only
// when the store is full (capacity is caps.Units, so a live unit always fits).
func (s *UserDataStore) set(id EntityID, v int32) bool {
	idx := id.Index()
	if int(idx) >= len(s.rowOf) {
		return false
	}
	if r := s.rowOf[idx]; r != -1 {
		s.Value[r] = v
		return true
	}
	if int(s.count) == len(s.Value) {
		return false
	}
	r := s.count
	s.Value[r] = v
	s.Entity[r] = id
	s.rowOf[idx] = r
	s.count++
	return true
}

func (s *UserDataStore) Remove(id EntityID) bool {
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
		s.Value[r] = s.Value[last]
		moved := s.Entity[last]
		s.Entity[r] = moved
		s.rowOf[moved.Index()] = r
	}
	s.rowOf[idx] = -1
	s.count--
	return true
}

func (s *UserDataStore) Row(id EntityID) int32 {
	if int(id.Index()) >= len(s.rowOf) {
		return -1
	}
	return s.rowOf[id.Index()]
}

func (s *UserDataStore) Count() int32 { return s.count }

// UserData returns the unit's custom value, or 0 if none was ever set
// (or id is malformed).
func (w *World) UserData(id EntityID) int32 {
	r := w.UserDatas.Row(id)
	if r == -1 {
		return 0
	}
	return w.UserDatas.Value[r]
}

// SetUserData assigns the unit's custom value, lazily allocating a row.
// Returns false on a dead unit or a full store.
func (w *World) SetUserData(id EntityID, v int32) bool {
	if !w.Ents.Alive(id) {
		return false
	}
	return w.UserDatas.set(id, v)
}

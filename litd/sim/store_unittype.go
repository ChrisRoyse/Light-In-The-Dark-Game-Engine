package sim

// UnitTypeStore (ecs-architecture.md §5): unit-type ID → immutable
// data-table row (the SLK analogue, R-AST-1). The store holds only
// the 16-bit type ID; stats/model/sounds resolve through the loaded
// data table (#166), which is immutable for the match.
// T2 pattern — see store_transform.go.

type UnitTypeStore struct {
	TypeID []uint16
	Entity []EntityID

	rowOf []int32
	count int32

	DebugAssert func(msg string, id EntityID)
}

func NewUnitTypeStore(rowCap, entityCap int) *UnitTypeStore {
	if rowCap <= 0 || entityCap <= 0 || rowCap > entityCap {
		panic("sim: store caps must satisfy 0 < rowCap <= entityCap")
	}
	s := &UnitTypeStore{
		TypeID: make([]uint16, rowCap),
		Entity: make([]EntityID, rowCap),
		rowOf:  make([]int32, entityCap),
	}
	for i := range s.rowOf {
		s.rowOf[i] = -1
	}
	return s
}

func (s *UnitTypeStore) Add(e *Entities, id EntityID, typeID uint16) bool {
	if !e.Alive(id) {
		s.assert("Add on dead entity", id)
		return false
	}
	idx := id.Index()
	if s.rowOf[idx] != -1 {
		s.assert("double Add", id)
		return false
	}
	if int(s.count) == len(s.Entity) {
		return false
	}
	r := s.count
	s.TypeID[r] = typeID
	s.Entity[r] = id
	s.rowOf[idx] = r
	s.count++
	return true
}

func (s *UnitTypeStore) Remove(id EntityID) bool {
	idx := id.Index()
	if idx >= uint32(len(s.rowOf)) {
		s.assert("Remove with malformed handle", id)
		return false
	}
	r := s.rowOf[idx]
	if r == -1 {
		s.assert("Remove of absent component", id)
		return false
	}
	last := s.count - 1
	if r != last {
		s.TypeID[r] = s.TypeID[last]
		moved := s.Entity[last]
		s.Entity[r] = moved
		s.rowOf[moved.Index()] = r
	}
	s.rowOf[idx] = -1
	s.count--
	return true
}

func (s *UnitTypeStore) Row(id EntityID) int32 {
	idx := id.Index()
	if idx >= uint32(len(s.rowOf)) {
		return -1
	}
	return s.rowOf[idx]
}

func (s *UnitTypeStore) Count() int32 { return s.count }

func (s *UnitTypeStore) assert(msg string, id EntityID) {
	if s.DebugAssert != nil {
		s.DebugAssert(msg, id)
	}
}

package sim

// OwnerStore (ecs-architecture.md §5): player index, team, color.
// T2 pattern — see store_transform.go.

type OwnerStore struct {
	Player []uint8 // player slot index
	Team   []uint8
	Color  []uint8 // team-color index (render maps to palette)
	Entity []EntityID

	rowOf []int32
	count int32

	DebugAssert func(msg string, id EntityID)
}

func NewOwnerStore(rowCap, entityCap int) *OwnerStore {
	if rowCap <= 0 || entityCap <= 0 || rowCap > entityCap {
		panic("sim: store caps must satisfy 0 < rowCap <= entityCap")
	}
	s := &OwnerStore{
		Player: make([]uint8, rowCap),
		Team:   make([]uint8, rowCap),
		Color:  make([]uint8, rowCap),
		Entity: make([]EntityID, rowCap),
		rowOf:  make([]int32, entityCap),
	}
	for i := range s.rowOf {
		s.rowOf[i] = -1
	}
	return s
}

func (s *OwnerStore) Add(e *Entities, id EntityID, player, team, color uint8) bool {
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
	s.Player[r] = player
	s.Team[r] = team
	s.Color[r] = color
	s.Entity[r] = id
	s.rowOf[idx] = r
	s.count++
	return true
}

func (s *OwnerStore) Remove(id EntityID) bool {
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
		s.Player[r] = s.Player[last]
		s.Team[r] = s.Team[last]
		s.Color[r] = s.Color[last]
		moved := s.Entity[last]
		s.Entity[r] = moved
		s.rowOf[moved.Index()] = r
	}
	s.rowOf[idx] = -1
	s.count--
	return true
}

func (s *OwnerStore) Row(id EntityID) int32 {
	idx := id.Index()
	if idx >= uint32(len(s.rowOf)) {
		return -1
	}
	return s.rowOf[idx]
}

func (s *OwnerStore) Count() int32 { return s.count }

func (s *OwnerStore) assert(msg string, id EntityID) {
	if s.DebugAssert != nil {
		s.DebugAssert(msg, id)
	}
}

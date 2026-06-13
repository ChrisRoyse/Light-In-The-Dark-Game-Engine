package sim

// presenceSet is a sparse set of entities — membership is the entire signal,
// there is no per-row value. It backs the per-unit boolean flags that are rare
// in practice (hidden units, XP-suspended heroes, …), so the common empty case
// costs zero rows. Swap-down removal keeps rows dense; iteration order is the
// insertion/removal history, which is deterministic given identical op order
// (the same property every sparse store here relies on for hashing & save).
//
// Each consumer gets its own presenceSet (distinct hash/save section) but shares
// this logic — see HiddenStore methods (store_hidden.go) and the XP-suspend
// methods (store_xpsuspend.go).
type presenceSet struct {
	Entity []EntityID

	rowOf []int32
	count int32
}

func newPresenceSet(rowCap, entityCap int) *presenceSet {
	if rowCap <= 0 || entityCap <= 0 || rowCap > entityCap {
		panic("sim: store caps must satisfy 0 < rowCap <= entityCap")
	}
	s := &presenceSet{
		Entity: make([]EntityID, rowCap),
		rowOf:  make([]int32, entityCap),
	}
	for i := range s.rowOf {
		s.rowOf[i] = -1
	}
	return s
}

// set adds id (idempotent). False only when the store is full or the handle
// index is out of range.
func (s *presenceSet) set(id EntityID) bool {
	idx := id.Index()
	if int(idx) >= len(s.rowOf) {
		return false
	}
	if s.rowOf[idx] != -1 {
		return true // already present
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

// Remove drops id (swap-down). False if absent.
func (s *presenceSet) Remove(id EntityID) bool {
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

func (s *presenceSet) Row(id EntityID) int32 {
	if int(id.Index()) >= len(s.rowOf) {
		return -1
	}
	return s.rowOf[id.Index()]
}

func (s *presenceSet) Has(id EntityID) bool { return s.Row(id) != -1 }

func (s *presenceSet) Count() int32 { return s.count }

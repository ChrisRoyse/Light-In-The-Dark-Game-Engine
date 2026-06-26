package sim

// BuildStore (#301): one row per STRUCTURE — both under-construction
// and finished. It holds only the mutable construction state and the
// footprint placement; everything else (cost, build duration, finished
// HP, refund fraction, footprint size) is immutable data resolved
// through the unit-type table by the building's TypeID, so it is never
// duplicated here. The row lives for the building's whole life: while
// Progress < def.BuildTicks the structure is rising; the footprint
// stamp on the pathing grid is reconstructed from FX/FY/FW at load.
// T2 pattern — see store_transform.go.

type BuildStore struct {
	Builder  []EntityID // worker that started it (0 = released/none)
	FX, FY   []int32    // footprint origin cell (path-grid coords)
	FW       []int32    // footprint square side in cells
	Progress []uint16   // construction ticks elapsed
	Entity   []EntityID // the building entity (the row key)

	rowOf []int32
	count int32

	DebugAssert func(msg string, id EntityID)
}

func NewBuildStore(rowCap, entityCap int) *BuildStore {
	if rowCap <= 0 || entityCap <= 0 || rowCap > entityCap {
		panic("sim: store caps must satisfy 0 < rowCap <= entityCap")
	}
	s := &BuildStore{
		Builder:  make([]EntityID, rowCap),
		FX:       make([]int32, rowCap),
		FY:       make([]int32, rowCap),
		FW:       make([]int32, rowCap),
		Progress: make([]uint16, rowCap),
		Entity:   make([]EntityID, rowCap),
		rowOf:    make([]int32, entityCap),
	}
	for i := range s.rowOf {
		s.rowOf[i] = -1
	}
	return s
}

func (s *BuildStore) add(e *Entities, id EntityID, fx, fy, fw int32) int32 {
	if !e.Alive(id) {
		s.assert("Add on dead entity", id)
		return -1
	}
	idx := id.Index()
	if s.rowOf[idx] != -1 {
		s.assert("double Add", id)
		return -1
	}
	if int(s.count) == len(s.Entity) {
		return -1
	}
	r := s.count
	s.Builder[r] = 0
	s.FX[r] = fx
	s.FY[r] = fy
	s.FW[r] = fw
	s.Progress[r] = 0
	s.Entity[r] = id
	s.rowOf[idx] = r
	s.count++
	return r
}

func (s *BuildStore) Remove(id EntityID) bool {
	idx := id.Index()
	if idx >= uint32(len(s.rowOf)) {
		s.assert("Remove with malformed handle", id)
		return false
	}
	r := s.rowOf[idx]
	if r == -1 {
		return false
	}
	last := s.count - 1
	if r != last {
		s.Builder[r] = s.Builder[last]
		s.FX[r] = s.FX[last]
		s.FY[r] = s.FY[last]
		s.FW[r] = s.FW[last]
		s.Progress[r] = s.Progress[last]
		moved := s.Entity[last]
		s.Entity[r] = moved
		s.rowOf[moved.Index()] = r
	}
	s.rowOf[idx] = -1
	s.count--
	return true
}

func (s *BuildStore) Row(id EntityID) int32 {
	idx := id.Index()
	if idx >= uint32(len(s.rowOf)) {
		return -1
	}
	return s.rowOf[idx]
}

func (s *BuildStore) Count() int32 { return s.count }

// builderRow returns the build row whose Builder is `worker`, or -1.
// Linear scan — structures under construction are few (R-GC-2 cap).
func (s *BuildStore) builderRow(worker EntityID) int32 {
	for r := int32(0); r < s.count; r++ {
		if s.Builder[r] == worker {
			return r
		}
	}
	return -1
}

func (s *BuildStore) assert(msg string, id EntityID) {
	if s.DebugAssert != nil {
		s.DebugAssert(msg, id)
	}
}

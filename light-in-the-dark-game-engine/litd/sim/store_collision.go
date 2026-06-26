package sim

// CollisionStore (ecs-architecture.md §5): WC3 collision-size
// semantics + grid occupancy. T2 pattern — see store_transform.go.

// Pathing-domain flags (pathfinding.md §4.1 movement layers). These
// select which dilated layer answers for the unit; the grid's own
// per-cell flags live in litd/sim/path.
const (
	PathGround uint8 = 1 << 0
	PathAir    uint8 = 1 << 1
	PathBuild  uint8 = 1 << 2 // structure: stamps a footprint
)

// NoStamp is the StampRef value for "no grid stamp held".
const NoStamp int32 = -1

type CollisionStore struct {
	SizeClass []uint8 // collision-size class index (data-driven radii)
	PathFlags []uint8 // Path* domain mask
	StampRef  []int32 // grid stamp ref for structures; NoStamp if none
	Entity    []EntityID

	rowOf []int32
	count int32

	DebugAssert func(msg string, id EntityID)
}

func NewCollisionStore(rowCap, entityCap int) *CollisionStore {
	if rowCap <= 0 || entityCap <= 0 || rowCap > entityCap {
		panic("sim: store caps must satisfy 0 < rowCap <= entityCap")
	}
	s := &CollisionStore{
		SizeClass: make([]uint8, rowCap),
		PathFlags: make([]uint8, rowCap),
		StampRef:  make([]int32, rowCap),
		Entity:    make([]EntityID, rowCap),
		rowOf:     make([]int32, entityCap),
	}
	for i := range s.rowOf {
		s.rowOf[i] = -1
	}
	return s
}

func (s *CollisionStore) Add(e *Entities, id EntityID, sizeClass, pathFlags uint8) bool {
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
	s.SizeClass[r] = sizeClass
	s.PathFlags[r] = pathFlags
	s.StampRef[r] = NoStamp
	s.Entity[r] = id
	s.rowOf[idx] = r
	s.count++
	return true
}

func (s *CollisionStore) Remove(id EntityID) bool {
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
		s.SizeClass[r] = s.SizeClass[last]
		s.PathFlags[r] = s.PathFlags[last]
		s.StampRef[r] = s.StampRef[last]
		moved := s.Entity[last]
		s.Entity[r] = moved
		s.rowOf[moved.Index()] = r
	}
	s.rowOf[idx] = -1
	s.count--
	return true
}

func (s *CollisionStore) Row(id EntityID) int32 {
	idx := id.Index()
	if idx >= uint32(len(s.rowOf)) {
		return -1
	}
	return s.rowOf[idx]
}

func (s *CollisionStore) Count() int32 { return s.count }

func (s *CollisionStore) assert(msg string, id EntityID) {
	if s.DebugAssert != nil {
		s.DebugAssert(msg, id)
	}
}

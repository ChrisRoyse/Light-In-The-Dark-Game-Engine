package sim

// MovementStore (ecs-architecture.md §5): locomotion state consuming
// pathfinding results. T2 pattern — see store_transform.go. All rates
// are PER-TICK fixed-point increments (R-SIM-1: no per-second floats
// anywhere; data tables convert at load).

import "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"

// Move states of the movement state machine (#113 consumes these).
const (
	MoveIdle      uint8 = iota // no active path
	MoveFollowing              // walking the waypoint list
	MoveBlocked                // local avoidance stalled, awaiting re-path
	MoveFlow                   // following a shared flow-field slot
)

// NoPath is the PathHandle value meaning "no cached path held".
// 0xFFFFFFFF, NOT zero: path.PathID 0 is the legitimate first ID
// (generation 0, slot 0), while slot 0xFFFFFF can never exist below
// the engine's pool ceilings — the sentinel is unreachable.
const NoPath uint32 = 0xFFFFFFFF

type MovementStore struct {
	Speed       []fixed.F64   // world units per TICK
	TurnRate    []fixed.Angle // max facing change per TICK
	Target      []fixed.Vec2  // current waypoint
	PathHandle  []uint32      // path.PathID bits; NoPath when none
	WaypointIdx []int32       // cursor into the path's waypoint list
	Stall       []uint16      // consecutive blocked ticks (avoidance)
	ResCell     []int32       // reserved grid cell; -1 = none
	State       []uint8       // Move* constants
	Entity      []EntityID

	rowOf []int32
	count int32

	DebugAssert func(msg string, id EntityID)
}

func NewMovementStore(rowCap, entityCap int) *MovementStore {
	if rowCap <= 0 || entityCap <= 0 || rowCap > entityCap {
		panic("sim: store caps must satisfy 0 < rowCap <= entityCap")
	}
	s := &MovementStore{
		Speed:       make([]fixed.F64, rowCap),
		TurnRate:    make([]fixed.Angle, rowCap),
		Target:      make([]fixed.Vec2, rowCap),
		PathHandle:  make([]uint32, rowCap),
		WaypointIdx: make([]int32, rowCap),
		Stall:       make([]uint16, rowCap),
		ResCell:     make([]int32, rowCap),
		State:       make([]uint8, rowCap),
		Entity:      make([]EntityID, rowCap),
		rowOf:       make([]int32, entityCap),
	}
	for i := range s.rowOf {
		s.rowOf[i] = -1
	}
	return s
}

// Add attaches Movement. Movement REQUIRES Transform (ecs §7 join
// idiom): adding to an entity without a Transform row fails closed
// and fires the debug assert.
func (s *MovementStore) Add(e *Entities, t *TransformStore, id EntityID, speed fixed.F64, turnRate fixed.Angle) bool {
	if !e.Alive(id) {
		s.assert("Add on dead entity", id)
		return false
	}
	if t.Row(id) == -1 {
		s.assert("Movement requires Transform", id)
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
	s.Speed[r] = speed
	s.TurnRate[r] = turnRate
	s.Target[r] = fixed.Vec2{}
	s.PathHandle[r] = NoPath
	s.WaypointIdx[r] = 0
	s.Stall[r] = 0
	s.ResCell[r] = -1
	s.State[r] = MoveIdle
	s.Entity[r] = id
	s.rowOf[idx] = r
	s.count++
	return true
}

func (s *MovementStore) Remove(id EntityID) bool {
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
		s.Speed[r] = s.Speed[last]
		s.TurnRate[r] = s.TurnRate[last]
		s.Target[r] = s.Target[last]
		s.PathHandle[r] = s.PathHandle[last]
		s.WaypointIdx[r] = s.WaypointIdx[last]
		s.Stall[r] = s.Stall[last]
		s.ResCell[r] = s.ResCell[last]
		s.State[r] = s.State[last]
		moved := s.Entity[last]
		s.Entity[r] = moved
		s.rowOf[moved.Index()] = r
	}
	s.rowOf[idx] = -1
	s.count--
	return true
}

func (s *MovementStore) Row(id EntityID) int32 {
	idx := id.Index()
	if idx >= uint32(len(s.rowOf)) {
		return -1
	}
	return s.rowOf[idx]
}

func (s *MovementStore) Count() int32 { return s.count }

func (s *MovementStore) assert(msg string, id EntityID) {
	if s.DebugAssert != nil {
		s.DebugAssert(msg, id)
	}
}

package sim

// OrderStore (ecs-architecture.md §5, combat-and-orders.md §2.3): the
// per-unit order HEAD — current order plus the index of the first
// pooled queue entry. The 16,000-entry intrusive queue pool itself is
// #144's deliverable; the head stores the linkage contract today.
// T2 pattern — see store_transform.go.

import "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"

// NoOrderEntry is the QueueHead value for an empty queue.
const NoOrderEntry int32 = -1

// Order kinds (the universal verb set grows with #144/#146; Stop is
// the default order every unit falls back to).
const (
	OrderStop uint8 = iota
	OrderMove
	OrderAttack
	OrderSmart // right-click, resolved by the smart-order table (#146)
	OrderHold
)

type OrderStore struct {
	Kind      []uint8      // current order
	Phase     []uint8      // orderFresh / orderRunning (orders.go)
	Target    []EntityID   // entity target (0 = none / point order)
	Point     []fixed.Vec2 // point target
	QueueHead []int32      // first pooled queue entry; NoOrderEntry = empty
	Entity    []EntityID

	rowOf []int32
	count int32

	DebugAssert func(msg string, id EntityID)
}

func NewOrderStore(rowCap, entityCap int) *OrderStore {
	if rowCap <= 0 || entityCap <= 0 || rowCap > entityCap {
		panic("sim: store caps must satisfy 0 < rowCap <= entityCap")
	}
	s := &OrderStore{
		Kind:      make([]uint8, rowCap),
		Phase:     make([]uint8, rowCap),
		Target:    make([]EntityID, rowCap),
		Point:     make([]fixed.Vec2, rowCap),
		QueueHead: make([]int32, rowCap),
		Entity:    make([]EntityID, rowCap),
		rowOf:     make([]int32, entityCap),
	}
	for i := range s.rowOf {
		s.rowOf[i] = -1
	}
	return s
}

// Add attaches an order head at the default order (Stop, empty queue).
func (s *OrderStore) Add(e *Entities, id EntityID) bool {
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
	s.Kind[r] = OrderStop
	s.Phase[r] = 0
	s.Target[r] = 0
	s.Point[r] = fixed.Vec2{}
	s.QueueHead[r] = NoOrderEntry
	s.Entity[r] = id
	s.rowOf[idx] = r
	s.count++
	return true
}

func (s *OrderStore) Remove(id EntityID) bool {
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
		s.Kind[r] = s.Kind[last]
		s.Phase[r] = s.Phase[last]
		s.Target[r] = s.Target[last]
		s.Point[r] = s.Point[last]
		s.QueueHead[r] = s.QueueHead[last]
		moved := s.Entity[last]
		s.Entity[r] = moved
		s.rowOf[moved.Index()] = r
	}
	s.rowOf[idx] = -1
	s.count--
	return true
}

func (s *OrderStore) Row(id EntityID) int32 {
	idx := id.Index()
	if idx >= uint32(len(s.rowOf)) {
		return -1
	}
	return s.rowOf[idx]
}

func (s *OrderStore) Count() int32 { return s.count }

func (s *OrderStore) assert(msg string, id EntityID) {
	if s.DebugAssert != nil {
		s.DebugAssert(msg, id)
	}
}

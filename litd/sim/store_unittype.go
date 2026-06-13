package sim

import (
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
)

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

// typeDefOf resolves a unit to its immutable data-table row, or nil when the
// unit has no type row or the type index is out of range.
func (w *World) typeDefOf(id EntityID) *data.Unit {
	if ut := w.UnitTypes.Row(id); ut != -1 {
		if tid := w.UnitTypes.TypeID[ut]; int(tid) < len(w.unitDefs) {
			return &w.unitDefs[tid]
		}
	}
	return nil
}

// UnitPointValue resolves a unit's type to its data-table point value
// (GetUnitPointValue), or 0 when the unit has no type row or the type is out
// of range. Read-only over immutable type data — no per-unit state.
func (w *World) UnitPointValue(id EntityID) int32 {
	if d := w.typeDefOf(id); d != nil {
		return d.PointValue
	}
	return 0
}

// UnitDefaultMoveSpeed returns the type's base move speed (per tick), 0 when
// untyped. GetUnitDefaultMoveSpeed — the spawn value before SetUnitMoveSpeed.
func (w *World) UnitDefaultMoveSpeed(id EntityID) fixed.F64 {
	if d := w.typeDefOf(id); d != nil {
		return d.MoveSpeedPerTick
	}
	return 0
}

// UnitDefaultAcquireRange returns the type's base acquisition range (world
// units), 0 when untyped. GetUnitDefaultAcquireRange.
func (w *World) UnitDefaultAcquireRange(id EntityID) fixed.F64 {
	if d := w.typeDefOf(id); d != nil {
		return d.AcquisitionRange
	}
	return 0
}

// UnitDefaultTurnSpeed returns the type's base turn rate (brad per tick), 0
// when untyped. GetUnitDefaultTurnSpeed.
func (w *World) UnitDefaultTurnSpeed(id EntityID) fixed.Angle {
	if d := w.typeDefOf(id); d != nil {
		return d.TurnRatePerTick
	}
	return 0
}

func (s *UnitTypeStore) assert(msg string, id EntityID) {
	if s.DebugAssert != nil {
		s.DebugAssert(msg, id)
	}
}

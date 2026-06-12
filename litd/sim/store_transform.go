package sim

// This file is the reference SoA component store (ecs-architecture.md
// §4): every other component store copies this exact pattern.
//
//   - Parallel dense columns, one slice per field: systems touch only
//     the columns they need; rows [0,count) are always live and
//     contiguous.
//   - rowOf sparse map (entity index → row, -1 absent), capacity =
//     entity cap, so entity→row lookup is one array probe.
//   - Removal is swap-with-last: the last row copies into the hole,
//     the moved entity's rowOf is fixed, count decrements. Iteration
//     order changes but remains fully deterministic — it depends only
//     on the deterministic add/remove history — and the state hash
//     reads stores in row order, so ordering bugs surface immediately.
//   - All slices make()d once at construction (R-GC-2); iteration is
//     plain index loops over the exported columns — no iterator
//     objects, no ForEach callbacks, no interface values (ecs §7).

import "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"

// TransformStore holds position and facing — the one component every
// entity has. Read columns directly: for r := int32(0); r < s.Count(); r++ { s.Pos[r] ... }.
type TransformStore struct {
	Pos    []fixed.Vec2
	Facing []fixed.Angle
	Entity []EntityID // row -> owning entity

	rowOf []int32 // entity index -> row, -1 if absent
	count int32

	// DebugAssert, when non-nil, fires on contract violations a debug
	// build wants loud: Add on a dead entity, double Add, Remove of an
	// absent component. The operation still fails closed (no-op).
	DebugAssert func(msg string, id EntityID)
}

// NewTransformStore allocates a store for at most rowCap components
// over an entity table of entityCap slots. Both allocations happen
// exactly once, at map load.
func NewTransformStore(rowCap, entityCap int) *TransformStore {
	if rowCap <= 0 || entityCap <= 0 || rowCap > entityCap {
		panic("sim: store caps must satisfy 0 < rowCap <= entityCap")
	}
	s := &TransformStore{
		Pos:    make([]fixed.Vec2, rowCap),
		Facing: make([]fixed.Angle, rowCap),
		Entity: make([]EntityID, rowCap),
		rowOf:  make([]int32, entityCap),
	}
	for i := range s.rowOf {
		s.rowOf[i] = -1
	}
	return s
}

// Add attaches the component to a live entity. Fails closed (false)
// when the entity is dead/stale, already has the component, or the
// store is full.
func (s *TransformStore) Add(e *Entities, id EntityID, pos fixed.Vec2, facing fixed.Angle) bool {
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
		return false // store full: gameplay outcome, never reallocation
	}
	r := s.count
	s.Pos[r] = pos
	s.Facing[r] = facing
	s.Entity[r] = id
	s.rowOf[idx] = r
	s.count++
	return true
}

// Remove detaches the component via swap-with-last. Returns false
// (no-op) when the entity has no component row.
func (s *TransformStore) Remove(id EntityID) bool {
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
		// copy the last row into the hole and fix its rowOf
		s.Pos[r] = s.Pos[last]
		s.Facing[r] = s.Facing[last]
		moved := s.Entity[last]
		s.Entity[r] = moved
		s.rowOf[moved.Index()] = r
	}
	s.rowOf[idx] = -1
	s.count--
	return true
}

// Row returns the dense row for id, or -1 when absent. Rows are
// ephemeral — never store them across a Remove.
func (s *TransformStore) Row(id EntityID) int32 {
	idx := id.Index()
	if idx >= uint32(len(s.rowOf)) {
		return -1
	}
	return s.rowOf[idx]
}

// Count returns the number of live rows; rows [0,Count) are valid.
func (s *TransformStore) Count() int32 { return s.count }

func (s *TransformStore) assert(msg string, id EntityID) {
	if s.DebugAssert != nil {
		s.DebugAssert(msg, id)
	}
}

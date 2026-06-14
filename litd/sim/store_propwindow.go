package sim

// Prop-window subsystem (#376; the second half of the #367 z/climb
// discovery). The propulsion window is the angle between a unit's facing
// and its desired move direction within which it may translate; outside
// it the unit turns in place first. movementSystem consults it (the gate
// lives in movement.go).
//
// Sparse + lazy (T2, like UnitNameStore): a row exists only once a unit's
// window is explicitly set. An un-set unit uses its unit-type default
// (DefaultPropWindow). The default for an unauthored unit is propWindowNoGate
// (a half-turn) so existing movement — and the golden / determinism traces —
// are byte-identical; only a narrowed window changes behavior.

import "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"

// propWindowNoGate is the half-turn tolerance that disables gating: the
// shortest arc between two angles never exceeds a half-turn, so a window
// this wide never blocks translation.
const propWindowNoGate = fixed.Angle(0x8000)

// PropWindowStore holds per-unit propulsion-window overrides.
type PropWindowStore struct {
	Value  []fixed.Angle
	Entity []EntityID

	rowOf []int32
	count int32
}

func NewPropWindowStore(rowCap, entityCap int) *PropWindowStore {
	if rowCap <= 0 || entityCap <= 0 || rowCap > entityCap {
		panic("sim: store caps must satisfy 0 < rowCap <= entityCap")
	}
	s := &PropWindowStore{
		Value:  make([]fixed.Angle, rowCap),
		Entity: make([]EntityID, rowCap),
		rowOf:  make([]int32, entityCap),
	}
	for i := range s.rowOf {
		s.rowOf[i] = -1
	}
	return s
}

func (s *PropWindowStore) set(id EntityID, v fixed.Angle) bool {
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

func (s *PropWindowStore) Row(id EntityID) int32 {
	idx := id.Index()
	if idx >= uint32(len(s.rowOf)) {
		return -1
	}
	return s.rowOf[idx]
}

func (s *PropWindowStore) Count() int32 { return s.count }

// Remove drops id's row (swap-down) on unit destroy so a recycled slot
// reverts to its type default. False on an absent row.
func (s *PropWindowStore) Remove(id EntityID) bool {
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

// ---- world surface ----

// DefaultPropWindow returns the unit type's base propulsion window (the
// data-table value), or propWindowNoGate for an untyped unit / before
// unit defs are bound. GetUnitDefaultPropWindow.
func (w *World) DefaultPropWindow(id EntityID) fixed.Angle {
	r := w.UnitTypes.Row(id)
	if r == -1 || w.unitDefs == nil {
		return propWindowNoGate
	}
	t := w.UnitTypes.TypeID[r]
	if int(t) >= len(w.unitDefs) {
		return propWindowNoGate
	}
	return w.unitDefs[t].PropWindow
}

// PropWindow returns a unit's effective propulsion window: the override if
// set, otherwise the type default. GetUnitPropWindow.
func (w *World) PropWindow(id EntityID) fixed.Angle {
	if r := w.PropWindows.Row(id); r != -1 {
		return w.PropWindows.Value[r]
	}
	return w.DefaultPropWindow(id)
}

// SetPropWindow overrides a unit's propulsion window. No-op on a dead unit
// or one without a transform. Returns false when it could not apply.
// SetUnitPropWindow.
func (w *World) SetPropWindow(id EntityID, window fixed.Angle) bool {
	if !w.Ents.Alive(id) || w.Transforms.Row(id) == -1 {
		return false
	}
	return w.PropWindows.set(id, window)
}

// angleArc returns the shortest-arc magnitude between two BAM angles, in
// [0, halfTurn]. Used by the prop-window gate.
func angleArc(a, b fixed.Angle) fixed.Angle {
	d := a - b // uint16 wraparound
	if d > propWindowNoGate {
		d = -d
	}
	return d
}

// propWindowBlocks reports whether the unit's facing is outside its
// propulsion window relative to the desired heading — i.e. it must turn in
// place this tick instead of translating. A no-gate window never blocks.
func (w *World) propWindowBlocks(id EntityID, facing, want fixed.Angle) bool {
	pw := w.PropWindow(id)
	if pw >= propWindowNoGate {
		return false
	}
	return angleArc(facing, want) > pw
}

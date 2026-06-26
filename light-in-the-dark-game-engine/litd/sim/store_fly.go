package sim

// Fly-height subsystem (#367). A unit's flight height is its z: a value
// the 2D Transforms store has no column for. SetUnitFlyHeight animates the
// height toward a target at a climb rate, integrated each movement tick —
// so it is per-unit current+target+rate state, not a static accessor.
//
// Sparse + lazy (T2, like UserDataStore): a row exists only once a unit's
// height is explicitly set. An un-set unit reads its unit-type default
// (DefaultFlyHeight), so the common ground unit costs no memory and the
// default is honest type data, never a placeholder. Prop-window — the
// other half of the #367 discovery — is a behavioral change to movement
// translation and is tracked separately.

import "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"

// FlyStore holds per-unit flight-height animation state. Height is the
// current z; Target the goal; Rate the per-tick climb magnitude (0 = the
// height is parked, set instantly).
type FlyStore struct {
	Height []fixed.F64
	Target []fixed.F64
	Rate   []fixed.F64
	Entity []EntityID

	rowOf []int32
	count int32
}

func NewFlyStore(rowCap, entityCap int) *FlyStore {
	if rowCap <= 0 || entityCap <= 0 || rowCap > entityCap {
		panic("sim: store caps must satisfy 0 < rowCap <= entityCap")
	}
	s := &FlyStore{
		Height: make([]fixed.F64, rowCap),
		Target: make([]fixed.F64, rowCap),
		Rate:   make([]fixed.F64, rowCap),
		Entity: make([]EntityID, rowCap),
		rowOf:  make([]int32, entityCap),
	}
	for i := range s.rowOf {
		s.rowOf[i] = -1
	}
	return s
}

// set creates or updates the row for id. Returns false only when the
// store is full (cap == caps.Units, so a live unit always fits).
func (s *FlyStore) set(id EntityID, height, target, rate fixed.F64) bool {
	idx := id.Index()
	if int(idx) >= len(s.rowOf) {
		return false
	}
	if r := s.rowOf[idx]; r != -1 {
		s.Height[r], s.Target[r], s.Rate[r] = height, target, rate
		return true
	}
	if int(s.count) == len(s.Height) {
		return false
	}
	r := s.count
	s.Height[r], s.Target[r], s.Rate[r] = height, target, rate
	s.Entity[r] = id
	s.rowOf[idx] = r
	s.count++
	return true
}

func (s *FlyStore) Row(id EntityID) int32 {
	idx := id.Index()
	if idx >= uint32(len(s.rowOf)) {
		return -1
	}
	return s.rowOf[idx]
}

func (s *FlyStore) Count() int32 { return s.count }

// Remove drops id's row (swap-down), called on unit destroy so a recycled
// slot never inherits stale height. False on an absent row.
func (s *FlyStore) Remove(id EntityID) bool {
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
		s.Height[r] = s.Height[last]
		s.Target[r] = s.Target[last]
		s.Rate[r] = s.Rate[last]
		moved := s.Entity[last]
		s.Entity[r] = moved
		s.rowOf[moved.Index()] = r
	}
	s.rowOf[idx] = -1
	s.count--
	return true
}

// ---- world surface ----

// DefaultFlyHeight returns the unit type's base flight height (the data
// table value), 0 for an untyped unit or before unit defs are bound.
// GetUnitDefaultFlyHeight.
func (w *World) DefaultFlyHeight(id EntityID) fixed.F64 {
	r := w.UnitTypes.Row(id)
	if r == -1 || w.unitDefs == nil {
		return 0
	}
	t := w.UnitTypes.TypeID[r]
	if int(t) >= len(w.unitDefs) {
		return 0
	}
	return w.unitDefs[t].FlyHeight
}

// FlyHeight returns a unit's current flight height: the animated value
// once set, otherwise the unit-type default. GetUnitFlyHeight.
func (w *World) FlyHeight(id EntityID) fixed.F64 {
	if r := w.Flys.Row(id); r != -1 {
		return w.Flys.Height[r]
	}
	return w.DefaultFlyHeight(id)
}

// SetFlyHeight retargets a unit's flight height. ratePerTick is the climb
// magnitude per tick; <= 0 snaps to the target instantly. The animation
// starts from the unit's current height. No-op on a dead unit or a unit
// without a transform (flight height is a spatial property). Returns false
// when it could not apply. SetUnitFlyHeight.
func (w *World) SetFlyHeight(id EntityID, target, ratePerTick fixed.F64) bool {
	if !w.Ents.Alive(id) || w.Transforms.Row(id) == -1 {
		return false
	}
	if ratePerTick <= 0 {
		return w.Flys.set(id, target, target, 0) // snap
	}
	return w.Flys.set(id, w.FlyHeight(id), target, ratePerTick)
}

// flySystem advances each animated flight height toward its target by the
// climb rate, clamped (never overshoots). Run in the movement phase. Empty
// store ⇒ no-op (golden/determinism traces undisturbed). Paused units
// freeze, mirroring movementSystem.
func (w *World) flySystem() {
	s := w.Flys
	for r := int32(0); r < s.count; r++ {
		if s.Rate[r] == 0 || s.Height[r] == s.Target[r] {
			continue
		}
		if w.Pauses.Has(s.Entity[r]) {
			continue
		}
		h, t, rate := s.Height[r], s.Target[r], s.Rate[r]
		if h < t {
			h = h.Add(rate)
			if h > t {
				h = t
			}
		} else {
			h = h.Sub(rate)
			if h < t {
				h = t
			}
		}
		s.Height[r] = h
	}
}

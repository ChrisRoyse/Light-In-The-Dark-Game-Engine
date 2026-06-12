package sim

// Ability field overrides (#353): sparse per-instance values over the
// immutable data.Ability table. Rows are pooled and indexed by
// entity-slot-field, so casts resolve fields without maps or iteration.

import (
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
)

// AbilityField names one overridable per-instance ability field. The
// enum is append-only because save/hash encoding in #354 will persist
// the numeric ids.
type AbilityField uint8

const (
	AbilityFieldCooldown AbilityField = iota
	AbilityFieldManaCost
	AbilityFieldRange
	AbilityFieldDamage
	AbilityFieldDuration
	AbilityFieldAreaOfEffect
	AbilityFieldCastTime

	// AbilityFieldCount is the number of known field ids.
	AbilityFieldCount
)

// AbilityOverrideCapPerUnit bounds copy-on-write field rows per unit.
const AbilityOverrideCapPerUnit = 16

// AbilityFieldStore owns sparse override rows. rowOf is keyed by
// entity index, slot, and field for O(1) lookup; each row still carries
// the full EntityID so a stale generation can never alias a recycled
// slot if cleanup is missed.
type AbilityFieldStore struct {
	Ent   []EntityID
	Slot  []uint8
	Field []uint8
	Value []fixed.F64

	live    []bool
	free    []int32
	rowOf   []int32
	perUnit []uint8

	count    int32
	rejected uint64

	DebugAssert func(msg string, id EntityID)
}

func NewAbilityFieldStore(rowCap, entityCap int) *AbilityFieldStore {
	if rowCap <= 0 || entityCap <= 0 {
		panic("sim: ability field caps must be positive")
	}
	s := &AbilityFieldStore{
		Ent:     make([]EntityID, rowCap),
		Slot:    make([]uint8, rowCap),
		Field:   make([]uint8, rowCap),
		Value:   make([]fixed.F64, rowCap),
		live:    make([]bool, rowCap),
		free:    make([]int32, rowCap),
		rowOf:   make([]int32, entityCap*AbilitySlots*int(AbilityFieldCount)),
		perUnit: make([]uint8, entityCap),
	}
	for i := range s.free {
		s.free[i] = int32(rowCap - 1 - i) // pop order: row 0 first
	}
	for i := range s.rowOf {
		s.rowOf[i] = -1
	}
	return s
}

// Set writes an override row, updating in place when the same
// entity-slot-field already exists. New rows fail closed when the
// per-unit cap or fixed pool is exhausted.
func (s *AbilityFieldStore) Set(e *Entities, id EntityID, slot int, field AbilityField, value fixed.F64) bool {
	if !s.validTarget(e, id, slot, field) {
		s.assert("ability field Set invalid target", id)
		return false
	}
	idx := id.Index()
	k := s.key(idx, slot, field)
	if r := s.rowOf[k]; r != -1 {
		if s.live[r] && s.Ent[r] == id {
			s.Value[r] = value
			return true
		}
		s.assert("ability field stale rowOf entry", id)
		return false
	}
	if s.perUnit[idx] >= AbilityOverrideCapPerUnit || len(s.free) == 0 {
		s.rejected++
		return false
	}
	r := s.free[len(s.free)-1]
	s.free = s.free[:len(s.free)-1]
	s.Ent[r] = id
	s.Slot[r] = uint8(slot)
	s.Field[r] = uint8(field)
	s.Value[r] = value
	s.live[r] = true
	s.rowOf[k] = r
	s.perUnit[idx]++
	s.count++
	return true
}

func (s *AbilityFieldStore) Get(id EntityID, slot int, field AbilityField) (fixed.F64, bool) {
	if !validAbilitySlot(slot) || !validAbilityField(field) {
		return 0, false
	}
	idx := id.Index()
	if idx >= uint32(len(s.perUnit)) {
		return 0, false
	}
	r := s.rowOf[s.key(idx, slot, field)]
	if r == -1 || !s.live[r] || s.Ent[r] != id {
		return 0, false
	}
	return s.Value[r], true
}

func (s *AbilityFieldStore) Remove(id EntityID, slot int, field AbilityField) bool {
	if !validAbilitySlot(slot) || !validAbilityField(field) {
		return false
	}
	idx := id.Index()
	if idx >= uint32(len(s.perUnit)) {
		return false
	}
	r := s.rowOf[s.key(idx, slot, field)]
	if r == -1 || !s.live[r] || s.Ent[r] != id {
		return false
	}
	s.freeRow(r)
	return true
}

func (s *AbilityFieldStore) RemoveSlot(id EntityID, slot int) int {
	if !validAbilitySlot(slot) {
		return 0
	}
	removed := 0
	for f := AbilityField(0); f < AbilityFieldCount; f++ {
		if s.Remove(id, slot, f) {
			removed++
		}
	}
	return removed
}

func (s *AbilityFieldStore) RemoveEntity(id EntityID) int {
	idx := id.Index()
	if idx >= uint32(len(s.perUnit)) || s.perUnit[idx] == 0 {
		return 0
	}
	removed := 0
	for slot := 0; slot < AbilitySlots; slot++ {
		removed += s.RemoveSlot(id, slot)
	}
	return removed
}

func (s *AbilityFieldStore) Count() int32       { return s.count }
func (s *AbilityFieldStore) Cap() int           { return len(s.Ent) }
func (s *AbilityFieldStore) Rejected() uint64   { return s.rejected }
func (s *AbilityFieldStore) FreeCount() int     { return len(s.free) }
func validAbilitySlot(slot int) bool            { return slot >= 0 && slot < AbilitySlots }
func validAbilityField(field AbilityField) bool { return field < AbilityFieldCount }

func (s *AbilityFieldStore) validTarget(e *Entities, id EntityID, slot int, field AbilityField) bool {
	idx := id.Index()
	return e != nil && e.Alive(id) &&
		idx < uint32(len(s.perUnit)) &&
		validAbilitySlot(slot) &&
		validAbilityField(field)
}

func (s *AbilityFieldStore) key(idx uint32, slot int, field AbilityField) int {
	return (int(idx)*AbilitySlots+slot)*int(AbilityFieldCount) + int(field)
}

func (s *AbilityFieldStore) freeRow(r int32) {
	id := s.Ent[r]
	idx := id.Index()
	slot := int(s.Slot[r])
	field := AbilityField(s.Field[r])
	s.rowOf[s.key(idx, slot, field)] = -1
	if idx < uint32(len(s.perUnit)) && s.perUnit[idx] > 0 {
		s.perUnit[idx]--
	}
	s.Ent[r] = 0
	s.Slot[r] = 0
	s.Field[r] = 0
	s.Value[r] = 0
	s.live[r] = false
	s.free = append(s.free, r)
	s.count--
}

func (s *AbilityFieldStore) assert(msg string, id EntityID) {
	if s.DebugAssert != nil {
		s.DebugAssert(msg, id)
	}
}

func abilityDefField(def *data.Ability, field AbilityField) (fixed.F64, bool) {
	if def == nil || !validAbilityField(field) {
		return 0, false
	}
	switch field {
	case AbilityFieldCooldown:
		return fixed.FromInt(int32(def.CooldownTicks)), true
	case AbilityFieldManaCost:
		return fixed.FromInt(def.ManaCost), true
	case AbilityFieldRange:
		return def.CastRange, true
	case AbilityFieldDamage, AbilityFieldDuration, AbilityFieldAreaOfEffect:
		return 0, true
	case AbilityFieldCastTime:
		return fixed.FromInt(int32(def.CastPointTicks)), true
	default:
		return 0, false
	}
}

package sim

// AbilityStore (ecs-architecture.md §5, combat-and-orders.md §5.1):
// fixed ability slots per unit; cooldowns are absolute ready-at tick
// clocks (no per-tick decrement loops), cast state is the §5.1 state
// machine's current state. T2 pattern — see store_transform.go.
// Slot with AbilityID 0 = empty (refs are defIndex+1 into the bound
// ability defs, ability.go).

import "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"

// AbilitySlots is the per-unit ability slot count (WC3 command-card
// scale; heroes use 4-5, the rest stay zero).
const AbilitySlots = 6

// Cast states (combat-and-orders.md §5.1 state machine).
const (
	CastReady uint8 = iota
	CastPrecast
	CastPoint
	CastChannel
	CastBackswing
	CastCooldown
)

type AbilityStore struct {
	AbilityID [][AbilitySlots]uint16 // ability ref (defIndex+1); 0 = empty slot
	Level     [][AbilitySlots]uint8
	ReadyAt   [][AbilitySlots]uint32 // absolute ready-at tick
	CastState [][AbilitySlots]uint8  // Cast* constants

	// per unit: the mana pool and the single active cast (a unit
	// channels at most one ability at a time)
	Mana      []fixed.F64
	MaxMana   []fixed.F64
	ManaRegen []fixed.F64 // per TICK
	CastSlot  []int8      // active cast slot; -1 = none
	CastEnd   []uint32    // absolute end tick of the current cast phase
	Entity    []EntityID

	rowOf []int32
	count int32

	DebugAssert func(msg string, id EntityID)
}

func NewAbilityStore(rowCap, entityCap int) *AbilityStore {
	if rowCap <= 0 || entityCap <= 0 || rowCap > entityCap {
		panic("sim: store caps must satisfy 0 < rowCap <= entityCap")
	}
	s := &AbilityStore{
		AbilityID: make([][AbilitySlots]uint16, rowCap),
		Level:     make([][AbilitySlots]uint8, rowCap),
		ReadyAt:   make([][AbilitySlots]uint32, rowCap),
		CastState: make([][AbilitySlots]uint8, rowCap),
		Mana:      make([]fixed.F64, rowCap),
		MaxMana:   make([]fixed.F64, rowCap),
		ManaRegen: make([]fixed.F64, rowCap),
		CastSlot:  make([]int8, rowCap),
		CastEnd:   make([]uint32, rowCap),
		Entity:    make([]EntityID, rowCap),
		rowOf:     make([]int32, entityCap),
	}
	for i := range s.rowOf {
		s.rowOf[i] = -1
	}
	return s
}

// Add attaches a zero-valued Ability row (all slots empty).
func (s *AbilityStore) Add(e *Entities, id EntityID) bool {
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
	s.AbilityID[r] = [AbilitySlots]uint16{}
	s.Level[r] = [AbilitySlots]uint8{}
	s.ReadyAt[r] = [AbilitySlots]uint32{}
	s.CastState[r] = [AbilitySlots]uint8{}
	s.Mana[r] = 0
	s.MaxMana[r] = 0
	s.ManaRegen[r] = 0
	s.CastSlot[r] = -1
	s.CastEnd[r] = 0
	s.Entity[r] = id
	s.rowOf[idx] = r
	s.count++
	return true
}

func (s *AbilityStore) Remove(id EntityID) bool {
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
		s.AbilityID[r] = s.AbilityID[last]
		s.Level[r] = s.Level[last]
		s.ReadyAt[r] = s.ReadyAt[last]
		s.CastState[r] = s.CastState[last]
		s.Mana[r] = s.Mana[last]
		s.MaxMana[r] = s.MaxMana[last]
		s.ManaRegen[r] = s.ManaRegen[last]
		s.CastSlot[r] = s.CastSlot[last]
		s.CastEnd[r] = s.CastEnd[last]
		moved := s.Entity[last]
		s.Entity[r] = moved
		s.rowOf[moved.Index()] = r
	}
	s.rowOf[idx] = -1
	s.count--
	return true
}

func (s *AbilityStore) Row(id EntityID) int32 {
	idx := id.Index()
	if idx >= uint32(len(s.rowOf)) {
		return -1
	}
	return s.rowOf[idx]
}

func (s *AbilityStore) Count() int32 { return s.count }

func (s *AbilityStore) assert(msg string, id EntityID) {
	if s.DebugAssert != nil {
		s.DebugAssert(msg, id)
	}
}

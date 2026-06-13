package sim

// HealthStore (ecs-architecture.md §5): life, regen, armor, death
// state. T2 pattern — see store_transform.go. Life and regen are
// 32.32 fixed-point; regen is the PER-TICK increment (a data table's
// "0.25 life/s" becomes 0.0125/tick at load — no per-second floats).

import "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"

// Death states (phase-7/decay machinery consumes these).
const (
	DeathAlive    uint8 = iota
	DeathDying          // death animation playing
	DeathDecaying       // corpse decay timer running
)

type HealthStore struct {
	Life         []fixed.F64
	MaxLife      []fixed.F64
	Regen        []fixed.F64 // life per TICK
	ArmorValue   []int16
	ArmorType    []uint8 // indexes the damage table (combat-and-orders.md)
	DeathState   []uint8 // Death* constants
	DecayTicks   []uint32
	Invulnerable []bool // damage packets are skipped while true (#365)
	Entity       []EntityID

	rowOf []int32
	count int32

	DebugAssert func(msg string, id EntityID)
}

func NewHealthStore(rowCap, entityCap int) *HealthStore {
	if rowCap <= 0 || entityCap <= 0 || rowCap > entityCap {
		panic("sim: store caps must satisfy 0 < rowCap <= entityCap")
	}
	s := &HealthStore{
		Life:         make([]fixed.F64, rowCap),
		MaxLife:      make([]fixed.F64, rowCap),
		Regen:        make([]fixed.F64, rowCap),
		ArmorValue:   make([]int16, rowCap),
		ArmorType:    make([]uint8, rowCap),
		DeathState:   make([]uint8, rowCap),
		DecayTicks:   make([]uint32, rowCap),
		Invulnerable: make([]bool, rowCap),
		Entity:       make([]EntityID, rowCap),
		rowOf:        make([]int32, entityCap),
	}
	for i := range s.rowOf {
		s.rowOf[i] = -1
	}
	return s
}

func (s *HealthStore) Add(e *Entities, id EntityID, maxLife, regenPerTick fixed.F64, armorValue int16, armorType uint8) bool {
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
	s.Life[r] = maxLife // spawn at full life
	s.MaxLife[r] = maxLife
	s.Regen[r] = regenPerTick
	s.ArmorValue[r] = armorValue
	s.ArmorType[r] = armorType
	s.DeathState[r] = DeathAlive
	s.DecayTicks[r] = 0
	s.Invulnerable[r] = false // units spawn vulnerable
	s.Entity[r] = id
	s.rowOf[idx] = r
	s.count++
	return true
}

func (s *HealthStore) Remove(id EntityID) bool {
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
		s.Life[r] = s.Life[last]
		s.MaxLife[r] = s.MaxLife[last]
		s.Regen[r] = s.Regen[last]
		s.ArmorValue[r] = s.ArmorValue[last]
		s.ArmorType[r] = s.ArmorType[last]
		s.DeathState[r] = s.DeathState[last]
		s.DecayTicks[r] = s.DecayTicks[last]
		s.Invulnerable[r] = s.Invulnerable[last]
		moved := s.Entity[last]
		s.Entity[r] = moved
		s.rowOf[moved.Index()] = r
	}
	s.rowOf[idx] = -1
	s.count--
	return true
}

func (s *HealthStore) Row(id EntityID) int32 {
	idx := id.Index()
	if idx >= uint32(len(s.rowOf)) {
		return -1
	}
	return s.rowOf[idx]
}

func (s *HealthStore) Count() int32 { return s.count }

func (s *HealthStore) assert(msg string, id EntityID) {
	if s.DebugAssert != nil {
		s.DebugAssert(msg, id)
	}
}

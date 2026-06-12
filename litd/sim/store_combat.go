package sim

// CombatStore (ecs-architecture.md §5, combat-and-orders.md §3): two
// weapon slots per unit, every duration an integer tick count, every
// cooldown an absolute "ready at tick T" clock — never a float
// accumulator. T2 pattern — see store_transform.go. Weapon slot 1
// zero-valued = unused (a melee-only unit simply never touches it).

import "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"

// WeaponSlots is the per-unit weapon count (WC3's attack 1 / attack 2).
const WeaponSlots = 2

// CooldownReady reports whether an absolute ready-at clock has come
// due. The signed-difference compare is wrap-safe at the uint32
// boundary (and 2^32 ticks is ~6.8 years of game time at 20/s, so a
// single match can never be ambiguous).
func CooldownReady(tick, readyAt uint32) bool { return int32(tick-readyAt) >= 0 }

type CombatStore struct {
	// per weapon slot
	DmgBase    [][WeaponSlots]int32  // flat damage
	DmgDice    [][WeaponSlots]uint8  // Ndice
	DmgSides   [][WeaponSlots]uint8  // roll(sides)
	AttackType [][WeaponSlots]uint8  // indexes the damage matrix
	Cooldown   [][WeaponSlots]uint16 // full attack period, ticks
	DamagePt   [][WeaponSlots]uint16 // windup ticks to FIRE
	Range      [][WeaponSlots]fixed.F64
	ProjRef    [][WeaponSlots]uint16 // projectile type row; 0 = instant
	ReadyAt    [][WeaponSlots]uint32 // absolute next-attack tick

	// per unit
	AcquisitionRange []fixed.F64
	Target           []EntityID // current attack target (0 = none)
	LastAttacker     []EntityID // damage memory (return-fire, AI)
	LastDamagedTick  []uint32
	Entity           []EntityID

	rowOf []int32
	count int32

	DebugAssert func(msg string, id EntityID)
}

func NewCombatStore(rowCap, entityCap int) *CombatStore {
	if rowCap <= 0 || entityCap <= 0 || rowCap > entityCap {
		panic("sim: store caps must satisfy 0 < rowCap <= entityCap")
	}
	s := &CombatStore{
		DmgBase:          make([][WeaponSlots]int32, rowCap),
		DmgDice:          make([][WeaponSlots]uint8, rowCap),
		DmgSides:         make([][WeaponSlots]uint8, rowCap),
		AttackType:       make([][WeaponSlots]uint8, rowCap),
		Cooldown:         make([][WeaponSlots]uint16, rowCap),
		DamagePt:         make([][WeaponSlots]uint16, rowCap),
		Range:            make([][WeaponSlots]fixed.F64, rowCap),
		ProjRef:          make([][WeaponSlots]uint16, rowCap),
		ReadyAt:          make([][WeaponSlots]uint32, rowCap),
		AcquisitionRange: make([]fixed.F64, rowCap),
		Target:           make([]EntityID, rowCap),
		LastAttacker:     make([]EntityID, rowCap),
		LastDamagedTick:  make([]uint32, rowCap),
		Entity:           make([]EntityID, rowCap),
		rowOf:            make([]int32, entityCap),
	}
	for i := range s.rowOf {
		s.rowOf[i] = -1
	}
	return s
}

// Add attaches a zero-valued Combat row (all weapon slots unused);
// the spawner fills columns from the unit's data-table row.
func (s *CombatStore) Add(e *Entities, id EntityID) bool {
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
	s.DmgBase[r] = [WeaponSlots]int32{}
	s.DmgDice[r] = [WeaponSlots]uint8{}
	s.DmgSides[r] = [WeaponSlots]uint8{}
	s.AttackType[r] = [WeaponSlots]uint8{}
	s.Cooldown[r] = [WeaponSlots]uint16{}
	s.DamagePt[r] = [WeaponSlots]uint16{}
	s.Range[r] = [WeaponSlots]fixed.F64{}
	s.ProjRef[r] = [WeaponSlots]uint16{}
	s.ReadyAt[r] = [WeaponSlots]uint32{}
	s.AcquisitionRange[r] = 0
	s.Target[r] = 0
	s.LastAttacker[r] = 0
	s.LastDamagedTick[r] = 0
	s.Entity[r] = id
	s.rowOf[idx] = r
	s.count++
	return true
}

func (s *CombatStore) Remove(id EntityID) bool {
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
		s.DmgBase[r] = s.DmgBase[last]
		s.DmgDice[r] = s.DmgDice[last]
		s.DmgSides[r] = s.DmgSides[last]
		s.AttackType[r] = s.AttackType[last]
		s.Cooldown[r] = s.Cooldown[last]
		s.DamagePt[r] = s.DamagePt[last]
		s.Range[r] = s.Range[last]
		s.ProjRef[r] = s.ProjRef[last]
		s.ReadyAt[r] = s.ReadyAt[last]
		s.AcquisitionRange[r] = s.AcquisitionRange[last]
		s.Target[r] = s.Target[last]
		s.LastAttacker[r] = s.LastAttacker[last]
		s.LastDamagedTick[r] = s.LastDamagedTick[last]
		moved := s.Entity[last]
		s.Entity[r] = moved
		s.rowOf[moved.Index()] = r
	}
	s.rowOf[idx] = -1
	s.count--
	return true
}

func (s *CombatStore) Row(id EntityID) int32 {
	idx := id.Index()
	if idx >= uint32(len(s.rowOf)) {
		return -1
	}
	return s.rowOf[idx]
}

func (s *CombatStore) Count() int32 { return s.count }

func (s *CombatStore) assert(msg string, id EntityID) {
	if s.DebugAssert != nil {
		s.DebugAssert(msg, id)
	}
}

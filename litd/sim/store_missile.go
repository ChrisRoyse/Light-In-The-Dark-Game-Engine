package sim

// MissileStore (#158, ADR #295 / R-SIM-7): missiles are independent
// first-class sim entities — Transform + this T2 store, never
// cosmetic attack attachments. Flight integrates at the movement-
// phase tail (missile.go); death goes through the standard deferred
// kill; scripts and abilities address missiles as ordinary entities
// (mid-flight retarget = a GuideEnt write).
//
// The payload is EITHER a compiled effect list (#296) or a plain
// DamagePacket value rolled at launch (the degenerate built-in
// weapon) — Payload.Len > 0 selects the effect list.

import (
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
)

// Missile flag bits.
const (
	// MissileAoE: when the guide target invalidates mid-flight the
	// missile continues to the last known position and detonates;
	// without it the missile expires payload-less (WC3's expire-vs-
	// AoE flag, generalized).
	MissileAoE uint8 = 1 << 0
)

type MissileStore struct {
	Speed     []fixed.F64       // world units per tick
	Arc       []fixed.F64       // presentation arc height (render-only; flight is straight)
	Flags     []uint8           // Missile* bits
	GuideEnt  []EntityID        // homing target; 0 = point missile
	GuidePt   []fixed.Vec2      // goal point / last known target position
	Payload   []data.EffectList // compiled effect list; Len 0 = Packet variant
	Packet    []DamagePacket    // rolled-at-launch degenerate payload
	Source    []EntityID        // launcher (may die mid-flight; delivery unaffected)
	BirthTick []uint32
	Entity    []EntityID

	rowOf []int32
	count int32

	DebugAssert func(msg string, id EntityID)
}

func NewMissileStore(rowCap, entityCap int) *MissileStore {
	if rowCap <= 0 || entityCap <= 0 || rowCap > entityCap {
		panic("sim: store caps must satisfy 0 < rowCap <= entityCap")
	}
	s := &MissileStore{
		Speed:     make([]fixed.F64, rowCap),
		Arc:       make([]fixed.F64, rowCap),
		Flags:     make([]uint8, rowCap),
		GuideEnt:  make([]EntityID, rowCap),
		GuidePt:   make([]fixed.Vec2, rowCap),
		Payload:   make([]data.EffectList, rowCap),
		Packet:    make([]DamagePacket, rowCap),
		Source:    make([]EntityID, rowCap),
		BirthTick: make([]uint32, rowCap),
		Entity:    make([]EntityID, rowCap),
		rowOf:     make([]int32, entityCap),
	}
	for i := range s.rowOf {
		s.rowOf[i] = -1
	}
	return s
}

func (s *MissileStore) Add(e *Entities, id EntityID) bool {
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
	s.Speed[r] = 0
	s.Arc[r] = 0
	s.Flags[r] = 0
	s.GuideEnt[r] = 0
	s.GuidePt[r] = fixed.Vec2{}
	s.Payload[r] = data.EffectList{}
	s.Packet[r] = DamagePacket{}
	s.Source[r] = 0
	s.BirthTick[r] = 0
	s.Entity[r] = id
	s.rowOf[idx] = r
	s.count++
	return true
}

func (s *MissileStore) Remove(id EntityID) bool {
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
		s.Arc[r] = s.Arc[last]
		s.Flags[r] = s.Flags[last]
		s.GuideEnt[r] = s.GuideEnt[last]
		s.GuidePt[r] = s.GuidePt[last]
		s.Payload[r] = s.Payload[last]
		s.Packet[r] = s.Packet[last]
		s.Source[r] = s.Source[last]
		s.BirthTick[r] = s.BirthTick[last]
		moved := s.Entity[last]
		s.Entity[r] = moved
		s.rowOf[moved.Index()] = r
	}
	s.rowOf[idx] = -1
	s.count--
	return true
}

func (s *MissileStore) Row(id EntityID) int32 {
	idx := id.Index()
	if idx >= uint32(len(s.rowOf)) {
		return -1
	}
	return s.rowOf[idx]
}

func (s *MissileStore) Count() int32 { return s.count }

func (s *MissileStore) assert(msg string, id EntityID) {
	if s.DebugAssert != nil {
		s.DebugAssert(msg, id)
	}
}

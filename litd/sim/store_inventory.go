package sim

// InventoryStore (ecs-architecture.md §5): six WC3-style item slots
// holding item ENTITY refs — items are entities with their own
// components, and campaign persistence saves their type IDs (§5.3).
// T2 pattern — see store_transform.go. Slot value 0 = empty.

import "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"

// InventorySlots is the WC3-style six-slot inventory.
const InventorySlots = 6

type InventoryStore struct {
	Slots [][InventorySlots]EntityID
	// ClassReady is the per-item-CLASS use-cooldown expiry tick
	// (#305 — items of one class share a cooldown, WC3-style).
	ClassReady [][data.ItemClassCount]uint32
	Entity     []EntityID

	rowOf []int32
	count int32

	DebugAssert func(msg string, id EntityID)
}

func NewInventoryStore(rowCap, entityCap int) *InventoryStore {
	if rowCap <= 0 || entityCap <= 0 || rowCap > entityCap {
		panic("sim: store caps must satisfy 0 < rowCap <= entityCap")
	}
	s := &InventoryStore{
		Slots:      make([][InventorySlots]EntityID, rowCap),
		ClassReady: make([][data.ItemClassCount]uint32, rowCap),
		Entity:     make([]EntityID, rowCap),
		rowOf:      make([]int32, entityCap),
	}
	for i := range s.rowOf {
		s.rowOf[i] = -1
	}
	return s
}

// Add attaches an empty inventory.
func (s *InventoryStore) Add(e *Entities, id EntityID) bool {
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
	s.Slots[r] = [InventorySlots]EntityID{}
	s.ClassReady[r] = [data.ItemClassCount]uint32{}
	s.Entity[r] = id
	s.rowOf[idx] = r
	s.count++
	return true
}

// SetSlot places an item entity in a slot. Fails closed on an
// out-of-range slot index or an occupied slot (the asserts fire; the
// inventory is untouched).
func (s *InventoryStore) SetSlot(id EntityID, slot int, item EntityID) bool {
	r := s.Row(id)
	if r == -1 {
		s.assert("SetSlot on absent inventory", id)
		return false
	}
	if slot < 0 || slot >= InventorySlots {
		s.assert("SetSlot out of range", id)
		return false
	}
	if s.Slots[r][slot] != 0 {
		s.assert("SetSlot on occupied slot", id)
		return false
	}
	s.Slots[r][slot] = item
	return true
}

// ClearSlot empties a slot, returning the item that was there.
func (s *InventoryStore) ClearSlot(id EntityID, slot int) (EntityID, bool) {
	r := s.Row(id)
	if r == -1 || slot < 0 || slot >= InventorySlots {
		s.assert("ClearSlot invalid", id)
		return 0, false
	}
	item := s.Slots[r][slot]
	s.Slots[r][slot] = 0
	return item, item != 0
}

func (s *InventoryStore) Remove(id EntityID) bool {
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
		s.Slots[r] = s.Slots[last]
		s.ClassReady[r] = s.ClassReady[last]
		moved := s.Entity[last]
		s.Entity[r] = moved
		s.rowOf[moved.Index()] = r
	}
	s.rowOf[idx] = -1
	s.count--
	return true
}

func (s *InventoryStore) Row(id EntityID) int32 {
	idx := id.Index()
	if idx >= uint32(len(s.rowOf)) {
		return -1
	}
	return s.rowOf[idx]
}

func (s *InventoryStore) Count() int32 { return s.count }

// UnitInventorySize returns the unit's item-carry capacity: InventorySlots for
// a unit that has an inventory, 0 otherwise. The engine models inventory as
// all-or-nothing (a unit either carries the full six slots or none), so this is
// the faithful capacity for GetUnitInventorySize / UnitInventorySize.
func (w *World) UnitInventorySize(id EntityID) int32 {
	if w.Invents.Row(id) != -1 {
		return InventorySlots
	}
	return 0
}

func (s *InventoryStore) assert(msg string, id EntityID) {
	if s.DebugAssert != nil {
		s.DebugAssert(msg, id)
	}
}

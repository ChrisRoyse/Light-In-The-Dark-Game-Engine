package sim

// Items (#305, ecs-architecture.md §5, items.md): items are ENTITIES.
// A ground item is an entity with a Transform (and spatial-bucket
// membership) and an ItemStore row — no collision, units walk through
// it. A carried item keeps its row (Carrier = the unit) and loses its
// Transform; the carrier's InventoryStore slot points at the item
// entity, so type AND instance state (charges) travel with it.
//
// Pickup/drop/give run instantly when adjacent (within itemReach) and
// refuse deterministically with a named reason otherwise; the
// OrderPickup verb (move→take, combat-and-orders.md §2.2) drives the
// approach through the movement system. Use executes the item's
// compiled effect pipeline (ADR #294 — the same arena abilities use),
// decrements charges, removes consumables at 0, and arms the
// per-CLASS use cooldown on the carrier (WC3 semantics: two potions
// of one class share a cooldown). Carried stat modifiers fold into
// the #162 derived-stat cache on pickup/drop — permanent hidden
// modifiers, no buff instances.
//
// Death: a dying carrier grounds drop-on-death items at deterministic
// footprint-adjacent cells and destroys the rest with itself; a dying
// item clears its carrier slot. Everything is counted and evented —
// nothing vanishes silently.

import (
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
)

// Item events (#305).
const (
	// EvItemPickedUp: Src = unit, Dst = item, Arg = inventory slot.
	EvItemPickedUp uint16 = 16
	// EvItemUsed: Src = unit, Dst = item, Arg = remaining charges.
	EvItemUsed uint16 = 17
	// EvItemDropped: Src = unit, Dst = item, Arg = 1 death-drop /
	// 0 ordered drop.
	EvItemDropped uint16 = 18
)

// OrderPickup drives move→take onto a ground item; Target = the item.
const OrderPickup uint8 = 7

// Item operation outcomes (one vocabulary for pickup/drop/give/use —
// the FSV trace reasons).
const (
	ItemOK          uint8 = 0
	ItemBadItem     uint8 = 1 // no item row / defs unbound / empty slot
	ItemNotGround   uint8 = 2 // pickup target is already carried
	ItemNoInventory uint8 = 3
	ItemFull        uint8 = 4
	ItemTooFar      uint8 = 5
	ItemNoSpace     uint8 = 6  // no free ground cell to drop into
	ItemNotOwned    uint8 = 7  // give across players
	ItemNotUsable   uint8 = 8  // passive item (no use pipeline)
	ItemOnCooldown  uint8 = 9  // class cooldown armed
	ItemBadTarget   uint8 = 10 // targeted use: missing/out-of-range target
)

// itemReach is the manipulation range (pickup/drop/give), unit center
// to item/recipient center.
var itemReach = fixed.FromInt(128)

// ItemRecord is the D-15 instance snapshot: type ID + instance
// fields, no EntityIDs (#208 campaign carry-over).
type ItemRecord struct {
	TypeID  uint16
	Charges uint16
}

// ---- item store (T2 pattern) ----

type ItemStore struct {
	TypeID  []uint16
	Charges []uint16
	Carrier []EntityID // 0 = on the ground
	Entity  []EntityID

	rowOf []int32
	count int32
}

func NewItemStore(rowCap, entityCap int) *ItemStore {
	if rowCap <= 0 || entityCap <= 0 || rowCap > entityCap {
		panic("sim: store caps must satisfy 0 < rowCap <= entityCap")
	}
	s := &ItemStore{
		TypeID:  make([]uint16, rowCap),
		Charges: make([]uint16, rowCap),
		Carrier: make([]EntityID, rowCap),
		Entity:  make([]EntityID, rowCap),
		rowOf:   make([]int32, entityCap),
	}
	for i := range s.rowOf {
		s.rowOf[i] = -1
	}
	return s
}

func (s *ItemStore) add(e *Entities, id EntityID, typeID, charges uint16) bool {
	if !e.Alive(id) || s.rowOf[id.Index()] != -1 || int(s.count) == len(s.TypeID) {
		return false
	}
	r := s.count
	s.TypeID[r] = typeID
	s.Charges[r] = charges
	s.Carrier[r] = 0
	s.Entity[r] = id
	s.rowOf[id.Index()] = r
	s.count++
	return true
}

func (s *ItemStore) Remove(id EntityID) bool {
	r := s.rowOf[id.Index()]
	if r == -1 {
		return false
	}
	last := s.count - 1
	s.TypeID[r] = s.TypeID[last]
	s.Charges[r] = s.Charges[last]
	s.Carrier[r] = s.Carrier[last]
	s.Entity[r] = s.Entity[last]
	s.rowOf[s.Entity[r].Index()] = r
	s.rowOf[id.Index()] = -1
	s.count--
	return true
}

func (s *ItemStore) Row(id EntityID) int32 {
	if int(id.Index()) >= len(s.rowOf) {
		return -1
	}
	return s.rowOf[id.Index()]
}
func (s *ItemStore) Count() int32 { return s.count }

// ---- world surface ----

// BindItemDefs installs the loaded item rows. Usable items (an
// effects list) require the effect arena to already cover their
// composition — fail-closed, never a runtime index panic. Rebinding
// a different table is refused.
func (w *World) BindItemDefs(defs []data.Item) bool {
	if len(defs) == 0 || len(defs) > 1<<16 {
		return false
	}
	if w.itemDefs != nil && len(w.itemDefs) != len(defs) {
		return false
	}
	for i := range defs {
		e := defs[i].Effects
		if e.Len > 0 && int(e.Off)+int(e.Len) > len(w.effects) {
			return false
		}
	}
	w.itemDefs = defs
	idx := make(map[string]uint16, len(defs))
	for i := range defs {
		if defs[i].ID != "" {
			idx[defs[i].ID] = uint16(i)
		}
	}
	w.itemDefByCode = idx
	return true
}

// ItemTypeID resolves an item code (data.Item.ID) to its bound typeID.
// ok=false for an unknown code or before BindItemDefs. Deterministic map
// lookup built once at bind, never iterated in gameplay.
func (w *World) ItemTypeID(code string) (uint16, bool) {
	id, ok := w.itemDefByCode[code]
	return id, ok
}

// ItemTypeCount returns the number of bound item types.
func (w *World) ItemTypeCount() int { return len(w.itemDefs) }

// ItemTypeOf returns the item entity's type id. ok=false for a non-item.
func (w *World) ItemTypeOf(item EntityID) (uint16, bool) {
	r := w.Items.Row(item)
	if r == -1 {
		return 0, false
	}
	return w.Items.TypeID[r], true
}

// ItemCharges returns the item entity's current charges. ok=false for a
// non-item.
func (w *World) ItemCharges(item EntityID) (uint16, bool) {
	r := w.Items.Row(item)
	if r == -1 {
		return 0, false
	}
	return w.Items.Charges[r], true
}

// SetItemCharges sets the item's charges. No-op (false) for a non-item.
// Does not auto-destroy at 0 — consumable removal happens through UseItem.
func (w *World) SetItemCharges(item EntityID, charges uint16) bool {
	r := w.Items.Row(item)
	if r == -1 {
		return false
	}
	w.Items.Charges[r] = charges
	return true
}

// ItemCarrier returns the unit carrying the item, or 0 if the item is on
// the ground (or not an item).
func (w *World) ItemCarrier(item EntityID) EntityID {
	r := w.Items.Row(item)
	if r == -1 {
		return 0
	}
	return w.Items.Carrier[r]
}

// ItemGroundPos returns a ground item's world position. ok=false if the
// item is carried (no Transform) or not an item.
func (w *World) ItemGroundPos(item EntityID) (fixed.Vec2, bool) {
	if w.Items.Row(item) == -1 {
		return fixed.Vec2{}, false
	}
	tr := w.Transforms.Row(item)
	if tr == -1 {
		return fixed.Vec2{}, false
	}
	return w.Transforms.Pos[tr], true
}

// AddInventory attaches an empty 6-slot inventory to a unit.
func (w *World) AddInventory(id EntityID) bool {
	return w.Invents.Add(w.Ents, id)
}

// SpawnItem creates a ground item of typeID at pos with the table's
// initial charges.
func (w *World) SpawnItem(typeID uint16, pos fixed.Vec2) (EntityID, bool) {
	if w.itemDefs == nil || int(typeID) >= len(w.itemDefs) {
		return 0, false
	}
	id, ok := w.CreateUnit(pos, 0)
	if !ok {
		return 0, false
	}
	if !w.Items.add(w.Ents, id, typeID, w.itemDefs[typeID].Charges) {
		w.DestroyUnit(id)
		return 0, false
	}
	return id, true
}

// ItemAt reads the D-15 record of the item in a unit's slot.
func (w *World) ItemAt(unit EntityID, slot int) (ItemRecord, bool) {
	ir := w.Invents.Row(unit)
	if ir == -1 || slot < 0 || slot >= InventorySlots {
		return ItemRecord{}, false
	}
	item := w.Invents.Slots[ir][slot]
	if item == 0 {
		return ItemRecord{}, false
	}
	r := w.Items.Row(item)
	if r == -1 {
		return ItemRecord{}, false
	}
	return ItemRecord{TypeID: w.Items.TypeID[r], Charges: w.Items.Charges[r]}, true
}

// ItemInSlot returns the item entity in a unit's inventory slot, or 0 if
// the slot is empty, out of range, or the unit has no inventory.
func (w *World) ItemInSlot(unit EntityID, slot int) EntityID {
	ir := w.Invents.Row(unit)
	if ir == -1 || slot < 0 || slot >= InventorySlots {
		return 0
	}
	return w.Invents.Slots[ir][slot]
}

// PickupItem moves a ground item into the unit's first free slot.
// Instant when within itemReach; OrderPickup drives the approach.
func (w *World) PickupItem(unit, item EntityID) uint8 {
	return w.takeItem(unit, item, true)
}

// AddItemToInventory force-grants a ground item into the unit's first
// free slot regardless of distance (UnitAddItem semantics). The item is
// detached from the ground (Transform + bucket) and carried. Same
// failure vocabulary as PickupItem minus ItemTooFar.
func (w *World) AddItemToInventory(unit, item EntityID) uint8 {
	return w.takeItem(unit, item, false)
}

// takeItem is the shared pickup core; requireReach gates on itemReach
// (instant pickup) vs force-grant.
func (w *World) takeItem(unit, item EntityID, requireReach bool) uint8 {
	r := w.Items.Row(item)
	if r == -1 || w.itemDefs == nil || !w.Ents.Alive(item) {
		return ItemBadItem
	}
	if w.Items.Carrier[r] != 0 {
		return ItemNotGround
	}
	ir := w.Invents.Row(unit)
	if ir == -1 || !w.Ents.Alive(unit) {
		return ItemNoInventory
	}
	tr, ur := w.Transforms.Row(item), w.Transforms.Row(unit)
	if tr == -1 || ur == -1 {
		return ItemBadItem
	}
	if requireReach && !fixed.DistSqLess(w.Transforms.Pos[ur], w.Transforms.Pos[tr], itemReach) {
		return ItemTooFar
	}
	// Power-up: consumed the instant it is taken — the effect fires on the
	// taker and the item is destroyed rather than stored (no slot needed).
	if def := &w.itemDefs[w.Items.TypeID[r]]; def.PowerUp {
		w.ExecuteEffects(def.Effects, EffectCtx{Source: unit, Target: unit})
		w.Emit(Event{Kind: EvItemUsed, Src: unit, Dst: item, Arg: 0})
		w.DestroyUnit(item)
		return ItemOK
	}
	slot := -1
	for s := 0; s < InventorySlots; s++ {
		if w.Invents.Slots[ir][s] == 0 {
			slot = s
			break
		}
	}
	if slot == -1 {
		return ItemFull
	}
	w.bucketRemove(item)
	w.Transforms.Remove(item)
	w.Items.Carrier[r] = unit
	w.Invents.SetSlot(unit, slot, item)
	w.recomputeBuffStats(unit) // fold the carried modifiers in
	w.Emit(Event{Kind: EvItemPickedUp, Src: unit, Dst: item, Arg: int64(slot)})
	return ItemOK
}

// DropItem grounds the item in a unit's slot at a deterministic
// adjacent cell.
func (w *World) DropItem(unit EntityID, slot int) uint8 {
	ir := w.Invents.Row(unit)
	if ir == -1 {
		return ItemNoInventory
	}
	if slot < 0 || slot >= InventorySlots || w.Invents.Slots[ir][slot] == 0 {
		return ItemBadItem
	}
	item := w.Invents.Slots[ir][slot]
	pos, ok := w.itemDropCell(unit, slot)
	if !ok {
		return ItemNoSpace
	}
	w.Invents.ClearSlot(unit, slot)
	w.groundItem(item, pos)
	w.recomputeBuffStats(unit)
	w.Emit(Event{Kind: EvItemDropped, Src: unit, Dst: item, Arg: 0})
	return ItemOK
}

// GiveItem hands the item in `from`'s slot to an adjacent unit of the
// same player (first free slot).
func (w *World) GiveItem(from EntityID, slot int, to EntityID) uint8 {
	fr := w.Invents.Row(from)
	tr := w.Invents.Row(to)
	if fr == -1 || tr == -1 || !w.Ents.Alive(to) {
		return ItemNoInventory
	}
	if slot < 0 || slot >= InventorySlots || w.Invents.Slots[fr][slot] == 0 {
		return ItemBadItem
	}
	fo, to2 := w.Owners.Row(from), w.Owners.Row(to)
	if fo == -1 || to2 == -1 || w.Owners.Player[fo] != w.Owners.Player[to2] {
		return ItemNotOwned
	}
	ft, tt := w.Transforms.Row(from), w.Transforms.Row(to)
	if ft == -1 || tt == -1 ||
		!fixed.DistSqLess(w.Transforms.Pos[ft], w.Transforms.Pos[tt], itemReach) {
		return ItemTooFar
	}
	free := -1
	for s := 0; s < InventorySlots; s++ {
		if w.Invents.Slots[tr][s] == 0 {
			free = s
			break
		}
	}
	if free == -1 {
		return ItemFull
	}
	item := w.Invents.Slots[fr][slot]
	w.Invents.ClearSlot(from, slot)
	w.Invents.SetSlot(to, free, item)
	w.Items.Carrier[w.Items.Row(item)] = to
	w.recomputeBuffStats(from)
	w.recomputeBuffStats(to)
	w.Emit(Event{Kind: EvItemDropped, Src: from, Dst: item, Arg: 0})
	w.Emit(Event{Kind: EvItemPickedUp, Src: to, Dst: item, Arg: int64(free)})
	return ItemOK
}

// SwapSlots reorders two slots of one inventory (pure bookkeeping —
// the derived fold is slot-order independent).
func (w *World) SwapSlots(unit EntityID, a, b int) bool {
	ir := w.Invents.Row(unit)
	if ir == -1 || a < 0 || a >= InventorySlots || b < 0 || b >= InventorySlots {
		return false
	}
	w.Invents.Slots[ir][a], w.Invents.Slots[ir][b] = w.Invents.Slots[ir][b], w.Invents.Slots[ir][a]
	return true
}

// UseItem executes the slot item's use pipeline against
// target/point. Charges decrement; a consumable at 0 charges is
// destroyed; the item's CLASS goes on cooldown for this carrier.
func (w *World) UseItem(unit EntityID, slot int, target EntityID, point fixed.Vec2) uint8 {
	ir := w.Invents.Row(unit)
	if ir == -1 || !w.Ents.Alive(unit) {
		return ItemNoInventory
	}
	if slot < 0 || slot >= InventorySlots || w.Invents.Slots[ir][slot] == 0 || w.itemDefs == nil {
		return ItemBadItem
	}
	item := w.Invents.Slots[ir][slot]
	r := w.Items.Row(item)
	if r == -1 {
		return ItemBadItem
	}
	def := &w.itemDefs[w.Items.TypeID[r]]
	if def.Effects.Len == 0 {
		return ItemNotUsable
	}
	if w.Invents.ClassReady[ir][def.Class] > w.tick {
		return ItemOnCooldown
	}
	if def.Targeted {
		tt := w.Transforms.Row(target)
		ut := w.Transforms.Row(unit)
		if !w.Ents.Alive(target) || tt == -1 || ut == -1 ||
			!fixed.DistSqLess(w.Transforms.Pos[ut], w.Transforms.Pos[tt], def.UseRange) {
			return ItemBadTarget
		}
	}
	w.ExecuteEffects(def.Effects, EffectCtx{Source: unit, Target: target, Point: point})
	w.Invents.ClassReady[ir][def.Class] = w.tick + uint32(def.CooldownTicks)
	if w.Items.Charges[r] > 0 {
		w.Items.Charges[r]--
	}
	w.Emit(Event{Kind: EvItemUsed, Src: unit, Dst: item, Arg: int64(w.Items.Charges[r])})
	if def.Consumable && w.Items.Charges[r] == 0 {
		w.Invents.ClearSlot(unit, slot)
		w.Items.Carrier[r] = 0 // detached; DestroyUnit needs no slot scan
		w.DestroyUnit(item)
		w.recomputeBuffStats(unit)
	}
	return ItemOK
}

// groundItem re-attaches a Transform and bucket membership at pos.
func (w *World) groundItem(item EntityID, pos fixed.Vec2) {
	r := w.Items.Row(item)
	w.Items.Carrier[r] = 0
	w.Transforms.Add(w.Ents, item, pos, 0)
	w.bucketInsert(item, pos)
	w.MarkSnap(item)
}

// itemDropCell scans footprint-adjacent cells like spawnCell, but
// rotated by `seed` so multi-item death drops spread deterministically
// instead of stacking one candidate.
func (w *World) itemDropCell(unit EntityID, seed int) (fixed.Vec2, bool) {
	return w.adjacentCell(unit, w.unitCollision(unit), seed)
}

func (w *World) unitCollision(unit EntityID) int32 {
	if ut := w.UnitTypes.Row(unit); ut != -1 {
		if tid := w.UnitTypes.TypeID[ut]; int(tid) < len(w.unitDefs) {
			return w.unitDefs[tid].CollisionSize
		}
	}
	return 0
}

// detachItem clears a dying item's carrier slot (direct item
// destruction — script kills, consumable removal already detaches).
func (w *World) detachItem(item EntityID) {
	r := w.Items.Row(item)
	if r == -1 || w.Items.Carrier[r] == 0 {
		return
	}
	carrier := w.Items.Carrier[r]
	w.Items.Carrier[r] = 0
	ir := w.Invents.Row(carrier)
	if ir == -1 {
		return
	}
	for s := 0; s < InventorySlots; s++ {
		if w.Invents.Slots[ir][s] == item {
			w.Invents.ClearSlot(carrier, s)
			break
		}
	}
	if w.Ents.Alive(carrier) {
		w.recomputeBuffStats(carrier)
	}
}

// releaseInventory resolves a dying carrier's slots: drop-on-death
// items ground at deterministic adjacent cells; everything else dies
// with the carrier (DestroyUnit, before the inventory row goes).
func (w *World) releaseInventory(unit EntityID, ir int32) {
	for s := 0; s < InventorySlots; s++ {
		item := w.Invents.Slots[ir][s]
		if item == 0 {
			continue
		}
		w.Invents.Slots[ir][s] = 0
		r := w.Items.Row(item)
		if r == -1 {
			continue
		}
		w.Items.Carrier[r] = 0
		drop := false
		if w.itemDefs != nil && int(w.Items.TypeID[r]) < len(w.itemDefs) {
			drop = w.itemDefs[w.Items.TypeID[r]].DropOnDeath
		}
		if drop {
			if pos, ok := w.itemDropCell(unit, s); ok {
				w.groundItem(item, pos)
				w.Emit(Event{Kind: EvItemDropped, Src: unit, Dst: item, Arg: 1})
				continue
			}
		}
		w.DestroyUnit(item)
	}
}

// foldItemStats writes the carried-modifier contributions (called by
// recomputeBuffStats between the hero-attribute fold and live buffs).
// Slot-ascending order; per mod row Add sums, Permille multiplies.
func (w *World) foldItemStats(id EntityID) {
	if w.itemDefs == nil {
		return
	}
	ir := w.Invents.Row(id)
	if ir == -1 {
		return
	}
	idx := id.Index()
	for s := 0; s < InventorySlots; s++ {
		item := w.Invents.Slots[ir][s]
		if item == 0 {
			continue
		}
		r := w.Items.Row(item)
		if r == -1 {
			continue
		}
		def := &w.itemDefs[w.Items.TypeID[r]]
		for mi := range def.Mods {
			m := &def.Mods[mi]
			w.buffAdd[m.Stat][idx] += m.Add
			if m.Permille != 1000 {
				w.buffMult[m.Stat][idx] = w.buffMult[m.Stat][idx].
					Mul(fixed.FromInt(m.Permille).Div(fixed.FromInt(1000)))
			}
		}
	}
}

// drivePickup advances one unit's OrderPickup (orders.go phase 3):
// approach through the movement system, take when within reach,
// complete false when the item is gone, taken, or unreachable.
func (w *World) drivePickup(r int32, id EntityID) {
	s := w.Orders
	item := s.Target[r]
	ir := w.Items.Row(item)
	if ir == -1 || !w.Ents.Alive(item) || w.Items.Carrier[ir] != 0 {
		w.completeOrder(r, id, false)
		return
	}
	switch w.PickupItem(id, item) {
	case ItemOK:
		w.completeOrder(r, id, true)
		return
	case ItemTooFar:
		// keep approaching
	default:
		w.completeOrder(r, id, false) // full inventory etc: visible failure
		return
	}
	mr := w.Movements.Row(id)
	if mr == -1 {
		w.completeOrder(r, id, false)
		return
	}
	tr := w.Transforms.Row(item)
	if s.Phase[r] == orderFresh {
		if !w.StartMoveTo(id, w.Transforms.Pos[tr]) {
			w.completeOrder(r, id, false)
			return
		}
		s.Phase[r] = orderRunning
		return
	}
	switch w.Movements.State[mr] {
	case MoveIdle: // arrived but still out of reach: unreachable
		w.completeOrder(r, id, false)
	case MoveBlocked:
		w.completeOrder(r, id, false)
	}
}

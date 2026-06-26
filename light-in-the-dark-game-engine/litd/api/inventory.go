package litd

// Unit inventory surface (#225, items.md). Inventory verbs hang off the Unit
// noun (R-API-1: the unit is the receiver). The deterministic sim ItemStore +
// InventoryStore are the source of truth; every verb here forwards to the sim
// and reports success by the sim's outcome code (sim.ItemOK == 0). A unit
// must have an inventory (EnableInventory) before it can carry items.

import "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"

// EnableInventory attaches an empty six-slot inventory to the unit, returning
// true if one was added (false if the unit already had one or the handle is
// invalid). JASS: inventory is a unit-type ability in WC3; here it is an
// explicit capability grant.
func (u Unit) EnableInventory() bool {
	if !u.Valid() {
		u.g.reportInvalid("Unit.EnableInventory")
		return false
	}
	return u.g.w.AddInventory(u.id)
}

// AddItem force-grants an existing ground item into the unit's first free
// slot, regardless of distance (UnitAddItem semantics). Returns true on
// success; false if the inventory is full, the item is already carried, the
// unit has no inventory, or either handle is invalid. JASS: UnitAddItem.
// JASS: UnitAddItem, UnitAddItemSwapped
func (u Unit) AddItem(it Item) bool {
	if !u.Valid() || !it.Valid() {
		if u.g != nil {
			u.g.reportInvalid("Unit.AddItem")
		}
		return false
	}
	return u.g.w.AddItemToInventory(u.id, it.id) == sim.ItemOK
}

// SlotOption configures AddItemByType (R-API-3 functional option).
type SlotOption func(*slotConfig)

type slotConfig struct {
	slot int // -1 = first free slot
}

// WithSlot requests that AddItemByType place the new item in a specific slot.
// If the slot is occupied (or out of range) the add fails cleanly and no item
// is left in play. JASS: UnitAddItemToSlotById.
func WithSlot(n int) SlotOption {
	return func(c *slotConfig) { c.slot = n }
}

// AddItemByType creates an item of typ at the unit's position and grants it to
// the unit, returning the new Item handle (the zero Item on failure — null
// type, entity cap, full inventory, or an occupied requested slot). The
// convenience form of CreateItem + AddItem. JASS: UnitAddItemById /
// UnitAddItemToSlotById (with WithSlot).
// JASS: UnitAddItemById, UnitAddItemByIdSwapped, UnitAddItemToSlotById
func (u Unit) AddItemByType(typ ItemType, opts ...SlotOption) Item {
	if !u.Valid() {
		if u.g != nil {
			u.g.reportInvalid("Unit.AddItemByType")
		}
		return Item{}
	}
	cfg := slotConfig{slot: -1}
	for _, o := range opts {
		o(&cfg)
	}
	if cfg.slot >= 0 && (cfg.slot >= u.InventorySize() || u.g.w.ItemInSlot(u.id, cfg.slot) != 0) {
		u.g.reportInvalid("Unit.AddItemByType (requested slot occupied or out of range)")
		return Item{}
	}
	it := u.g.CreateItem(typ, u.Position())
	if !it.Valid() {
		return Item{} // CreateItem already reported
	}
	if u.g.w.AddItemToInventory(u.id, it.id) != sim.ItemOK {
		it.Remove() // no partial spawn left on the ground
		u.g.reportInvalid("Unit.AddItemByType (inventory full)")
		return Item{}
	}
	if cfg.slot >= 0 {
		// item landed in the first free slot; move it to the requested one.
		for s := 0; s < u.InventorySize(); s++ {
			if u.g.w.ItemInSlot(u.id, s) == it.id && s != cfg.slot {
				u.g.w.SwapSlots(u.id, s, cfg.slot)
				break
			}
		}
	}
	return it
}

// ItemInSlot returns the item in the unit's slot n, or the zero Item if the
// slot is empty, out of range, or the unit has no inventory. JASS:
// UnitItemInSlot.
// JASS: UnitItemInSlot, UnitItemInSlotBJ
func (u Unit) ItemInSlot(n int) Item {
	if !u.Valid() {
		u.g.reportInvalid("Unit.ItemInSlot")
		return Item{}
	}
	id := u.g.w.ItemInSlot(u.id, n)
	if id == 0 {
		return Item{}
	}
	return Item{id: id, g: u.g}
}

// ItemCount returns the number of items the unit is carrying. Zero on an
// invalid handle or a unit without an inventory. JASS: UnitInventoryCount.
// JASS: UnitInventoryCount
func (u Unit) ItemCount() int {
	if !u.Valid() {
		u.g.reportInvalid("Unit.ItemCount")
		return 0
	}
	n := 0
	for s := 0; s < u.InventorySize(); s++ {
		if u.g.w.ItemInSlot(u.id, s) != 0 {
			n++
		}
	}
	return n
}

// DropItem grounds the item in slot n at a deterministic adjacent cell.
// Returns true on success; false on an empty/out-of-range slot, no free
// ground cell, no inventory, or an invalid handle. JASS: UnitDropItemSlot /
// UnitDropItemPoint collapse here (drop position is deterministic).
// JASS: UnitDropItem, UnitDropItemPoint, UnitDropItemPointBJ, UnitDropItemPointLoc, UnitDropItemSlot, UnitDropItemSlotBJ, UnitRemoveItem, UnitRemoveItemFromSlot, UnitRemoveItemFromSlotSwapped, UnitRemoveItemSwapped, WidgetDropItem
func (u Unit) DropItem(n int) bool {
	if !u.Valid() {
		u.g.reportInvalid("Unit.DropItem")
		return false
	}
	return u.g.w.DropItem(u.id, n) == sim.ItemOK
}

// GiveItemTo hands the item in slot n to an adjacent unit of the same player.
// Returns true on success; false if out of range, the recipient is full or a
// foreign player's, the slot is empty, or a handle is invalid. JASS:
// UnitDropItemTarget.
// JASS: UnitDropItemTarget, UnitDropItemTargetBJ
func (u Unit) GiveItemTo(n int, to Unit) bool {
	if !u.Valid() || !to.Valid() {
		if u.g != nil {
			u.g.reportInvalid("Unit.GiveItemTo")
		}
		return false
	}
	return u.g.w.GiveItem(u.id, n, to.id) == sim.ItemOK
}

// SwapItems reorders two of the unit's inventory slots. Returns true on
// success; false on out-of-range slots, no inventory, or an invalid handle.
func (u Unit) SwapItems(a, b int) bool {
	if !u.Valid() {
		u.g.reportInvalid("Unit.SwapItems")
		return false
	}
	return u.g.w.SwapSlots(u.id, a, b)
}

// UseOption configures Unit.UseItem (R-API-3 functional option).
type UseOption func(*useConfig)

type useConfig struct {
	target Unit
	point  Vec2
	hasPt  bool
}

// UseOn directs a targeted item's effect at a unit. JASS: the target arg of
// UnitUseItemTarget.
func UseOn(target Unit) UseOption {
	return func(c *useConfig) { c.target = target }
}

// UseAt directs an item's effect at a ground point. JASS: the point args of
// UnitUseItemPoint.
func UseAt(point Vec2) UseOption {
	return func(c *useConfig) { c.point = point; c.hasPt = true }
}

// UseItem fires the item in slot n's use pipeline, decrementing charges and
// arming the item class's cooldown on the unit; a consumable at 0 charges is
// destroyed. Returns true on success; false if the item is passive, on
// cooldown, the slot is empty, a required target is missing/out-of-range, or
// the handle is invalid. JASS: UnitUseItem / UnitUseItemTarget /
// UnitUseItemPoint collapse here via UseOn/UseAt.
// JASS: UnitUseItem, UnitUseItemPoint, UnitUseItemPointLoc, UnitUseItemTarget
func (u Unit) UseItem(n int, opts ...UseOption) bool {
	if !u.Valid() {
		u.g.reportInvalid("Unit.UseItem")
		return false
	}
	var cfg useConfig
	for _, o := range opts {
		o(&cfg)
	}
	var target sim.EntityID
	if cfg.target.Valid() {
		target = cfg.target.id
	}
	return u.g.w.UseItem(u.id, n, target, vec(cfg.point)) == sim.ItemOK
}

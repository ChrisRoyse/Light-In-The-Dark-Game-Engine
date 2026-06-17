package litd

// Item surface (#225, items.md; public-api-design.md §2 row 5). An item is a
// pickup entity: a ground item carries a Transform; a carried item lives in a
// unit's inventory slot and travels type + instance state (charges) with the
// carrier. The deterministic sim is the source of truth — type id, charges,
// and carrier all read back from the ItemStore. Inventory verbs live on Unit
// (inventory.go); this file is the Item noun and its ctor.

// ItemType names a bound item type. The zero value is the null type (an
// unknown code or no item table bound), and CreateItem rejects it.
type ItemType struct {
	ref uint16 // typeID + 1; 0 = null
}

// IsZero reports the null item type.
func (t ItemType) IsZero() bool { return t.ref == 0 }

// ItemType resolves an item code (e.g. "potion") to its bound type, or the
// null ItemType if the code is unknown or no item table is bound. JASS: the
// 'xxxx' rawcodes passed to CreateItem.
func (g *Game) ItemType(code string) ItemType {
	if g == nil || g.w == nil {
		return ItemType{}
	}
	if id, ok := g.w.ItemTypeID(code); ok {
		return ItemType{ref: id + 1}
	}
	return ItemType{}
}

// CreateItem spawns a ground item of type typ at pos with the table's initial
// charges, returning its handle (the zero Item on failure — null/unknown type
// or the entity cap reached). JASS: CreateItem, CreateItemLoc collapse here;
// the returned handle replaces the bj_lastCreatedItem side channel
// (GetLastCreatedItem tombstoned).
// JASS: CreateItem, CreateItemLoc
func (g *Game) CreateItem(typ ItemType, pos Vec2) Item {
	if g == nil || g.w == nil {
		return Item{}
	}
	if typ.IsZero() {
		g.reportInvalid("Game.CreateItem (null ItemType)")
		return Item{}
	}
	id, ok := g.w.SpawnItem(typ.ref-1, vec(pos))
	if !ok {
		g.reportInvalid("Game.CreateItem (spawn failed: entity cap or unbound type)")
		return Item{}
	}
	return Item{id: id, g: g}
}

// Type returns the item's type handle, or the null ItemType on an invalid
// handle. JASS: GetItemTypeId.
// JASS: GetItemTypeId
func (i Item) Type() ItemType {
	if !i.Valid() {
		i.g.reportInvalid("Item.Type")
		return ItemType{}
	}
	if tid, ok := i.g.w.ItemTypeOf(i.id); ok {
		return ItemType{ref: tid + 1}
	}
	return ItemType{}
}

// Charges returns the item's current charge count, or 0 on an invalid handle.
// JASS: GetItemCharges
func (i Item) Charges() int {
	if !i.Valid() {
		i.g.reportInvalid("Item.Charges")
		return 0
	}
	c, _ := i.g.w.ItemCharges(i.id)
	return int(c)
}

// SetCharges sets the item's charge count (clamped to the uint16 range).
// Setting 0 does not destroy the item — consumable removal happens through
// Unit.UseItem. No-op on an invalid handle. JASS: SetItemCharges.
// JASS: SetItemCharges
func (i Item) SetCharges(n int) {
	if !i.Valid() {
		i.g.reportInvalid("Item.SetCharges")
		return
	}
	if n < 0 {
		n = 0
	}
	if n > 0xFFFF {
		n = 0xFFFF
	}
	i.g.w.SetItemCharges(i.id, uint16(n))
}

// Carried reports whether the item is currently in a unit's inventory (as
// opposed to lying on the ground). False on an invalid handle.
func (i Item) Carried() bool {
	return i.Valid() && i.g.w.ItemCarrier(i.id) != 0
}

// Carrier returns the unit carrying the item, or the zero Unit if the item is
// on the ground or the handle is invalid. JASS: GetItemPlayer maps to the
// carrier's owner via Carrier().Owner().
func (i Item) Carrier() Unit {
	if !i.Valid() {
		i.g.reportInvalid("Item.Carrier")
		return Unit{}
	}
	c := i.g.w.ItemCarrier(i.id)
	if c == 0 {
		return Unit{}
	}
	return Unit{id: c, g: i.g}
}

// Position returns a ground item's world position, or the zero Vec2 if the
// item is carried (no ground position) or the handle is invalid. JASS:
// GetItemX/GetItemY/GetItemLoc collapse here.
// JASS: GetItemLoc, GetItemX, GetItemY
func (i Item) Position() Vec2 {
	if !i.Valid() {
		i.g.reportInvalid("Item.Position")
		return Vec2{}
	}
	if p, ok := i.g.w.ItemGroundPos(i.id); ok {
		return Vec2{X: toFloat(p.X), Y: toFloat(p.Y)}
	}
	return Vec2{}
}

// Remove destroys the item, taking it out of play. A carried item is removed
// from its carrier's inventory first. No-op on an invalid handle. JASS:
// RemoveItem.
// JASS: RemoveItem
func (i Item) Remove() {
	if !i.Valid() {
		i.g.reportInvalid("Item.Remove")
		return
	}
	i.g.w.DestroyUnit(i.id)
}

# Items — JASS → Go Mapping

> Part of the [JASS API mapping](README.md). Governing rules: PRD [§4.2 dedup D1–D5, §4.3 API shape](../../../PRD.md).

## Surface size (grep survey, 2026-06-11)

| Source | Approx. count | Notes |
|---|---|---|
| `common.j` natives | **~93** | item CRUD/state, inventory (`UnitAddItem`/`UnitItemInSlot`/`UnitUseItem`), shop stock (`AddItemToStock`), item-type queries, random-item pools |
| `blizzard.j` BJs | **~65** | `...Loc`/`...BJ`/`Swap` wrappers, `GetLastCreatedItem`, random-item-pool helpers |

## Representative JASS signatures

```jass
native CreateItem       takes integer itemid, real x, real y returns item
native GetItemTypeId    takes item i returns integer
native SetItemCharges   takes item whichItem, integer charges returns nothing
native UnitAddItem      takes unit whichUnit, item whichItem returns boolean
native UnitItemInSlot   takes unit whichUnit, integer itemSlot returns item
native UnitUseItem      takes unit whichUnit, item whichItem returns boolean

function CreateItemLoc takes integer itemId, location loc returns item
function GetLastCreatedItem takes nothing returns item
function SetItemPositionLoc takes item whichItem, location loc returns nothing
function UnitAddItemToSlotById takes unit whichUnit, integer itemId, integer itemSlot returns boolean
```

## Canonical Go surface

```go
type Item struct{ /* opaque handle */ }
type ItemType string // four-cc analogue; row key into data tables

func (g *Game) CreateItem(typ ItemType, pos Vec2) Item

func (i Item) Type() ItemType
func (i Item) Position() Vec2
func (i Item) SetPosition(p Vec2)
func (i Item) Charges() int
func (i Item) SetCharges(n int)
func (i Item) Owner() Player          // SetItemPlayer/GetItemPlayer
func (i Item) SetDroppable(b bool)
func (i Item) SetInvulnerable(b bool)
func (i Item) Remove()

// Inventory lives on Unit (noun-method rule R-API-1):
func (u Unit) AddItem(i Item) bool
func (u Unit) AddItemByType(t ItemType, opts ...ItemOption) (Item, bool) // slot via WithSlot(n)
func (u Unit) ItemInSlot(slot int) Item
func (u Unit) UseItem(i Item, target OrderTarget) bool  // use/usePoint/useTarget unified
func (u Unit) DropItem(i Item, at Vec2) bool
func (u Unit) Inventory() []Item

// Shop stock:
func (u Unit) AddItemToStock(t ItemType, current, max int)
func (u Unit) RemoveItemFromStock(t ItemType)
```

## Dedup rules applied

| Rule | Application | Example |
|---|---|---|
| **D1** | passthrough BJs dropped | `RemoveItemBJ`, `SetItemChargesBJ` → methods above |
| **D2** | id+slot convenience collapses onto options | `UnitAddItemToSlotById` → `AddItemByType(t, WithSlot(n))`; `GetLastCreatedItem` deleted (constructor returns) |
| **D3** | `...Loc` variants → `Vec2` | `CreateItemLoc`, `SetItemPositionLoc` → `CreateItem(t, pos)`, `SetPosition(p)` |
| **D4** | random-item-pool logic kept once | `ChooseRandomItemEx`/`RandomDistribution*` item helpers → `helpers.RandomItemType(level, class)` over the seeded sim PRNG |
| **D5** | boolean flag get/set pairs → typed accessors | `SetItemPawnable`/`IsItemPawnable` → `Item.SetPawnable(b)`/`Item.Pawnable()` |

`UnitUseItem`/`UnitUseItemPoint`/`UnitUseItemTarget` is the same D3 collapse as unit
orders: one `UseItem(i, target OrderTarget)`.

## Subsystem dependencies

- **sim** (primary): item entities in the ECS (ground items have transforms), inventory component per unit (fixed 6-slot array — preallocated, R-GC-2), stock tables, pickup/drop/use events through the event bus.
- **render**: ground-item models (GLB from CC0 packs), inventory icons in HUD (UI layer).
- **asset**: item-type stat rows in `data/` JSON (R-AST-1) — cost, charges, abilities granted, classification (permanent/charged/powerup/artifact); icon atlas entries.

## Porting hazards

1. **Items grant abilities** — a charged item is mostly a container for an ability with charges. Sequence the [abilities-and-buffs](abilities-and-buffs.md) data model first; item port depends on it.
2. **Powerup semantics**: powerup items (runes, tomes) auto-consume on pickup and may be picked up with full inventory. This is special-cased engine behavior, not derivable from signatures — capture in the item-class data schema.
3. **Manipulation events** (`EVENT_PLAYER_UNIT_PICKUP_ITEM`, `...DROP_ITEM`, `...USE_ITEM`, `...PAWN_ITEM` and the `GetManipulatedItem` response) belong to the event payload schema in [triggers-and-events](triggers-and-events.md) — coordinate so `e.Item()` exists.
4. **Stock replenish timers** (BJ `PerformStockUpdates` machinery) is real logic (D4): port once as a sim system driven by data-table restock intervals, not as script helpers.
5. **Item ownership vs unit ownership**: `SetItemPlayer` affects shop purchasability, not control. Easy to conflate; test both.

# Key-Value Store — Public API & Lua Binding

---

## 1. Go API (`litd/api`)

Typed accessors keep the tagged union safe at the boundary.

```go
// Entity-scoped (on any Unit / Item / Destructible / Region handle that wraps an EntityID)
func (u Unit) SetInt(key string, v int64)
func (u Unit) GetInt(key string) (int64, bool)
func (u Unit) SetReal(key string, v float64)     // float64 ↔ fixed.F64 at the boundary
func (u Unit) GetReal(key string) (float64, bool)
func (u Unit) SetBool(key string, v bool)
func (u Unit) GetBool(key string) (bool, bool)
func (u Unit) SetString(key, v string)
func (u Unit) GetString(key string) (string, bool)
func (u Unit) SetUnit(key string, v Unit)
func (u Unit) GetUnit(key string) (Unit, bool)
func (u Unit) SetPoint(key string, v Vec2)
func (u Unit) GetPoint(key string) (Vec2, bool)
func (u Unit) SetGroupRef(key string, v Group)
func (u Unit) GetGroupRef(key string) (Group, bool)
func (u Unit) Has(key string) bool
func (u Unit) DeleteKey(key string)
func (u Unit) EachKey(fn func(key string))

// Global and player scope
func (g *Game) SetGlobalInt(key string, v int64)
func (g *Game) GetGlobalInt(key string) (int64, bool)
// ... mirror typed setters/getters for global ...
func (p Player) SetInt(key string, v int64)
func (p Player) GetInt(key string) (int64, bool)
// ... mirror typed setters/getters for player ...
```

`key` strings are interned on first use; callers pass plain strings. (A perf-sensitive
caller may pre-intern with `g.KeyID("enemyCount")` and use the id form, but this is
optional.)

## 2. Lua binding (`litd/luabind`)

Lua is dynamically typed, so the binding offers `SetKV`/`GetKV` that infer the variant from
the Lua value, plus explicit typed forms when the author wants to pin a type.

```lua
-- entity scope (u is a unit handle)
SetKV(u, "enemyCount", 30)              -- inferred KVInt
SetKV(u, "spawnAngle", 1.57)            -- inferred KVFixed (number with fraction)
SetKV(u, "slotType", "weapon")          -- inferred KVString
SetKV(u, "spawnerGroup", campGroup)     -- inferred KVGroup
SetKV(u, "home", spawner)               -- inferred KVEntity (unit handle)

local n   = GetKV(u, "enemyCount")      -- 30
local typ = GetKV(u, "slotType")        -- "weapon"
local ok  = HasKV(u, "spawnAngle")
DeleteKV(u, "enemyCount")

-- explicit typed forms (avoid int/float ambiguity)
SetKVInt(u, "state", 0)
SetKVReal(u, "ratio", 0.5)

-- global & player scope
SetGlobalKV("bossPhase", 2)
local phase = GetGlobalKV("bossPhase")
SetPlayerKV(1, "score", 1500)
```

> **Determinism note for Lua authors.** `GetKV` on an absent key returns `nil`. Iteration
> via `EachKV(u, fn)` visits keys in interned-id order (deterministic), never Lua-table
> order. Never iterate a raw Lua table to drive sim mutation — use the KV store, which is
> ordered and serialized.

## 3. Mapping to WC3 / JASS

| JASS / WC3 hashtable | PRD2 |
|----------------------|------|
| `GetUnitUserData` / `SetUnitUserData` | `Unit.GetInt(reservedKey)` / `Unit.SetInt(...)` (back-compat shim, R-KV-5) |
| `SaveInteger(hash, parent, child, v)` | `SetKVInt(owner, key, v)` (the `(parent,child)` pair maps to `(owner, key)`) |
| `LoadInteger(hash, parent, child)` | `GetKVInt(owner, key)` |
| `SaveStr` / `LoadStr` | `SetKVString` / `GetKVString` |
| `SaveUnitHandle` / `LoadUnitHandle` | `SetKV(owner,key,unit)` / `GetKVUnit` |
| `HaveSavedInteger` etc. | `HasKV(owner, key)` |
| `FlushChildHashtable` | `KVClearOwner(owner)` |

The WC3 hashtable's `(parentKey, childKey)` integer pair is exactly the `(owner, key)`
shape; PRD2 makes `owner` a real handle and `key` an interned string, which is both safer
and more legible than the raw integer hashtable.

## 4. Worked patterns

### Spawner config (tutorial: enemy spawner)
```lua
-- at init, configure each spawner marker
SetKV(marker, "enemyType", "skeleton")
SetKV(marker, "enemyCount", 6)
SetKV(marker, "spawnAngle", 0.0)
SetKV(marker, "group", NewGroup())

-- when the timer fires, read config and spawn into the group
local g = GetKV(marker, "group")
for i = 1, GetKV(marker, "enemyCount") do
    local u = Spawn(GetKV(marker, "enemyType"), RandomPointIn(marker))
    GroupAdd(g, u)
    SetKV(u, "home", marker)         -- back-link for respawn bookkeeping
end
```

### Quest state machine (tutorial: gathering quest)
```lua
SetKVInt(questItem, "state", 0)               -- 0=not started,1=active,2=done
-- ...
if GetKV(questItem, "state") == 1 and collected >= target then
    SetKVInt(questItem, "state", 2)
    SetWidgetText(reminderUI, "Quest complete")
end
```

### Equipment slot limit (tutorial: equipment limit)
```lua
SetKV(weaponItem, "slotType", "weapon")
-- on pickup
if HasKV(hero, "equipped_weapon") then
    DropItem(hero, GetKV(hero, "equipped_weapon"))
end
SetKV(hero, "equipped_weapon", weaponItem)
```

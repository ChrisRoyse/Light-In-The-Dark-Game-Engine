# Unit-Group Store — Public API & Lua Binding

---

## 1. Go API (`litd/api`)

```go
// Group is a value handle over a sim GroupID. Stale ⇒ no-op.
type Group struct { /* g *Game; id sim.GroupID */ }

func (g *Game) NewGroup() Group
func (gr Group) Destroy()
func (gr Group) Valid() bool

// membership
func (gr Group) Add(u Unit)
func (gr Group) Remove(u Unit)          // fast swap-remove
func (gr Group) RemoveOrdered(u Unit)   // stable
func (gr Group) Clear()
func (gr Group) Count() int
func (gr Group) Contains(u Unit) bool
func (gr Group) First() Unit            // zero Unit if empty
func (gr Group) Each(fn func(u Unit))   // insertion order; safe to Remove inside

// set algebra (write into receiver)
func (gr Group) Union(a, b Group)
func (gr Group) Intersect(a, b Group)
func (gr Group) Difference(a, b Group)
func (gr Group) CopyFrom(src Group)

// query-fill (clears receiver first), value-type geometry only
func (gr Group) FillRadius(center Vec2, radius float64, q Query)
func (gr Group) FillRect(min, max Vec2, q Query)
func (gr Group) FillOwner(p Player, q Query)
func (gr Group) FillType(t UnitType, q Query)

// Query is an options struct (R-API-4): filter predicates with sane zero-value defaults.
type Query struct {
    AliveOnly   bool
    Enemy       Player   // if set, keep only enemies of this player
    Ally        Player
    Structures  TriState // include/exclude/only
    Flying      TriState
    Max         int      // cap results (0 = unlimited); deterministic truncation by visit order
}
```

## 2. Lua binding (`litd/luabind`)

Mirrors JASS `group` ergonomics so ported logic and AI-authored scripts read naturally.

```lua
local g = NewGroup()

-- fill all enemy units of player 1 within 300 of a point, then damage each
GroupFillRadius(g, castPoint, 300, { enemyOf = 1, aliveOnly = true })
GroupEach(g, function(u)
    DealDamage(caster, u, 500, DAMAGE_MAGIC)
end)

GroupCount(g)            -- quest progress / emptiness
GroupContains(g, u)
GroupFirst(g)            -- deterministic first member
GroupAdd(g, u)
GroupRemove(g, u)
GroupClear(g)

-- set algebra
GroupUnion(dst, a, b)
GroupIntersect(dst, a, b)
GroupDifference(dst, a, b)

DestroyGroup(g)
```

### Iteration safety
`GroupEach` snapshots the member count at entry and tolerates `GroupRemove` of the current
unit inside the loop (the swap-remove backfills the slot; the snapshot bound prevents
visiting the moved tail twice). This matches the most common WC3 idiom
(`ForGroup` + kill).

## 3. Mapping to WC3 / JASS

| JASS | PRD2 |
|------|------|
| `CreateGroup` / `DestroyGroup` | `NewGroup` / `Group.Destroy` |
| `GroupAddUnit` / `GroupRemoveUnit` | `Group.Add` / `Group.Remove` |
| `GroupEnumUnitsInRange` | `Group.FillRadius` |
| `GroupEnumUnitsInRect` | `Group.FillRect` |
| `GroupEnumUnitsOfPlayer` | `Group.FillOwner` |
| `ForGroup(g, code)` | `Group.Each(fn)` / `GroupEach` |
| `FirstOfGroup` | `Group.First()` |
| `CountUnitsInGroup` | `Group.Count()` |
| `IsUnitInGroup` | `Group.Contains()` |

## 4. Worked patterns from the tutorial corpus

### One-click AOE kill
```lua
local g = NewGroup()
GroupFillRadius(g, casterPoint, 800, { enemyOf = casterOwner, aliveOnly = true })
GroupEach(g, function(u) Kill(u) end)
```

### Spawner emptiness drives respawn (with [timer](../01-timer-wheel/))
```lua
-- camp.group holds the camp's living spawns
OnEvent(EVENT_UNIT_DEATH, function(ev)
    local u = EventUnit(ev)
    if GroupContains(camp.group, u) then
        GroupRemove(camp.group, u)
        if GroupCount(camp.group) == 0 then
            After(10.0, function() RespawnCamp(camp) end)  -- 10s respawn
        end
    end
end)
```

### Lowest-HP target (tutorial: healer AI)
```lua
local g = NewGroup()
GroupFillRadius(g, healer.pos, 1200, { allyOf = healer.owner, aliveOnly = true })
local best, bestFrac = nil, 2.0
GroupEach(g, function(u)
    local f = UnitLifeFraction(u)
    if f < bestFrac then best, bestFrac = u, f end
end)
if best then IssueTargetOrder(healer, "heal", best) end
```

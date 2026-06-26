# 03 — Generic Typed Key-Value Store

> **Requirement namespace:** `R-KV-*`
> **Supersedes:** `UserDataStore` (int32-only, `litd/sim/store_userdata.go`)
> **Primary tick phase:** none autonomous; written by mutating systems; pruned in cleanup
> **New sub-hash:** `"kv"`
> **New caps:** `Caps.KVPairs` (default 32,768)

---

## Problem

Authors constantly need to attach **arbitrary custom attributes** to game objects:
- A spawner region needs `enemyType`, `enemyCount`, `spawnAngle`.
- A quest item needs `state ∈ {0,1,2}`.
- A piece of equipment needs `slotType = "weapon"` and a `modifierRef`.
- A spawned unit needs a back-link to its `spawnerGroup`.
- An NPC needs its `dialogTableRow`.

The tutorial corpus dedicates an entire concept ("custom key value = primary index + key
+ value") to this, and builds spawners, quests, equipment, and dialog on top of it.

Our engine offers only `UserDataStore`: **one `int32` per unit**
(`store_userdata.go:10`). That cannot hold a string tag, a float, a `Vec2`, an entity
back-reference, or more than one value per object. Authors are forced into parallel Lua
tables keyed by unit id — nondeterministic iteration, garbage, no serialization.

## Solution

A **generic typed key-value store**: attach any number of `(key → value)` pairs to any
sim object (or to global/player scope), where the value is a **tagged union** over the
sim's primitive types. Keys are interned strings with stable integer IDs. Lookup is
O(log n) binary search over sorted `(owner, key)` pairs — **no Go map in hashed state**.
Fully serializable and hashed; auto-prunes pairs whose owner dies.

## Value types (the tagged union — R-KV-2)

| Tag | Go type | Use |
|-----|---------|-----|
| `KVInt` | `int64` | counts, states, flags-as-int |
| `KVFixed` | `fixed.F64` | percentages, ratios, distances |
| `KVBool` | `bool` | toggles |
| `KVString` | interned string id | tags ("weapon"), names, dialog keys |
| `KVEntity` | `EntityID` | back-references (spawner↔unit, owner, target) |
| `KVVec2` | `fixed.Vec2` | stored positions, offsets |
| `KVGroup` | `GroupID` | a unit's associated group ([02](../02-unit-groups/)) |
| `KVTimer` | `TimerID` | a unit's owned timer ([01](../01-timer-wheel/)) |

No arbitrary `interface{}` ever enters hashed state; the union is closed and each variant
is a fixed-width value.

## What this unlocks

- **Spawners**: store wave config on the spawner entity; read it when the timer fires.
- **Quests**: `state` per quest object; `Count` patterns combine with [groups](../02-unit-groups/).
- **Equipment / item systems**: type tags and modifier refs per item.
- **Behavior trees / FSMs**: per-unit blackboard values, combined with
  [custom events](../04-custom-events/).
- The [ability](../06-ability-composition/) layer stores per-instance ability parameters as
  KV pairs, making templates parameterizable without code.

## Documents

- [spec.md](spec.md) — value union, interning, scopes, lookup, pruning, back-compat.
- [api.md](api.md) — Go API and Lua binding.
- [test-plan.md](test-plan.md) — determinism, type safety, zero-alloc, FSV.

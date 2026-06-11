# Hashtable & GameCache — JASS → Go Mapping

> Part of the [JASS API mapping](README.md). Governing rules: PRD [§4.2 dedup D1–D5](../../../PRD.md).

## Surface size (grep survey, 2026-06-11)

| Source | Approx. count | Notes |
|---|---|---|
| `common.j` natives | **~130** | `Save*`/`Load*`/`HaveSaved*`/`RemoveSaved*` × every handle type (~100 hashtable natives), plus gamecache `Store*`/`GetStored*`/`Sync*`/`Flush*` |
| `blizzard.j` BJs | **~41** | arg-reordered `SaveRealBJ`-style wrappers, `Save/Load*HandleBJ` family, `InitGameCacheBJ` |

## Representative JASS signatures

```jass
native InitHashtable          takes nothing returns hashtable
native SaveInteger            takes hashtable table, integer parentKey, integer childKey, integer value returns nothing
native SaveUnitHandle         takes hashtable table, integer parentKey, integer childKey, unit whichUnit returns boolean
native LoadInteger            takes hashtable table, integer parentKey, integer childKey returns integer
native FlushParentHashtable   takes hashtable table returns nothing

native InitGameCache          takes string campaignFile returns gamecache
native StoreInteger           takes gamecache cache, string missionKey, string key, integer value returns nothing
native SyncStoredInteger      takes gamecache cache, string missionKey, string key returns nothing

function SaveUnitHandleBJ takes unit whichUnit, integer key, integer missionKey, hashtable table returns boolean
function InitGameCacheBJ takes string campaignFile returns gamecache
```

## Canonical Go surface

This category is **mostly tombstoned, not ported**. Hashtables exist in JASS because
the language has no data structures and no way to attach data to handles. Go has maps,
structs, and closures — the *capability* (associate arbitrary data with keys/handles)
is native to the language.

```go
// What survives, for porter ergonomics and handle-keyed attachment:
type Table[V any] struct{ /* deterministic ordered map, sim-owned */ }
func NewTable[V any]() *Table[V]
func (t *Table[V]) Set(parent, child int64, v V)
func (t *Table[V]) Get(parent, child int64) (V, bool)   // Load* + HaveSaved* in one call
func (t *Table[V]) Delete(parent, child int64)
func (t *Table[V]) DeleteParent(parent int64)            // FlushChildHashtable

// Handle-keyed attachment (the dominant WC3 hashtable use case):
func Attach[V any](h Handle, v V)    // replaces SaveXHandle(ht, GetHandleId(u), ...)
func Attached[V any](h Handle) (V, bool)

// GameCache → persistent campaign storage:
type SaveData struct{ /* key-value file-backed store */ }
func (g *Game) SaveData(name string) *SaveData
func (d *SaveData) SetInt(section, key string, v int)    // StoreInteger
func (d *SaveData) Int(section, key string) (int, bool)
func (d *SaveData) SaveUnitSnapshot(section, key string, u Unit)   // StoreUnit
func (d *SaveData) RestoreUnit(section, key string, owner Player, pos Vec2, facing Angle) Unit
```

`SaveData` is the campaign **cross-map persistence layer, v1 architecture** per
D-2026-06-11-15: game-cache semantics and hero carry-over are built into the sim and the
save format from M3 (retrofit is brutal, build-in is cheap). The campaign menu/mission-flow
UI that consumes it ships with the M8 editor milestone.
*Revised 2026-06-11 per D-2026-06-11-15.*

## Dedup rules applied

| Rule | Application | Example |
|---|---|---|
| **D1** | all 24 `Save/Load*BJ` arg-reorder wrappers dropped | `SaveUnitHandleBJ` → `Attach`/`Table.Set` |
| **D2** | n/a beyond D1 | — |
| **D3** | **the ~80-native type-matrix collapse**: `Save{Integer,Real,Boolean,Str}` + `Save{Unit,Item,Timer,Trigger,…×20+}Handle` × `Load`/`HaveSaved`/`RemoveSaved` → one generic `Table[V]` | Go generics replace the per-type matrix outright |
| **D4** | `RestoreUnit` (gamecache → live unit reconstruction) is real logic, kept once | `SaveData.SaveUnitSnapshot`/`RestoreUnit` — serializes hero level/items/abilities for campaign carry-over |
| **D5** | `Have*`/`Load*` pairs → comma-ok returns | `Get(...) (V, bool)` |

Tombstone justification recorded per function in the audit manifest: "superseded by
Go language facility (map/generics)". No capability lost — the audit's escape hatch
(b) used at scale.

## Subsystem dependencies

- **sim**: `Table`/`Attach` stores are sim-owned and **included in the determinism state hash if written from sim context** — script-attached data is gameplay state (R-SIM-2). Iteration order must be insertion-ordered, never Go map order.
- **render**: none.
- **asset**: `SaveData` persists to disk under the user profile dir (campaign cross-map progress — v1 architecture, D-2026-06-11-15); format is versioned JSON/binary — same pipeline as save games in [game-state-and-melee](game-state-and-melee.md).

## Porting hazards

1. **`SyncStored*` is a multiplayer primitive** (broadcast a local value to all clients — used for local-data sync tricks). It lands at **M7 with lockstep multiplayer** (D-2026-06-11-5): manifest status is "scheduled M7" with a canonical mapping onto the lockstep sync channel — a committed milestone, not a vague v2 tombstone. *Revised 2026-06-11 per D-2026-06-11-5.*
2. **`GetHandleId` arithmetic**: maps do pointer-arithmetic-adjacent tricks with handle ids (offsets, ranges). `Attach` covers attachment; raw `GetHandleId` survives in [math-strings-conversion](math-strings-conversion.md) as an opaque stable id with no recycling guarantees.
3. **Determinism of user tables**: if a map script stores data keyed by something nondeterministic and then iterates, replays diverge. `Table` never exposes iteration in v1 (WC3 hashtables couldn't be iterated either) — keeps the trap closed.
4. **`RestoreUnit` scope creep**: full WC3 semantics (what exactly a gamecache unit snapshot carries) is under-documented; define the snapshot schema explicitly in M2 and tombstone the rest.
5. **GC pressure**: `Attach` on hot paths (per-projectile data) must use pooled storage, not naive `map[Handle]any` boxing (R-GC-2/3).

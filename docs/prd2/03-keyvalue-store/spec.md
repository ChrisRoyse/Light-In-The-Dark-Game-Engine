# Key-Value Store — Specification

## 1. Data model

```go
// litd/sim/kv.go (new)

type KVType uint8
const (
    KVInt KVType = iota
    KVFixed
    KVBool
    KVString   // value holds an interned string id
    KVEntity
    KVVec2     // value uses two columns (Val, Val2)
    KVGroup
    KVTimer
)

type KVStore struct {
    // Sorted parallel arrays keyed by (Owner, Key). Binary search; NO Go map (R-KV-4).
    Owner []uint64   // packed scope+entity (see §3). Sorted ascending, then by Key.
    Key   []uint32   // interned key id
    Type  []uint8    // KVType
    Val   []int64    // primary value bits (int64 / fixed.F64 / bool / string-id / EntityID / Vec2.X / GroupID / TimerID)
    Val2  []int64    // secondary (Vec2.Y); 0 otherwise

    count int32

    keys  internTable // string→id and id→string, serialized (R-KV-3)
    strs  internTable // interned string VALUES (KVString), serialized
}
```

- **Sorted arrays, binary search.** The "map from `(owner,key)` to value" is realized as
  arrays sorted by the composite key. Insert keeps them sorted (binary-search the slot,
  shift). This gives O(log n) get and O(n) worst-case upsert (shift), with **deterministic
  order by construction** and **no Go map in hashed state** (R-KV-4). For write-heavy
  hotspots, see §6 (per-owner contiguous runs make shifts local and cheap in practice).
- **`Val`/`Val2` are raw int64 bits.** Each `KVType` defines how to interpret them. `Vec2`
  uses both columns; every other type uses `Val` only.

## 2. Interning (R-KV-3)

Two intern tables, both serialized:
- **Key intern table**: maps key strings (`"enemyCount"`) to stable `uint32` ids. Keys are
  declared once and reused; the table is small and grows only when a new key string first
  appears. Serialized so a saved `Key` column resolves to the same strings on load.
- **String-value intern table**: `KVString` values (e.g. `"weapon"`) are interned the same
  way, so string values hash and serialize as integers.

Interning is **append-only within a match** (ids never recycle), so ids are stable for the
match's lifetime and across save/load. The tables serialize as `(count, [len,bytes]*)` in
id order.

## 3. Scopes (R-KV-1)

The `Owner` column is a packed `uint64`:

```
[ scope:8 | reserved:8 | entityOrSlot:48 ]
  scope = KVScopeEntity | KVScopeGlobal | KVScopePlayer
```

- **Entity scope:** `entityOrSlot` holds the `EntityID`. Pruned when the entity dies
  (R-KV-8).
- **Global scope:** one shared namespace (`entityOrSlot = 0`). Never auto-pruned. Holds
  map-wide config and quest globals.
- **Player scope:** `entityOrSlot = player slot`. Never auto-pruned. Holds per-player
  counters (gold-beyond-resource, kills, score).

Packing scope into the sort key keeps all of an owner's pairs **contiguous** in the
arrays, which is what makes per-owner iteration and pruning a localized range operation.

## 4. Operations

| Op | Signature (sim) | Semantics |
|----|------------------|-----------|
| Set | `KVSet(owner, key, type, val, val2)` | upsert; binary-search slot; insert-shift if new |
| Get | `KVGet(owner, key) (type, val, val2, ok)` | binary search; `ok=false` if absent |
| Has | `KVHas(owner, key) bool` | |
| Delete | `KVDelete(owner, key)` | remove-shift; keeps sort |
| ClearOwner | `KVClearOwner(owner)` | drop the owner's contiguous run (O(run)) |
| EachOwner | `KVEachOwner(owner, fn)` | iterate the owner's run in key order |

**Type discipline (R-KV-4):** `Get` returns the stored `KVType`; typed accessors
(`KVGetInt`, `KVGetString`, …) check the tag and return `(zero, false)` on mismatch rather
than reinterpreting bits. Reading an absent key returns the type's zero value and
`ok=false`. Writes overwrite the type (a key may change type across writes; the last write
wins, deterministically).

## 5. Back-compat with `UserDataStore` (R-KV-5)

`UserDataStore` is retired. The legacy API is re-expressed:

```go
// old:  GetUnitUserData(u) int32 ; SetUnitUserData(u, v)
// new:  KVGetInt(entity(u), keyUserData) ; KVSet(entity(u), keyUserData, KVInt, v, 0)
```

`keyUserData` is a reserved interned key (id assigned at world setup). Existing maps and
JASS-ported code keep working unchanged through a thin shim; the `"userdata"` sub-hash is
removed and its contribution folds into `"kv"`. A migration note in the changelog and a
one-shot save-format upgrade handle old saves.

## 6. Performance & layout

- **Per-owner contiguity** (from scope packing) means most real workloads — "set five keys
  on this spawner, read them later" — touch one short contiguous run, so insert-shifts are
  cheap and cache-friendly.
- **Optional row index:** a non-hashed `entity index → arrayStart` hint accelerates locating
  an owner's run without a full binary search; rebuilt on load.
- **Zero-alloc:** all columns and intern tables are preallocated to `Caps.KVPairs` and the
  intern-id ceilings. A `Set` that would exceed `Caps.KVPairs` fails (no-op) and increments
  the hashed `kvDropped` counter (R-KV-6). No `append`-growth; the insert-shift operates in
  place within the preallocated backing.

## 7. Hashing (R-KV-7)

Sub-hash `"kv"`: write `count` and `kvDropped`; iterate pairs in array order (already the
canonical ascending `(Owner, Key)` order); for each write `Owner(u64)`, `Key(u32)`,
`Type(u8)`, `Val(i64)`, `Val2(i64)`. Then write the two intern tables in id order
(`count`, then each `(len, bytes)`). The optional row index is not hashed.

Because the arrays are *maintained* in sorted order, hashing is a straight linear pass with
no sort step — the order is an invariant, not a per-hash computation.

## 8. Pruning (R-KV-8)

In phase 7 (cleanup), for each entity that died this tick, `KVClearOwner(entity(dead))`
drops its contiguous run. Global/player-scope pairs are never pruned. Deterministic;
bounded by the dead-entity list.

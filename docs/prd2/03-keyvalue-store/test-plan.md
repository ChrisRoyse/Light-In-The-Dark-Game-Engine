# Key-Value Store — Test & Verification Plan

---

## 1. Unit tests

| ID | Test | Asserts |
|----|------|---------|
| T-KV-1a | Set/Get round-trips every `KVType` (int/fixed/bool/string/entity/vec2/group/timer) | R-KV-1, R-KV-2 |
| T-KV-2a | Closed union: no path stores an arbitrary interface; each variant is fixed width | R-KV-2 |
| T-KV-3a | Key interning is stable: same string ⇒ same id within a match; ids serialize | R-KV-3 |
| T-KV-3b | String-value interning stable; two pairs with value `"weapon"` share an id | R-KV-3 |
| T-KV-4a | Get(absent) ⇒ zero value + `ok=false`; typed-get on a type mismatch ⇒ zero + false (no bit reinterpret) | R-KV-4 |
| T-KV-4b | Arrays stay sorted by `(Owner,Key)` after random insert/delete; binary search finds every present key | R-KV-4 |
| T-KV-5a | `GetUnitUserData`/`SetUnitUserData` shim round-trips via the reserved key | R-KV-5 |
| T-KV-6a | Exceeding `Caps.KVPairs` ⇒ Set no-op + `kvDropped++`, 0 allocs | R-KV-6 |
| T-KV-8a | All entity-scope pairs of a dead unit are gone after cleanup; global/player pairs survive | R-KV-8 |

## 2. Determinism / replay

| ID | Test | Asserts |
|----|------|---------|
| T-KV-DET-1 | 10k-tick run with KV churn across scopes; byte-identical `HashState` across runs and `-race` | R-SIM-1 |
| T-KV-DET-2 | `EachOwner` / `EachKey` visit order is interned-id order, pinned by fixture | R-KV-4 |
| T-KV-DET-3 | `FirstDivergence` localizes a corrupted value byte to `"kv"` | R-SIM-6 |

## 3. Save / load round-trip

| ID | Test | Asserts |
|----|------|---------|
| T-KV-SAV-1 | Save with mixed-type pairs across all three scopes; load; every pair + type identical | R-KV-7 |
| T-KV-SAV-2 | Intern tables round-trip; a `Key`/string-value column resolves to the same strings on load | R-KV-3 |
| T-KV-SAV-3 | Mirror invariant: save bytes == hashed bytes | R-SIM-6 |
| T-KV-SAV-4 | Legacy save (old `userdata` block) upgrades into `kv` under the reserved key | R-KV-5 |

## 4. Zero-alloc gate

| ID | Test | Asserts |
|----|------|---------|
| T-KV-GC-1 | `AllocsPerRun` over Set/Get/Delete steady state (incl. insert-shift) = 0 allocs | R-GC-1 |
| T-KV-GC-2 | Interning a *new* key may allocate once (table growth is bounded & one-time); interning an existing key is 0 allocs | R-GC-1 |

## 5. Integration / FSV

| ID | Scenario | Source-of-truth check |
|----|----------|------------------------|
| F-KV-1 | Spawner reads its KV config when a timer fires | spawned unit type/count/positions match the KV config in the state JSON |
| F-KV-2 | Quest `state` transitions 0→1→2 drive UI text | event log + UI text snapshot at each transition |
| F-KV-3 | Equipment limit: picking up a second weapon drops the first | inventory state JSON shows exactly one weapon; drop event logged |
| F-KV-4 | Save mid-quest; load; quest `state` and spawner config intact | post-load KV dump equals pre-save dump |

## 6. Preflight wiring

- KV determinism fixture in the FULL 10k step; **core set/get/type subtest in `--fast`**.
- `T-KV-GC-1` in the zero-alloc gate.
- Migration test (`T-KV-SAV-4`) gated full-only.

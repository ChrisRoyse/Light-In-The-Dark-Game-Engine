# Unit-Group Store — Test & Verification Plan

---

## 1. Unit tests

| ID | Test | Asserts |
|----|------|---------|
| T-UGR-1a | Add returns ordered, unique membership; re-adding is a no-op | R-UGR-1, R-UGR-2 |
| T-UGR-2a | `Each` visits members in insertion order | R-UGR-2 |
| T-UGR-2b | `RemoveOrdered` preserves order; `Remove` (swap) is O(1) and still deterministic | R-UGR-2, R-UGR-4 |
| T-UGR-3a | A member that dies is absent from `Each`/`Count` the tick after cleanup | R-UGR-3 |
| T-UGR-3b | `First` never returns a stale handle after a death | R-UGR-3 |
| T-UGR-4a | Add/Remove/Contains are zero-alloc at steady state | R-UGR-4 |
| T-UGR-5a | Union/Intersect/Difference correctness vs a reference set, into a destination group | R-UGR-5 |
| T-UGR-5b | Set algebra is zero-alloc (writes into preallocated destination) | R-UGR-5, R-GC-1 |
| T-UGR-6a | `FillRadius`/`FillRect`/`FillOwner`/`FillType` produce identical membership + order across two runs | R-UGR-6 |
| T-UGR-6b | `Query.Max` truncates by deterministic visit order | R-UGR-6 |
| T-UGR-7a | Group-slot exhaustion ⇒ invalid GroupID + `groupDropped++`, 0 allocs | R-UGR-7 |
| T-UGR-7b | Membership-arena exhaustion ⇒ Add no-op + `groupMemberDropped++` | R-UGR-7 |

## 2. Determinism / replay

| ID | Test | Asserts |
|----|------|---------|
| T-UGR-DET-1 | 10k-tick run with group churn (fills, set algebra, pruning); byte-identical `HashState` across runs and `-race` | R-SIM-1 |
| T-UGR-DET-2 | `FirstDivergence` localizes a corrupted member byte to `"unitgroups"` | R-SIM-6 |
| T-UGR-DET-3 | Query-fill order is grid-row-major then entity-index; a fixture pins the exact order | R-UGR-6 |

## 3. Save / load round-trip

| ID | Test | Asserts |
|----|------|---------|
| T-UGR-SAV-1 | Save with many groups of varied sizes (incl. one large, several empty); load; membership + order identical | R-UGR-8 |
| T-UGR-SAV-2 | Mirror invariant: save bytes == hashed bytes (sans non-hashed indices) | R-SIM-6 |
| T-UGR-SAV-3 | Span/free-slot allocator order survives; next `CreateGroup`/`Add` reuses the same slots/spans as a no-save control | R-UGR-8 |
| T-UGR-SAV-4 | Presence/reverse indices are rebuilt on load and produce identical `Contains`/pruning behavior | R-UGR-3 |

## 4. Zero-alloc gate

| ID | Test | Asserts |
|----|------|---------|
| T-UGR-GC-1 | `AllocsPerRun` over create→fill→iterate→algebra→prune→destroy = 0 allocs | R-GC-1 |

## 5. Integration / FSV

| ID | Scenario | Source-of-truth check |
|----|----------|------------------------|
| F-UGR-1 | AOE nova: fill-by-radius, damage each | Event log shows one `EvUnitDamaged` per enemy in radius, none outside; state JSON HP deltas match the distance falloff |
| F-UGR-2 | Spawner camp lifecycle via group emptiness + timer | unit count returns to baseline only after the group empties and the 10s timer fires |
| F-UGR-3 | Save mid-fight with a populated combat group; load; verify the group still drives the same orders | post-load order log matches no-save control |

## 6. Preflight wiring

- Group determinism fixture in the FULL 10k step; the **core fill+algebra subtest in
  `--fast`**.
- `T-UGR-GC-1` added to the zero-alloc gate.
- Heavy churn e2e guarded with `testing.Short()`.

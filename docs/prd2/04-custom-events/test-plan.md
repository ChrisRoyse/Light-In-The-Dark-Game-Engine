# Custom Events — Test & Verification Plan

---

## 1. Unit tests

| ID | Test | Asserts |
|----|------|---------|
| T-EVT-1a | `RegisterEvent(name)` returns a stable id ≥ first custom id | R-EVT-1 |
| T-EVT-5a | Re-registering an existing name returns the same id (idempotent) | R-EVT-5 |
| T-EVT-5b | Registering `Caps.CustomEventKinds+1` distinct names: last ⇒ invalid id + `customEventDropped++` | R-EVT-5 |
| T-EVT-3a | Emit/subscribe on a custom kind dispatches in emission × registration order, identical to a built-in kind | R-EVT-3, R-EVT-6 |
| T-EVT-3b | Ring overflow on custom kinds drops deterministically (same counter path as built-ins) | R-EVT-3 |
| T-EVT-4a | Scalar `Arg`, `EmitGroup`, and `EmitBag` payloads each reach the handler intact | R-EVT-4 |
| T-EVT-6a | A handler cannot distinguish custom vs built-in except by `Kind()`; both share the path | R-EVT-6 |
| T-EVT-8a | A coroutine `WaitForEvent(customKind)` resumes when the kind fires, in FIFO seq order | R-EVT-8 |

## 2. Determinism / replay

| ID | Test | Asserts |
|----|------|---------|
| T-EVT-DET-1 | 10k-tick run driving a state machine + BT entirely via custom events; byte-identical `HashState` across runs and `-race` | R-SIM-1 |
| T-EVT-DET-2 | Registering the same names in the same setup order yields the same ids on two runs | R-EVT-2 |
| T-EVT-DET-3 | The lint flags a custom-kind registration placed outside world setup / driven by nondeterministic input | R-EVT-2 (determinism caveat) |
| T-EVT-DET-4 | `FirstDivergence` localizes a corrupted registry byte to `"customevents"` | R-SIM-6 |

## 3. Save / load round-trip

| ID | Test | Asserts |
|----|------|---------|
| T-EVT-SAV-1 | Save with custom kinds registered and handlers subscribed; load; `EmitEvent(sameName)` reaches the same handlers | R-EVT-2 |
| T-EVT-SAV-2 | Name intern table + `nameToKind` round-trip; ids stable across load | R-EVT-2, R-EVT-7 |
| T-EVT-SAV-3 | Mirror invariant: registry save bytes == hashed bytes | R-SIM-6 |
| T-EVT-SAV-4 | A coroutine parked on `WaitForEvent(customKind)` at save time resumes correctly after load when the kind fires | R-EVT-8 |

## 4. Zero-alloc gate

| ID | Test | Asserts |
|----|------|---------|
| T-EVT-GC-1 | Scalar `Emit` + dispatch steady state = 0 allocs | R-GC-1 |
| T-EVT-GC-2 | `EmitGroup`/`EmitBag` reuse preallocated bag/group handles (bag pool) = 0 allocs at steady state | R-GC-1 |

## 5. Integration / FSV

| ID | Scenario | Source-of-truth check |
|----|----------|------------------------|
| F-EVT-1 | Boss state machine: sleep→battle→transform→death driven by custom events | event log shows each transition fired once, in order; model/skill swap visible in screenshot |
| F-EVT-2 | BT AI: guards patrol/attack, healer heals lowest-HP, all via custom-event dispatch | order log matches the expected branch per unit type |
| F-EVT-3 | Save mid-battle (boss in transform), load, confirm subsequent transitions still fire | post-load transitions match no-save control |

## 6. Preflight wiring

- Custom-event determinism fixture (state machine) in the FULL 10k step; **core
  register/emit/subscribe subtest in `--fast`**.
- Registration-placement lint added to the determinism-lint step (`determlint`).
- Zero-alloc gates added.

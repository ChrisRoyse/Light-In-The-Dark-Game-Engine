# Timer Wheel — Test & Verification Plan

> Acceptance follows FSV (`prompts/fsv.md`): run it, then inspect the source of truth
> (state hash, save bytes, event log), never exit codes alone. Each requirement maps to at
> least one test. All tests live under `litd/sim` (and `litd/luabind` for the Lua path).

---

## 1. Unit tests

| ID | Test | Asserts |
|----|------|---------|
| T-TMR-1a | `single` fires exactly once at `now+Interval` | R-TMR-1 |
| T-TMR-1b | `loop` fires every `Interval` for ≥1000 ticks | R-TMR-1 |
| T-TMR-1c | `count(K)` fires exactly K times then frees the slot | R-TMR-1 |
| T-TMR-3a | Two timers waking on the same tick fire in `Seq` order regardless of wheel impl | R-TMR-3 |
| T-TMR-3b | `nextSeq` persists; a timer created after load gets a non-colliding `Seq` | R-TMR-3 |
| T-TMR-4a | `1ms` duration quantizes up to 1 tick; `49ms`→1 tick; `51ms`→2 ticks | R-TMR-4 |
| T-TMR-5a | Creating `Caps.Timers+1` timers: the last returns invalid ID, `timerDropped==1`, **0 allocs** | R-TMR-5 |
| T-TMR-6a | An owned loop timer stops firing the tick after its owner dies | R-TMR-6 |
| T-TMR-6b | Cancelling, then the owner dying, does not double-free (idempotent) | R-TMR-6 |
| T-TMR-8a | A pending Go-closure `After` timer is dropped on load and logged; sim hash matches a run that never created it | R-TMR-8 |

## 2. Determinism / replay

| ID | Test | Asserts |
|----|------|---------|
| T-TMR-DET-1 | 10k-tick scripted run with heavy timer churn; final `HashState` is byte-identical across two runs and across `-race` | R-SIM-1 |
| T-TMR-DET-2 | `FirstDivergence` localizes an intentionally corrupted timer column to system `"timers"` | R-SIM-6 |
| T-TMR-DET-3 | Cross-platform fixture (recorded hash) matches on linux/amd64 and the CI reference machine | R-SIM-1 |

## 3. Save / load round-trip

| ID | Test | Asserts |
|----|------|---------|
| T-TMR-SAV-1 | Save mid-match with live single/loop/count timers (including one mid-`count` with `Remaining>0`); load; continue; hash tracks a no-save control run tick-for-tick | R-TMR-2, R-TMR-7 |
| T-TMR-SAV-2 | **Mirror invariant**: bytes written by the save serializer equal bytes fed to the hasher (sans free-list-only region) | R-SIM-6 |
| T-TMR-SAV-3 | Free-list LIFO order survives round-trip; the next `CreateTimer` after load reuses the identical slot as the no-save control | R-TMR-7 |
| T-TMR-SAV-4 | Continuation re-bind: every `Cont` referenced by shipped ability templates is registered; an unregistered `Cont` in a save is dropped deterministically | R-TMR-2 |

## 4. Zero-alloc gate

| ID | Test | Asserts |
|----|------|---------|
| T-TMR-GC-1 | `testing.AllocsPerRun(1000, …)` over create→fire→reschedule→cancel steady state reports **0 allocs/op** | R-GC-1 |
| T-TMR-GC-2 | No store column is ever `append`-grown after `NewWorld` (asserted via cap==len invariant probe) | R-GC-2 |

## 5. Integration / FSV

| ID | Scenario | Source-of-truth check |
|----|----------|------------------------|
| F-TMR-1 | Spawner camp: clear it, wait 10 s, verify respawn | Parse the sim state JSON: unit count returns to baseline at tick `clearTick+200`; screenshot confirms units rendered |
| F-TMR-2 | Telegraphed detonation: cast, observe 3 s warning then damage | Event log shows `warn` projectile at T, `explosion` + `EvUnitDamaged` at T+60 |
| F-TMR-3 | Save during the 3 s telegraph, load, confirm detonation still occurs on schedule | Post-load event log shows detonation at the original absolute tick |

## 6. Preflight wiring

- Add the timer determinism fixture to `scripts/preflight.sh` (FULL gate, 10k determinism
  step). Decide fast vs full per the CLAUDE.md split: the **core single/loop/count
  determinism subtest runs in `--fast`**; the heavy churn + save/load matrix is full-only.
- Add `T-TMR-GC-1` to the zero-alloc gate set.
- Tag the heavy churn e2e with `if testing.Short() { t.Skip(...) }`.

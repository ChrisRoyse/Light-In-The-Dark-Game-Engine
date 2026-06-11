# Public API — Execution Model

> Expands [PRD §4.4 (Execution-model semantics)](../../PRD.md#44-execution-model-semantics-from-the-jass-runtime),
> rules R-EXEC-1…5. The [PRD](../../PRD.md) is the source of truth; this document elaborates, it
> does not override.

| | |
|---|---|
| **Status** | Draft v1.0 (expanded from PRD Draft v1.0) |
| **Date** | 2026-06-11 |
| **Owner** | Paul Ascenzi (Light in the Dark Analytics) |
| **Siblings** | [Architecture](architecture.md) · [Deduplication policy](deduplication-policy.md) · [Public API design](public-api-design.md) · [Naming & style](naming-and-style.md) |

---

## 1. The inherited semantics

JASS "threads" are cooperative coroutines scheduled by the game loop: a script runs until it
yields at `TriggerSleepAction`/`PolledWait` (or trips the opcode limit), and exactly one thread
runs at a time — which is why WC3 maps share globals freely without locks. LitD keeps this model
because it is *the correct model* for a deterministic sim: scripts interleave at known points
only, in a known order, inside the tick. The danger in Go is that goroutines invite exactly the
free-running concurrency JASS never had. R-EXEC-1…5 exist to close that door.

## 2. The deterministic cooperative scheduler (R-EXEC-1)

### 2.1 Requirements

Script logic (event handlers, timer callbacks, helper waits) runs **inside the sim tick**, on a
scheduler with these guarantees:

- **S-1 (exclusivity):** at most one script job executes at any instant; the sim systems and the
  scheduler never run simultaneously. Shared access to game state therefore needs no locks and
  has no races by construction.
- **S-2 (deterministic order):** the set of jobs runnable on a tick is executed in a totally
  ordered, reproducible sequence (§2.4). No ordering input may come from the Go runtime
  (goroutine wakeup order, channel select fairness, map iteration).
- **S-3 (bounded suspension points):** a job can suspend only by calling a blocking API verb
  (`helpers.PolledWait`, `Timer` waits, future `Sleep`-class helpers). There is no preemption and
  no opcode limit in v1; a non-yielding handler stalls the tick and is a script bug, surfaced by
  the debug-mode watchdog (wall-clock-based, diagnostic only — it never alters sim behavior).
- **S-4 (tick residency):** all resumption happens at defined phases of the tick
  ([PRD R-SIM-1](../../PRD.md#51-simulation-core-deterministic)); nothing script-visible occurs
  between ticks or on the render thread.

### 2.2 Primary design: goroutines as coroutines with strict baton-passing

Go has no stackful coroutine primitive, but a goroutine pair with unbuffered handoff channels is
one. Each script job that needs suspension capability owns a parked goroutine; the scheduler and
the job pass a single conceptual **baton**:

```go
// inside litd/sim/sched — illustrative, not final
type job struct {
    id      jobID         // dispatch-ordered identity (§2.4)
    resume  chan struct{} // scheduler → job: you have the baton
    yielded chan yieldKind// job → scheduler: baton back (done | waiting until tick T)
}

func (s *sched) runJob(j *job) {
    j.resume <- struct{}{} // hand baton to the job's goroutine
    k := <-j.yielded       // block until it yields or completes
    s.account(j, k)        // requeue at wake tick, or release to pool
}
```

Properties that make this deterministic despite using goroutines:

- The scheduler goroutine **blocks** on `<-j.yielded` for the entire time the job runs: the Go
  scheduler has exactly one runnable goroutine in the sim domain at any moment, so OS-level
  scheduling freedom cannot reorder anything observable (S-1, S-2).
- A wait verb is implemented as: register wake tick → `j.yielded <- waiting` → block on
  `<-j.resume`. The job's stack — ordinary Go stack, local variables intact — is preserved for
  free; this is the whole reason to prefer goroutines over hand-rolled state machines for D4
  helpers like `PolledWait`.
- Job goroutines are pooled (created at map load, reused across firings) to honor the zero-alloc
  tick rule ([PRD R-GC-1/2](../../PRD.md#531-go-garbage-collection-discipline)).
- Job goroutines never touch `time`, never spawn goroutines (no API exists for them to reach),
  and cannot run unless handed the baton.

### 2.3 Alternative considered: stackless continuation jobs

The fallback design, kept open until the M1 determinism spike validates baton-passing overhead:
handlers that never wait (the overwhelming majority) run as **plain synchronous calls** with no
goroutine at all; only a call to a wait verb promotes the job to a continuation — either a
goroutine on demand, or (fully stackless) an explicit
`Wait(d, func(){ ...rest... })` continuation-passing helper style. The stackless variant costs
ergonomics (no mid-function suspension; the modder writes the continuation) but removes all
goroutine machinery from the hot path. **Decision rule:** the hybrid is the default plan —
synchronous fast path always, goroutine promotion only on first wait — and pure-stackless is
adopted only if M1/M3 benchmarks show baton-passing breaking the 10 ms tick budget. Either way
the public API is unchanged: `helpers.PolledWait` is specified to suspend the calling job, and
how is an implementation detail.

### 2.4 Deterministic resume order

The runnable queue on each tick is ordered by the tuple:

```
(wakeTick, phase, registrationSeq, firingSeq)
```

- `wakeTick` — the tick a waiting job becomes runnable (quantized, §3).
- `phase` — fixed tick phases: ① timer expirations, ② command-stream application, ③ sim systems
  (movement, combat, …) which emit events, ④ event dispatch, ⑤ resumption of woken waiting jobs.
- `registrationSeq` — a monotonic counter stamped at `OnEvent`/`After` registration time
  (R-EXEC-2's "deterministic registration order").
- `firingSeq` — for multiple firings of the same handler in one tick (e.g. two units die to one
  spell), the order events were emitted by sim systems, which is itself deterministic because
  systems iterate ordered ECS storage, never Go maps
  ([PRD R-SIM-2](../../PRD.md#51-simulation-core-deterministic)).

Ties are impossible by construction (the tuple is unique per job activation). The same total
order is used in headless and rendered builds — the state-hash equivalence test
([Architecture §4](architecture.md#4-headless-mode)) would catch a divergence.

## 3. Waits and quantization (R-EXEC-5)

All public durations — `g.After(d, fn)`, `g.Every(d, fn)`, `helpers.PolledWait(d)`,
event-time accessors — quantize to **sim ticks: 50 ms at 20 Hz**, with these rules:

- Quantization is **ceiling-based with a one-tick minimum**: `PolledWait(1 * time.Millisecond)`
  waits one full tick (50 ms); `PolledWait(0)` and negative durations return immediately without
  suspending (matching JASS's `if (duration > 0)` guard in `PolledWait`).
- `time.Duration` is accepted for ergonomics, but there is **no sub-tick timing anywhere in the
  public API**: no frame callbacks, no render-time clocks, no wall-clock access. A script cannot
  observe render rate, which is what keeps a 30 FPS low-tier machine and a 144 Hz desktop in
  lockstep ([PRD G5](../../PRD.md#21-goals)).
- A periodic `g.Every(d, fn)` fires at exact tick multiples with no drift accumulation (next fire
  is computed from the schedule origin, not from the previous fire's completion).
- The JASS distinction between real-time `TriggerSleepAction` and game-time `PolledWait`
  disappears: LitD has only game time. The D4 helper keeps the `PolledWait` name as the
  migration-friendly spelling ([Naming & style §3](naming-and-style.md#3-the-jassgo-mapping-table));
  its polling implementation is gone — it is a direct scheduler suspension.

## 4. Filter purity (R-EXEC-2, second half)

JASS `boolexpr` conditions had to be wait-free; map crashes from waiting conditions were a
classic bug class. LitD enforces purity **by API shape rather than by policing**:

```go
// Filters receive a read-only view, return bool. That is the entire contract.
g.OnEvent(litd.EventUnitAttacked, handler,
    litd.Where(func(v litd.EventView) bool {
        return v.Unit().Owner() == p && v.Unit().LifePercent() < 50
    }))

units := g.UnitsIn(rect, func(u litd.UnitView) bool { return u.Dead() == false })
```

- Filter parameters are **view types** (`EventView`, `UnitView`): read-only projections that
  expose getters only. No mutating verb, no `Game` access, no wait verb is reachable from a view —
  the compiler enforces what the JASS manual could only document.
- Filters run inline (no job, no baton) at dispatch/query time, in deterministic candidate order
  (ordered ECS storage).
- Residual impurity (a filter mutating captured script-local variables) is permitted — it cannot
  desync the sim, only confuse its author — but debug mode runs filters twice on a sampled basis
  and warns if the two results differ, catching nondeterministic filters cheaply.

## 5. Collections: callback-enum → slices (R-EXEC-4)

The JASS pattern `GroupEnumUnitsInRect(g, r, filter)` + `ForGroup(g, callback)` +
`GetEnumUnit()` smuggles a hidden thread-local "current element" through global state. The
canonical replacement returns plain data:

```go
for _, u := range g.UnitsIn(rect, filter) {   // ordered deterministically
    u.Order(litd.OrderStop)
}
```

- Result slices are ordered by entity index (deterministic) and are **snapshots**: mutation
  during iteration (killing units in the loop) cannot invalidate the slice, mirroring the safe
  half of WC3's enum semantics without the unsafe half.
- For zero-alloc hot paths, each query verb has an appending variant
  (`g.AppendUnitsIn(dst []Unit, rect, filter) []Unit`) so pooled buffers can be reused
  ([PRD R-GC-2](../../PRD.md#531-go-garbage-collection-discipline)).
- `FirstOfGroup` loops, `GetEnumUnit`, and the `Counted` enum twins are all tombstoned or
  D3-collapsed accordingly
  ([Deduplication policy §4, §7](deduplication-policy.md#7-tombstone-policy)).

## 6. AI-domain isolation (R-EXEC-3)

WC3 ran AI scripts in separate contexts: max 6 threads per player, **no shared globals** with the
map script, communication only via integer-pair command stacks, and several natives broken inside
the AI context. The lesson LitD takes: AI is a *foreign domain* with a message-passing boundary,
not privileged script.

- v1 ships no computer-player AI ([PRD §9.4](../../PRD.md#9-open-questions)); every `commonai`
  native is tombstoned `v2` in the manifest with this section as the design anchor.
- The v2 design sketch: each AI player gets its **own scheduler instance** (same deterministic
  scheduler type as §2, separate job space), running in a dedicated phase of the tick. It sees
  the sim only through the same read-only view types as filters (§4), and it acts only by
  enqueuing **typed commands** onto the same ordered command stream that player input and
  replays use ([Architecture §2](architecture.md#2-import-rules)) — the typed-Go-channel
  descendant of WC3's integer-pair command stack.
- Consequences: AI cannot desync a match (its commands are in the deterministic stream like
  everyone else's), AI can be disabled or replaced wholesale (it has no hooks into map script
  state), and a future external-process AI is the same interface over a pipe.

## 7. What the modder must know (the short version)

The entire contract, as it will appear at the top of the `litd` package godoc:

1. Your handlers run one at a time, inside the simulation tick, in the order you registered them.
   You never need a mutex.
2. If you wait (`helpers.PolledWait`, `g.After`), you resume on a later tick; everything may have
   changed — re-check your handles (`Valid()`,
   [R-API-5](public-api-design.md#35-r-api-5--error-semantics-and-zero-value-handles)).
3. All time is game time in 50 ms steps. There is no frame time.
4. Filters can look but not touch — the types they receive make sure of it.
5. Don't loop forever without waiting; in debug mode the engine will tell you where.

## 8. Acceptance criteria for this section

- M1 spike: scheduler prototype (baton-passing and synchronous-fast-path hybrid) passes the
  10k-tick state-hash reproducibility test, including jobs suspended across ticks, under
  `GOMAXPROCS=1` and `GOMAXPROCS=N` with identical hashes.
- `go test -race` clean across the scheduler with concurrent render-snapshot reads.
- Benchmarks: handler dispatch (no wait) allocation-free; baton handoff ≤ the M1-set budget per
  suspension; 500 simultaneously waiting jobs resumable within tick budget
  ([PRD §5.3](../../PRD.md#53-performance-budgets-acceptance-gates-low-tier-reference-machine-dual-core-2-ghz-intel-uhd-620-4-gb-ram)).
- Event-order conformance tests: registration-order dispatch, emit-order firing, wake-tuple
  resume order — each pinned by golden tests that fail on any reordering.

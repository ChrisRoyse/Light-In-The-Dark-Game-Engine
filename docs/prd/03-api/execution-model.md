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
- **S-5 (serializability, D-2026-06-11-9):** the scheduler's complete suspension state — every
  suspended coroutine/job, pending timer, and event subscription — serializes into the mid-game
  save format and reconstructs on load with identical resume order (§2.4 tuples included).
  Full save/load is **v1 scope**; serializability is a day-one design constraint on the M1
  scheduler representation, not a retrofit. *Revised 2026-06-11 per D-2026-06-11-9.*

### 2.2 Rejected design: goroutines as coroutines with strict baton-passing (historical)

*Revised 2026-06-11 per D-2026-06-11-9 — formerly "primary design"; serializability (S-5)
demoted it to a candidate. Revised again 2026-06-11 per D-2026-06-11-28: the candidate is
**dead** — the executed scheduler spike validated the stackless design (§2.3), and this
section is retained only as the rationale record for why baton-passing lost.*

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

### 2.3 Stackless / descriptive-suspension design — DECIDED and spike-validated

*Revised 2026-06-11 per D-2026-06-11-9. Revised again 2026-06-11 per D-2026-06-11-28: the
"decision rule" below is discharged — the spike ran and the stackless design is **the**
scheduler design, not a default plan.*

The competing designs: handlers that never wait (the overwhelming majority) run as **plain
synchronous calls** with no goroutine at all; only a call to a wait verb promotes the job to a
continuation — either a goroutine on demand, or (fully stackless) an explicit
`Wait(d, func(){ ...rest... })` continuation-passing helper style. The stackless variant costs
ergonomics (no mid-function suspension; the modder writes the continuation) but removes all
goroutine machinery from the hot path.

**S-5 changes the weighting.** A parked goroutine's stack is opaque to us — it cannot be
written into a save file. Mid-game saves (full v1 scope, D-2026-06-11-9) therefore require
that every suspension be representable as a **descriptive suspension record** (what is waited
on, wake tick, the continuation to run) regardless of how the suspension is executed at
runtime. That materially favors the stackless/descriptive design: it gets serialization for
free, where baton-passing would have to carry an equivalent record alongside the live stack
and prove the two never disagree.

**Decision — spike-validated (D-2026-06-11-28):** the synchronous fast path is
unconditional; for suspension, the stackless/descriptive-suspension representation is
**the design**. The M1-scope spike (`spikes/scheduler`) implemented scripts as descriptive
suspension records (PC + locals + typed suspension) with a sleep queue keyed
`(wakeTick, seq)` and event waiters FIFO by seq, then gob-serialized the full scheduler
state **mid-run**, restored it, and advanced — traces and state bit-identical with the
uninterrupted run, resume order deterministic. The baton-passing design is eliminated
(§2.2 is the historical record); remaining M1 work is production hardening, not design
choice. The public API is unchanged either way: `helpers.PolledWait` is specified to suspend
the calling job, and how is an implementation detail.

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

*Revised 2026-06-11 per D-2026-06-11-6 (supersedes the v2 deferral).*

- The AI domain is a **full v1 port**, shipped as its own milestone **M5.5** (after the core
  API at M5, before the M6 vertical slice). All ~123 `common.ai` natives plus the AI-related
  `common.j` natives map canonically ([ai-natives](jass-mapping/ai-natives.md)); none is
  tombstoned for capability reasons, and M6's melee opponent runs on this domain, not a Go
  stopgap.
- The M5.5 design: each AI player gets its **own scheduler instance** (same deterministic
  scheduler type as §2, separate job space, S-1…S-5 all apply — AI suspensions serialize into
  saves like any other), running in a dedicated phase of the tick. It sees the sim only
  through the same read-only view types as filters (§4), and it acts only by enqueuing
  **typed commands** onto the same ordered command stream that player input and replays use
  ([Architecture §2](architecture.md#2-import-rules)) — the typed-Go-channel descendant of
  WC3's integer-pair command stack.
- Consequences: AI cannot desync a match (its commands are in the deterministic stream like
  everyone else's), AI can be disabled or replaced wholesale (it has no hooks into map script
  state), and a future external-process AI is the same interface over a pipe.

## 7. The Lua execution surface (D-2026-06-11-8, R-SEC-1)

*Added 2026-06-11 per D-2026-06-11-8/20.*

v1 (M5) ships an embedded deterministic Lua VM — concretely a **vendored fork of
`yuin/gopher-lua`** (D-2026-06-11-25, *revised 2026-06-11*: four LITD patches —
instruction-budget hook in `mainLoop`, deterministic mathlib replacement, coroutine/LState
persister, LState pooling + golden cross-arch CI test; see
[Determinism §2.6](../04-simulation/determinism.md)) — as the runtime-loadable creation
surface ([PRD §5.6](../../PRD.md#56-moddingscripting),
[Architecture §6](architecture.md#6-the-lua-binding-layer-v1-m5)). Its execution semantics
are **this document, unchanged**:

- **Same scheduler.** A Lua coroutine is a scheduler job: it suspends only at the same wait
  verbs, resumes by the same `(wakeTick, phase, registrationSeq, firingSeq)` tuple (§2.4),
  and quantizes durations to ticks (§3). A mixed Go/Lua world has one total order, not two.
  Lua coroutine suspensions are descriptive by nature (the VM owns the coroutine state), so
  they serialize into saves under S-5 like every other job.
- **Same determinism rules.** No wall clock, no `os`/`io`/network, `math.random` replaced by
  the sim PRNG, no Go-runtime ordering observable. The bindings are generated from
  `api-manifest.json`, so the Lua surface cannot reach anything the Go surface cannot.
- **Hard quotas (R-SEC-1).** Each Lua context carries **per-tick instruction and memory
  quotas**, enforced by the VM. Exceeding a quota is a loud script error
  (R-FSV-4 — never a silent slowdown), and the world is told exactly where. This is the one
  deliberate divergence from S-3's "no opcode limit" stance: Go handlers are first-party code
  and get the diagnostic watchdog only; Lua worlds are untrusted third-party content
  (D-2026-06-11-20) and get hard limits. The quotas double as the **lockstep stall guard**
  for M7 multiplayer — a world script cannot stall the tick past budget on one client and
  desync or hang the session.
- **Same isolation philosophy as the AI domain (§6):** a sandboxed foreign domain whose only
  authority is the game API it was handed.

## 8. What the modder must know (the short version)

The entire contract, as it will appear at the top of the `litd` package godoc:

1. Your handlers run one at a time, inside the simulation tick, in the order you registered them.
   You never need a mutex.
2. If you wait (`helpers.PolledWait`, `g.After`), you resume on a later tick; everything may have
   changed — re-check your handles (`Valid()`,
   [R-API-5](public-api-design.md#35-r-api-5--error-semantics-and-zero-value-handles)).
3. All time is game time in 50 ms steps. There is no frame time.
4. Filters can look but not touch — the types they receive make sure of it.
5. Don't loop forever without waiting; in debug mode the engine will tell you where.

## 9. Acceptance criteria for this section

- M1 scheduler: the §2.3 design is already spike-validated (D-2026-06-11-28,
  `spikes/scheduler` — mid-run save/restore bit-identical); the production implementation
  (synchronous fast path + suspension records) passes the 10k-tick state-hash
  reproducibility test, including jobs suspended across ticks, under `GOMAXPROCS=1` and
  `GOMAXPROCS=N` with identical hashes. *(Revised 2026-06-11 per D-2026-06-11-28.)*
- **Serializability (S-5, D-2026-06-11-9):** save → load → resume round-trip with jobs
  suspended mid-wait, pending timers, and live event subscriptions produces a state hash
  identical to the uninterrupted run; golden test from M3.
- **Lua quotas (R-SEC-1):** instruction- and memory-quota breach tests fail loudly with
  location; sandbox-escape test suite (no `os`/`io`/net/FFI reachable) green from M5.
- `go test -race` clean across the scheduler with concurrent render-snapshot reads.
- Benchmarks: handler dispatch (no wait) allocation-free; suspension-record push/resume ≤ the
  M1-set budget per suspension; 500 simultaneously waiting jobs resumable within tick budget
  ([PRD §5.3](../../PRD.md#53-performance-budgets-acceptance-gates-low-tier-reference-machine-dual-core-2-ghz-intel-uhd-620-4-gb-ram)).
- Event-order conformance tests: registration-order dispatch, emit-order firing, wake-tuple
  resume order — each pinned by golden tests that fail on any reordering.

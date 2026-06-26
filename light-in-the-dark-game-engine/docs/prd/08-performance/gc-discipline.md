# Performance — Go Garbage-Collection Discipline

> Expands [PRD §5.3.1 (Go garbage-collection discipline)](../../PRD.md#531-go-garbage-collection-discipline), requirements **R-GC-1 … R-GC-5**.
> The [PRD](../../PRD.md) is the source of truth; this document elaborates, it does not override.

| | |
|---|---|
| **Status** | Draft v1.0 (expanded from PRD Draft v1.0) |
| **Date** | 2026-06-11 |
| **Owner** | Paul Ascenzi (Light in the Dark Analytics) |
| **Parent requirements** | R-GC-1 (zero allocs/tick and /frame), R-GC-2 (pools), R-GC-3 (value types in hot paths), R-GC-4 (tuning is fallback only), R-GC-5 (CI regression rejection) |

---

## 1. Why zero, not "low"

Go's GC is concurrent and has sub-millisecond pauses, but on a **dual-core 2 GHz** reference
machine ([Budgets and Benchmarks §5](./budgets-and-benchmarks.md)) the relevant cost is not
the pause — it is that the concurrent marker and the mutator share two cores. A GC cycle
running during a 500-unit battle steals a meaningful fraction of exactly the CPU the 10 ms
tick budget and the 16.6 ms frame budget need. The only allocation rate that makes GC
frequency *zero at steady state* is an allocation rate of zero; any "small" per-tick
allocation, multiplied by 20 Hz × match length, schedules GC cycles forever.

Hence R-GC-1's absolute formulation: **zero heap allocations per sim tick and per render
frame at steady state**, with explicitly carved-out non-steady-state moments — map load,
match start unit bursts, window resize ([UI and HUD §5](../07-platform/ui-and-hud.md)),
menu transitions — where allocation is normal and expected. "Steady state" is defined
operationally: the measurement windows of the benchmark scenarios in
[Budgets and Benchmarks §3–4](./budgets-and-benchmarks.md), which begin after warm-up
ticks/frames.

The discipline applies to the **sim tick path** (`litd/sim`), the **render frame path**
(`litd/render`, including HUD update and audio dispatch —
[Audio §5](../07-platform/audio.md)), and the **input encode path**
([Input §8](../07-platform/input.md)). It does *not* apply to tooling, loaders, or the menu.

## 2. Allocation hazards catalog (R-GC-3)

The recurring Go constructs that silently allocate, with the project-standard remedy. Code
review uses this section as a checklist for any PR touching a hot path.

| # | Hazard | Why it allocates | Remedy |
|---|---|---|---|
| H1 | **Interface boxing** — storing a concrete value in an `interface{}`/`any` or a small interface (`fmt.Println(x)`, `sort.Sort(byX(s))`, `errors.Wrap`, heterogeneous event payloads) | Non-pointer values convert to interfaces by heap-allocating a copy | No interfaces in hot-path signatures (R-GC-3). Events are a closed set of concrete value-struct types in a tagged union (§4.3); sorting uses `slices.SortFunc` with concrete types or hand-rolled insertion sort on SoA indices; `fmt` is banned in hot paths outright |
| H2 | **Closure captures** — `func() { use(u) }` where the closure outlives the frame (handed to a scheduler, timer, or event queue) | Captured variables escape to the heap; the closure header allocates | Hot-path callbacks are pre-bound at registration time (allowed to allocate, happens once); per-tick deferred work is encoded as *data* (a pooled job struct with an opcode and value fields), not as a fresh closure. `OnEvent` handlers register once at setup (R-EXEC-2), never per tick |
| H3 | **String building** — `"unit " + name + " died"`, `fmt.Sprintf`, `strconv.Itoa` per frame for HUD labels | Every concatenation/format allocates a new string | Logging/string building is debug-mode only (R-GC-3). HUD labels rewrite into preallocated `[]byte` with `strconv.AppendInt`/`AppendFloat` and only on value change ([UI and HUD §3](../07-platform/ui-and-hud.md)) |
| H4 | **Slice growth** — `append` past capacity in tick code (`targets = append(targets, id)`) | Reallocation + copy; worse, the old backing array becomes garbage | All hot-path slices are preallocated at map load to fixed capacity (R-GC-2: ECS stores never reallocate mid-match). `append` into them is fine *up to capacity*; exceeding capacity is a checked, deterministic overflow (drop + event), never a grow. Scratch slices come from per-system scratch buffers reset (`s = s[:0]`) each tick |
| H5 | **Map churn** — inserting/deleting in `map[K]V` per tick; also `range` over maps in gameplay code | Bucket growth allocates; iteration order is random — a *determinism* violation (R-SIM-2) before it is a performance one | No Go maps in gameplay state at all. Lookups are dense arrays indexed by entity ID, or sorted keyed slices. Maps are permitted in load-time code and presentation-side caches that never affect sim state |
| H6 | **`time.After` / `time.NewTimer` in loops** — `select { case <-time.After(d): }` per iteration | Allocates a timer (and pre-1.23 leaked it until firing); also injects wall-clock into logic | Wall-clock timing is banned inside the sim entirely (R-SIM-2); waits are tick-quantized counters on the cooperative scheduler (R-EXEC-5). Render-side periodic work derives from frame timestamps already collected once per frame |
| H7 | **Boxed method values & function fields** — `u.TakeDamage` as a value, `var f func(x int) = obj.m` | Method-value creation allocates a closure | Pass `(receiver, funcID)` pairs or call through concrete types; behavior tables are arrays of plain `func` symbols indexed by data-table ID (free functions don't allocate) |
| H8 | **`time.Time`/`error` formatting, `panic`/`recover` paths, `defer` accumulating in long loops** | Misc heap traffic; `recover` paths box | Hot paths are panic-free by R-API-5; `defer` only at tick/frame top level, never inside per-entity loops |
| H9 | **cgo crossings hiding allocations** — per-draw G3N/GL calls building parameter slices or strings | Library-internal allocation we don't control | Render-path G3N usage is audited with the same `AllocsPerRun` gates; offending engine paths are patched in the vendored fork (`repoes/engine`), consistent with [PRD §3.4](../../PRD.md#34-engine-viability-and-risks-g3n) |

## 3. Verification-first culture

A hazard checklist is advisory; the gates in §5–§6 are the actual enforcement. The rule for
contributors: **never reason your way to "this doesn't allocate" — measure.** Escape
analysis is subtle (a one-line change can flip a variable from stack to heap), so every
hot-path function carries a benchmark gate, and every optimization claim in review links a
gate or a `-gcflags=-m` excerpt.

## 4. Pooling patterns (R-GC-2)

R-GC-2 requires all transient gameplay objects to come from preallocated pools. Three
patterns cover every case in the engine; new systems must justify deviating.

### 4.1 SoA free-list pool — missiles, buffs, order-queue entries

Transients live directly inside the ECS struct-of-arrays stores (R-SIM-3): a fixed-capacity
parallel-array block plus a free list of indices and a generation counter per slot
(generation-counted IDs are the same scheme the command stream uses for entity references,
[Input §8](../07-platform/input.md)).

```go
type MissilePool struct {
    pos, vel   []Fixed2     // parallel SoA arrays, len == cap, fixed at map load
    target     []EntityID
    gen        []uint16     // slot generation; stale handles never resurrect
    free       []int32      // free-list stack, preallocated
    liveCount  int32
}

func (p *MissilePool) Spawn() (Handle, bool) { // bool: pool exhausted
    if len(p.free) == 0 { return Handle{}, false } // deterministic, budgeted failure
    i := p.free[len(p.free)-1]
    p.free = p.free[:len(p.free)-1]
    return Handle{Index: i, Gen: p.gen[i]}, true
}
```

Pool exhaustion is a *designed* state: capacities are sized from the §5.3 worst case (500
units + 500 missiles) with headroom, and overflow behavior is deterministic (oldest
missile recycled, or spawn refused with an event) — never a grow, never a panic.

### 4.2 Value-type ring buffer — events and per-tick scratch

Events (R-API-4) are the textbook interface-boxing trap (H1). The engine's event queue is a
preallocated ring of one concrete struct:

```go
type Event struct {            // one flat value type, no pointers, no interfaces
    Kind    EventKind          // tag
    Tick    uint32
    Subject EntityID           // primary entity (dying unit, casting unit, …)
    Object  EntityID           // secondary (killer, target, …)
    Arg     Fixed              // kind-specific scalar (damage amount, …)
}

type EventQueue struct {
    buf  []Event   // fixed capacity, allocated at map load
    head, tail int
}
```

Handlers receive `*Event` (pointer into the ring, valid for the synchronous dispatch only —
enforced by convention and a debug-mode poisoning pass). Public-API event objects
(`litd.Event`) are thin value views over this record, satisfying R-GC-3's "value types for
event payloads". The same ring pattern backs the per-tick command queue
([Input §8](../07-platform/input.md)) and per-system scratch buffers.

### 4.3 Source/voice pools — presentation side

The render and audio layers pool their transients identically: OpenAL sources are created
once and recycled by the admission controller ([Audio §5](../07-platform/audio.md));
HUD label byte-buffers, selection-circle instances, and health-bar quads are fixed pools
sized to the selection cap and on-screen entity ceiling.

## 5. CI enforcement with `testing.AllocsPerRun` (R-GC-1, R-GC-5)

Every hot path has an allocation gate in `bench/allocgates/`, run on every PR from M3
([Budgets and Benchmarks §6](./budgets-and-benchmarks.md)). The pattern:

```go
func TestTickAllocs(t *testing.T) {
    w := loadBenchWorld(t, "bench_battle_500v500") // setup may allocate freely
    for i := 0; i < 100; i++ { w.Step() }          // warm-up: exit non-steady state

    allocs := testing.AllocsPerRun(200, func() {
        w.Step()                                    // one full 20 Hz tick
    })
    if allocs != 0 {
        t.Fatalf("sim tick allocated %.1f objects/tick; R-GC-1 requires 0", allocs)
    }
}
```

Gate inventory (each is one focused test like the above):

| Gate | Path covered |
|---|---|
| `TestTickAllocs` | Full `sim.Step()` on each [benchmark scenario](./budgets-and-benchmarks.md) |
| `TestEventDispatchAllocs` | Event publish + N registered handlers |
| `TestCommandDecodeAllocs` | Command-record decode/validate/enqueue ([Input §8](../07-platform/input.md)) |
| `TestPathRequestAllocs` | Path query + result writeback (R-SIM-5) |
| `TestFrameUpdateAllocs` | Render-side per-frame CPU work, GL calls stubbed: interpolation, HUD diff/update, audio admission ([UI and HUD §6](../07-platform/ui-and-hud.md), [Audio §5](../07-platform/audio.md)) |
| `TestAPIHotcallAllocs` | Representative public-API hot calls (`Unit.Position`, `Unit.Order`, `g.UnitsIn`) — the API layer must not undo the sim's discipline |

Rules:

- The threshold is **exactly 0** and is not configurable (R-GC-5: any regression above the
  zero baseline is rejected). There is no "allowed allocations" list; a path that cannot
  reach zero gets redesigned, not whitelisted.
- `AllocsPerRun` measures the calling goroutine; hot paths are single-goroutine by design
  (R-EXEC-1), so this is sufficient. A secondary match-length gate asserts
  `runtime.MemStats.Mallocs` delta ≈ 0 across a 10k-tick headless run to catch anything
  (background goroutines, finalizers) the per-call gates miss.
- Gates run with `-run` filters in short mode on every PR (< 1 minute total) and over the
  full scenario matrix nightly.

## 6. Escape-analysis verification workflow

`AllocsPerRun` tells you *that* a path allocates; escape analysis tells you *why*. The
standard workflow when a gate fails (also step 3 of the profiling workflow in
[Budgets and Benchmarks §7](./budgets-and-benchmarks.md)):

1. **Locate:** `go test -bench=Tick -memprofile mem.out`, then
   `go tool pprof -sample_index=alloc_objects mem.out` — identifies the allocating line.
2. **Explain:** rebuild the package with the escape-analysis report:

   ```sh
   go build -gcflags='-m -m' ./litd/sim/... 2>&1 | grep -E 'escapes to heap|moved to heap'
   ```

   The doubled `-m -m` prints the *chain* of reasons ("parameter u leaks to {heap} because
   …"), which is what you need to fix the cause rather than the symptom. Typical verdicts
   map straight onto the §2 catalog: "escapes to heap: interface conversion" → H1; "func
   literal escapes" → H2; "… escapes to heap: too large for stack" → restructure or pool.
3. **Fix and re-gate:** the change is accepted when the `AllocsPerRun` gate is green again.
   The escape report itself is **not** gated in CI — its output format is compiler-version
   dependent and noisy; the allocation count is the stable contract. The report is a
   diagnostic tool, the gate is the law.
4. **Annotate:** functions that were non-obviously rescued from escaping get a short
   `// escape: …` comment citing the hazard (H1–H9) so the next editor doesn't regress it
   blindly; the gate still backstops them.

Inlining interacts with escape analysis (a non-inlined accessor can force its receiver to
escape), so hot-path accessors are kept under the inlining budget; suspected inlining
regressions are checked with `-gcflags='-m'`'s "cannot inline" lines during review of
compiler upgrades.

## 7. GC tuning fallback policy (R-GC-4)

R-GC-4: tuning is a fallback, not a strategy. Concretely:

- **All budget gates run with default GC settings** (`GOGC=100`, no memory limit). A
  benchmark that only passes with tuning is a failed benchmark; the fix is allocation
  work (§2–§6), not knobs.
- **Permitted startup settings in release builds**, applied once in `main`, never changed
  mid-match:
  - `debug.SetMemoryLimit(1.2 GiB)` — a soft limit comfortably under the 1.5 GB match
    budget ([Budgets and Benchmarks §2](./budgets-and-benchmarks.md)), so that if a leak or
    load spike occurs, the GC bounds RSS *before* a 4 GB machine starts swapping
    (swapping is the catastrophic failure mode on the reference machine; an extra GC cycle
    is merely bad).
  - `debug.SetGCPercent` may be *raised* (e.g. 200–400) to space out the rare cycles caused
    by legitimate non-steady-state allocation, trading idle heap headroom for fewer cycles —
    acceptable only because steady-state allocation is zero and the memory limit bounds the
    worst case.
- **Forbidden:** `GOGC=off` (unbounded heap on a 4 GB machine), per-frame `runtime.GC()`
  calls (a scheduling hack that hides allocation bugs), and any mid-match retuning.
  Explicitly *permitted and encouraged*: a single voluntary `runtime.GC()` at the end of
  map load, collecting load-time garbage once before the steady-state clock starts.
- Release builds log `GODEBUG=gctrace=1`-equivalent summaries (cycle count, total pause)
  into the perf JSON artifacts; the nightly run asserts **zero GC cycles** during the
  steady-state window of `bench_battle_500v500` — the end-to-end proof that R-GC-1 through
  R-GC-4 compose.

## 8. Acceptance criteria

1. Every gate in the §5 inventory exists, runs on every PR from M3, and gates at exactly
   zero (R-GC-1, R-GC-5).
2. The 10k-tick `Mallocs`-delta and zero-GC-cycle nightly assertions are green on
   `bench_battle_500v500` at steady state.
3. All §5.3 time budgets pass with default GC settings on the reference machine (R-GC-4);
   the release-build memory limit and GOGC values are documented constants with a linked
   justification.
4. ECS stores and pools demonstrably never reallocate mid-match: capacities asserted
   constant across the full benchmark suite (pointer-identity check at match start vs end).
5. Code review applies the §2 hazard catalog to every hot-path PR; H1–H9 references appear
   in review discussion and `// escape:` annotations where relevant.

## 9. Related documents

- [Budgets and Benchmarks](./budgets-and-benchmarks.md) — the CI harness, scenarios, and regression mechanics these gates run inside.
- [Input §8](../07-platform/input.md) — pooled command encoding; generation-counted IDs.
- [Audio §5](../07-platform/audio.md), [UI and HUD §3](../07-platform/ui-and-hud.md) — presentation-side pooling and zero-alloc label updates.
- [PRD §5.3.1](../../PRD.md#531-go-garbage-collection-discipline) — parent requirements R-GC-1…5; [PRD §5.1](../../PRD.md#51-simulation-core-deterministic) — determinism rules that share root causes with H5/H6.

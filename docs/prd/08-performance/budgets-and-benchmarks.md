# Performance — Budgets and Benchmarks

> Expands [PRD §5.3 (Performance budgets)](../../PRD.md#53-performance-budgets-acceptance-gates-low-tier-reference-machine-dual-core-2-ghz-intel-uhd-620-4-gb-ram).
> The [PRD](../../PRD.md) is the source of truth; this document elaborates, it does not override.

| | |
|---|---|
| **Status** | Draft v1.0 (expanded from PRD Draft v1.0) |
| **Date** | 2026-06-11 |
| **Owner** | Paul Ascenzi (Light in the Dark Analytics) |
| **Parent section** | PRD §5.3 budget table; CI enforcement from M3 |

---

## 1. Philosophy: budgets are acceptance gates

Per G3, the budgets in [PRD §5.3](../../PRD.md#53-performance-budgets-acceptance-gates-low-tier-reference-machine-dual-core-2-ghz-intel-uhd-620-4-gb-ram)
are **acceptance gates, not aspirations**: a milestone does not exit, and a PR does not
merge (from M3 onward), while a budget is red. Three rules govern how budgets are used:

1. **Every budget has exactly one measurement methodology** (§2). A number without a
   defined measurement procedure is not a budget; "60 FPS" means nothing until percentile,
   scene, and machine are fixed.
2. **The reference machine is the floor, not the average** (§5). Nothing is accepted on the
   strength of developer hardware.
3. **Two-tier enforcement.** CI machines are not reference machines, so CI enforces
   *relative regression gates* on every PR (§6), while *absolute* budgets are verified on
   the physical reference machine at every milestone exit and on a scheduled cadence.

The companion document [GC Discipline](./gc-discipline.md) covers the allocation budgets
(R-GC-1…5), which are enforced by the same harness; this document covers time, memory, and
size budgets.

## 2. Per-budget measurement methodology

| Budget (PRD §5.3) | Metric definition | Procedure |
|---|---|---|
| **Render ≥ 60 FPS typical** | Frame time **p50 ≤ 16.6 ms and p99 ≤ 33 ms** over the "typical" segment of the render benchmark scene (§4) | Per-frame CPU+present time recorded into a preallocated ring buffer (the recorder must itself satisfy R-GC-1); percentiles computed post-run. FPS averages are never used — they hide hitches |
| **Render ≥ 30 FPS worst case (500 units on screen)** | Frame time **p99 ≤ 33.3 ms** over the "max battle" segment of the scene; additionally no single frame > 100 ms (hitch gate) | Same recorder; the 500-unit segment holds all units inside the frustum, animating, with projectiles and the full HUD ([UI and HUD §6](../07-platform/ui-and-hud.md)) |
| **Sim tick ≤ 10 ms worst case** | **Max** tick wall time (not mean, not p99 — the budget already includes 50 % headroom against the 50 ms tick period) across the full headless benchmark battle (§3) | `go test -bench` harness around `sim.Step()`; per-tick durations recorded; gate on max and report p50/p99 for trend analysis |
| **Cold start ≤ 5 s** | Process exec → main menu first frame presented | Timestamp written at first present; measured on the reference machine from cold page cache (`echo 3 > /proc/sys/vm/drop_caches` on Linux; reboot script on Windows). CI runs it warm and gates relatively only |
| **Map load ≤ 10 s** | "Load" command accepted → first playable frame (sim tick 0 executed + first present) of the 128×128 benchmark map with full asset set, including decoded audio ([Audio §6](../07-platform/audio.md)) | Instrumented load phases (asset I/O, GLB parse, GPU upload, pathfinding-grid build, audio decode) each reported separately so regressions are attributable |
| **RAM ≤ 1.5 GB full match** | **Peak RSS** of the process over a full benchmark match, *and* Go heap (`runtime.ReadMemStats.HeapInuse`) reported alongside to separate Go heap from cgo/GL driver memory | Sampled every second by the harness; the gate is on peak RSS measured on the reference machine (driver memory varies by GPU, so CI tracks Go heap only) |
| **Binary + base assets ≤ 300 MB** | Sum of release binary + `assets/` shipped set, uncompressed on disk | Trivial CI step on every release build; reported per directory so growth is attributable |

Every measurement emits a single machine-readable JSON result file (metric, value, gate,
commit, machine ID) consumed by the gate checker and archived for trend dashboards.

## 3. The headless sim benchmark (CI workhorse)

The sim is headless by requirement (R-SIM-4), which makes it perfectly benchmarkable in
ordinary CI containers — no GPU, no display, no flakiness from rendering.

**Workload definition.** A benchmark is `(map, PRNG seed, recorded command stream)` — the
exact replay format defined in [Input §8](../07-platform/input.md). This has two crucial
properties: the workload is *bit-reproducible* (R-SIM-2 means every run simulates identical
states, so timing variance is environmental only), and benchmarks double as determinism
tests (the state hash is asserted at the end of every benchmark run; a wrong hash fails the
build before any timing is considered).

**Canonical scenarios** (checked into `bench/scenarios/`):

| Scenario | Contents | Gates exercised |
|---|---|---|
| `bench_idle_500` | 500 idle units, no orders, 2k ticks | Baseline tick cost; allocation gate floor |
| `bench_battle_500v500` | 500 vs 500 scripted attack-move into sustained combat with projectiles, deaths, buffs, 10k ticks | The §5.3 worst-case tick budget; pooling under churn ([GC Discipline §4](./gc-discipline.md)) |
| `bench_path_storm` | 500 units issued simultaneous cross-map move orders through chokepoints, repeated | Pathfinding worst case (R-SIM-5) |
| `bench_econ_macro` | 12 players, full base-building, training, harvesting loops, 20k ticks | Order queues, production, event system |
| `bench_input_spam` | `bench_battle` workload + maximal-rate command stream | Command decode/validation path (R-INP-1.7) |

Each scenario runs under three harness modes: `go test -bench` wall-time benchmarks (with
`benchstat`-compatible output), `testing.AllocsPerRun` allocation gates
([GC Discipline §5](./gc-discipline.md)), and a memory-watermark run reporting peak Go heap.

## 4. The scripted render benchmark scene

Rendering cannot be benchmarked headless, so the render harness is a real engine build
driving a **scripted, fully deterministic scene**: fixed camera path (spline over the
benchmark map), scripted sim underneath (a recorded command stream again), fixed 90-second
duration, segmented:

1. **Typical** (60 s): mixed economy + small skirmishes, ~150 units visible, full HUD,
   audio at full voice budget ([Audio §5](../07-platform/audio.md)).
2. **Max battle** (20 s): the 500-unit battle fully in frustum — the R-RND-3 draw-call
   ceiling and the 30 FPS floor are evaluated here. Draw calls per frame are counted by the
   render layer and gated (≤ 300) in the same run.
3. **Stress transitions** (10 s): rapid camera jumps, mass deaths, selection churn —
   hunting allocation spikes and hitches.

Where it runs:

- **Reference machine** (§5): absolute gates — the only environment where FPS numbers are
  treated as acceptance values. Milestone exits (M4, M6) and a weekly scheduled run.
- **CI GPU runner** (a fixed, dedicated machine — never cloud-shared, to keep variance
  low): relative regression gates per PR from M4.
- **Mesa `llvmpipe` software GL in containers**: not for timing (software rasterization
  distorts everything) but valuable for *correctness* CI — the scene renders, screenshot
  comparisons for the HUD ([UI and HUD §8](../07-platform/ui-and-hud.md)), draw-call
  counting, and frame-path allocation gates, all of which are GPU-independent.

## 5. Reference machine definition

[PRD §5.3](../../PRD.md#53-performance-budgets-acceptance-gates-low-tier-reference-machine-dual-core-2-ghz-intel-uhd-620-4-gb-ram)
names the class; this section pins it down so "passes on the reference machine" is
reproducible:

| Component | Specification |
|---|---|
| CPU | x86-64, **2 cores / 4 threads, 2.0 GHz sustained** class (Intel Pentium Gold / i3-7020U / i5-7Y54 era). Turbo disabled or irrelevant: budgets must hold at sustained clocks under thermal steady state |
| GPU | **Intel UHD 620** integrated (24 EU, shared memory) — the single most common low-tier laptop GPU of the target audience era |
| RAM | **4 GB total system** (≈ 2.5–3 GB available to the process after OS + iGPU carve-out — the real reason the match budget is 1.5 GB, not 4) |
| Storage | SATA SSD class (~500 MB/s). Budgets are *not* validated against HDDs; cold start and map load assume SSD |
| Display | 1366 × 768 (the dominant low-tier panel) — render gates measured at this resolution; 1920 × 1080 measured and reported, not gated, in v1 |
| OS / driver | Windows 10 22H2 with current Intel driver, and Ubuntu LTS with Mesa — both measured; the **worse** result is the gating number |
| Power | AC power, OS "balanced" profile, thermal steady state (5-minute warm-up load before measurement) |

The lab machine matching this spec is inventoried, its exact model recorded in
`bench/reference-machine.md` in-repo, and never upgraded silently — replacing it is a
documented decision, because every historical absolute number is calibrated to it.

## 6. Regression gates (CI, from M3)

Per the PRD, budgets are CI-enforced **from M3 onward** (sim gates at M3, render gates at
M4 when the render core exists). Mechanics:

- **Baselines** live in-repo (`bench/baselines/*.json`), keyed by scenario × machine class,
  updated only by an explicit, reviewed baseline-update commit — never automatically.
- **Per-PR sim gates** (every PR touching `litd/sim`, `litd/api`, or shared code):
  - All §3 scenarios run; state hash must match the recorded hash (determinism gate —
    hard fail).
  - `testing.AllocsPerRun` on tick paths must be **exactly 0** (R-GC-1/R-GC-5 — hard fail,
    no tolerance, see [GC Discipline §5](./gc-discipline.md)).
  - Wall-time: 10 benchmark iterations, compared to baseline with `benchstat`;
    fail if the scenario regresses **> 5 %** with statistical significance (p < 0.05).
    The 5 % band absorbs CI noise; the absolute ≤ 10 ms gate is asserted whenever the run
    executes on the reference machine or the dedicated runner.
- **Per-PR render gates** (PRs touching `litd/render`, shaders, HUD): draw-call count gate
  (absolute, machine-independent), frame-path allocation gate (absolute zero), frame-time
  regression > 5 % on the dedicated GPU runner.
- **Scheduled absolute runs:** weekly full suite on the reference machine; results posted
  to the trend dashboard; a red absolute budget opens a blocking issue against the current
  milestone.
- **Milestone exits** (M3 sim, M4 render, M6 all of §5.3 green — see
  [PRD §7](../../PRD.md#7-milestones)): full absolute suite on the reference machine, both
  OSes, results attached to the milestone review.

## 7. Profiling workflow

Standardized so any contributor investigates a regression the same way:

1. **Reproduce via scenario.** Every investigation starts from a §3/§4 scenario (or a new
   recorded command stream that becomes a scenario). No ad-hoc "it feels slow".
2. **CPU:** `go test -bench=Battle500 -cpuprofile cpu.out ./bench/...` then
   `go tool pprof -http=: cpu.out`. The harness also supports attaching
   `net/http/pprof` in dev builds of the full engine for live capture.
3. **Allocations:** `-memprofile` + `pprof -sample_index=alloc_objects` to find the
   allocation site, then the escape-analysis workflow in
   [GC Discipline §6](./gc-discipline.md) to fix it.
4. **Frame traces:** `runtime/trace` capture around a marked window of the render
   benchmark (`trace.Start` hotkey in dev builds); `go tool trace` shows GC pauses,
   goroutine scheduling, and the sim-tick/render-frame interleaving. This is the tool for
   *hitches* — p99 problems that flat profiles average away. GC behavior is corroborated
   with `GODEBUG=gctrace=1`.
5. **GPU-side:** when CPU frame time is green but FPS is not, the bottleneck is GPU:
   RenderDoc (Linux/Windows, works with G3N's GL) against the benchmark scene, checking
   overdraw, state changes, and draw-call batching against R-RND-2…7 assumptions.
6. **Record the finding.** Closed performance investigations append a one-paragraph
   entry (cause, fix, scenario that now guards it) to `bench/postmortems.md` — the
   regression test corpus grows from real incidents.

## 8. Acceptance criteria

1. From M3, no PR merges with a red determinism, allocation, or > 5 % wall-time regression
   gate on the sim scenarios; from M4, same for render gates.
2. Every budget row in PRD §5.3 has an automated measurement producing archived JSON, and
   milestone exits attach reference-machine results.
3. The benchmark scenarios of §3–§4 exist as versioned, replayable command streams; each
   asserts its state hash before reporting timings.
4. `bench/reference-machine.md` documents the physical machine; absolute gate results are
   only ever claimed from it or its documented successor.

## 9. Related documents

- [GC Discipline](./gc-discipline.md) — allocation budgets R-GC-1…5, the other half of the performance contract.
- [Input §8](../07-platform/input.md) — the command-stream format benchmark workloads are recorded in.
- [UI and HUD §6](../07-platform/ui-and-hud.md), [Audio §5](../07-platform/audio.md) — subsystem budgets enforced inside these scenes.
- [PRD §5.3](../../PRD.md#53-performance-budgets-acceptance-gates-low-tier-reference-machine-dual-core-2-ghz-intel-uhd-620-4-gb-ram) and [PRD §7](../../PRD.md#7-milestones) — parent budgets and milestone gates.

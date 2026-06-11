# Determinism (R-SIM-2)

**Parent:** [PRD §5.1](../../PRD.md) · **Related:** [ECS Architecture](ecs-architecture.md) · [Tick & Scheduler](tick-and-scheduler.md) · [Pathfinding](pathfinding.md) · [Combat & Orders](combat-and-orders.md)

---

## 1. Requirement statement

R-SIM-2 requires **bit-for-bit determinism**: the same map plus the same ordered command stream must produce the same simulation state hash on every run, on every supported platform (Linux/Windows/macOS × amd64/arm64), in every build of the same engine version. This is the load-bearing requirement for the entire product: replays, headless CI verification (R-SIM-4), and committed lockstep multiplayer (D-2026-06-11-5, milestone M7) are all *derived* from it. A single non-deterministic operation anywhere in the tick path silently poisons all three.

Determinism is therefore treated as an architectural property enforced by construction and verified continuously, not a quality achieved by debugging desyncs after the fact.

## 2. Numeric representation: DECIDED — fixed-point `int64` 32.32

*Revised 2026-06-11 per D-2026-06-11-1.*

The question is decided: **all gameplay math is fixed-point `int64` 32.32** (D-2026-06-11-1, PRD §5.1/§9). The M1 spike still runs, but its purpose shifts from arbitration to **validation** — it measures the 32.32 implementation's performance against the ≤ 10 ms tick budget and calibrates range/precision margins. Ordered-float (Option B) is dropped as a candidate; the only condition that reopens the question is M1 showing fixed-point cannot meet the tick budget. The candidate analysis below is retained as the rationale record.

### 2.1 Option A — fixed-point arithmetic

Two sub-variants:

| Variant | Storage | Fractional precision | Effective range | Multiply intermediate |
|---|---|---|---|---|
| **16.16** | `int32` | ~0.0000153 (2⁻¹⁶) | ±32,768 world units | `int64` (widen, multiply, shift) |
| **32.32** | `int64` | 2⁻³² | ±2.1 × 10⁹ world units | 128-bit via `math/bits.Mul64` or split-multiply |

Assessment:

- **16.16 / int32** is the classic RTS choice (and the spiritual match for WC3-era engines). A 128×128 map at 128 world units per cell spans ~16,384 units — comfortably inside ±32k. The hazard is *intermediate overflow*: squared distances exceed int32 range almost immediately, so every multiply must widen to `int64` before the >>16 shift, and squared-distance comparisons must be done entirely in 64-bit. This is mechanical but must be enforced by convention (a `fixed.F32` named type with methods; raw operator use on the underlying int32 forbidden by lint rule).
- **32.32 / int64** removes overflow anxiety for positions and distances, but plain multiplication of two 32.32 values overflows int64; the helper must use `bits.Mul64`/`bits.Div64` (both compile to single instructions on amd64 and arm64). Doubles the memory footprint of every positional component — relevant against the SoA cache-efficiency goals of [R-SIM-3](ecs-architecture.md) and the 4 GB RAM target.
- Both variants are **trivially deterministic**: Go integer arithmetic is fully specified by the language (two's complement, defined shift and overflow behavior) and identical on every architecture Go supports. There is nothing the compiler or CPU can do to change a result.
- Cost: no hardware `sqrt`/`sin`/`cos`. Trigonometry comes from precomputed lookup tables (an angle type quantized to e.g. 1/65536 of a turn indexing 16k-entry sine tables) and integer Newton–Raphson square roots. This is well-trodden RTS territory and also *faster* than float transcendentals on the low-tier CPU target.

### 2.2 Option B — strictly ordered float32

Floats *can* be deterministic if (and only if) every operation is performed in the same order, at the same precision, with no compiler-introduced reassociation or contraction. IEEE-754 basic operations (+, −, ×, ÷, √) are exactly specified; the danger is everything around them.

### 2.3 Go-specific hazards (apply to Option B; several apply to the whole sim regardless)

1. **FMA contraction.** The Go spec explicitly permits the compiler to fuse `x*y + z` into a single fused-multiply-add when the hardware supports it. arm64 has FMA and the gc compiler *uses it*; amd64 codegen historically does not fuse by default. The same Go expression can therefore yield different low bits on arm64 vs amd64. The spec-blessed suppression is to force intermediate rounding with explicit `float64(x*y) + z`-style conversions — fragile, invisible in review, and one missed site away from a desync. This single hazard is the strongest argument against floats.
2. **`math` package non-determinism.** `math.Sin`, `Cos`, `Atan2`, `Pow`, etc. are *not* specified to the bit; they have assembly implementations on some architectures and pure-Go fallbacks on others, and results may differ in the last ulp across platforms and Go releases. An ordered-float sim must ship its own bit-specified transcendental implementations (e.g. table + polynomial with fixed evaluation order) and never call `math` in gameplay code — at which point much of the float convenience advantage evaporates. `math.Sqrt` is the exception (IEEE-exact, compiles to a hardware instruction) and would be permitted.
3. **Map iteration order.** Go randomizes `map` iteration deliberately, per run. Any gameplay loop over a map — iterating entities, buffs, subscribed event handlers — produces a different order each execution, which changes float accumulation order, target-selection ties, and event sequence. R-SIM-2 bans `map` iteration in gameplay code outright: keyed slices, sorted index arrays, and the [ECS dense stores](ecs-architecture.md) are the only iterable collections in the tick path. Maps are permitted only as lookup indices whose iteration is never observed (and a `go vet`-style custom analyzer in CI flags `range` over maps inside `litd/sim`).
4. **Goroutine scheduling.** Goroutine interleaving is non-deterministic by design. No computation whose result feeds simulation state may involve concurrent goroutines within a tick (R-SIM-5 reiterates this for pathfinding). The script scheduler of [R-EXEC-1](tick-and-scheduler.md) avoids the hazard structurally: suspensions are descriptive records resumed one at a time in deterministic order, with no goroutines in the script path at all (*revised 2026-06-11 per D-2026-06-11-9 — the serializable stackless scheduler*).
5. **Miscellaneous:** `select` with multiple ready cases chooses randomly; `time.Now()` and any wall-clock reads are banned inside the tick; struct padding bytes must never enter the state hash (hash field-by-field, not by raw memory); GOARCH-dependent `int`/`uintptr` sizes mean gameplay code uses explicitly sized types only.

### 2.4 The fixed-point package sketch

*Revised 2026-06-11 per D-2026-06-11-1 (32.32 over `int64`).*

The numeric type is wrapped once, in one package, with the raw representation unexported in spirit (Go cannot fully hide it without losing zero-cost composition, so a lint rule enforces what the type system cannot):

```go
package fixed

type F64 int64 // 32.32 — named type; raw arithmetic on it is lint-banned

const One F64 = 1 << 32

func FromInt(i int32) F64   { return F64(i) << 32 }
func (a F64) Mul(b F64) F64 // 128-bit intermediate via bits.Mul64, then >>32
func (a F64) Div(b F64) F64 // <<32 widening into 128-bit, then bits.Div64
func (a F64) Floor() int64  { return int64(a >> 32) }

// Squared distances on full 32.32 coordinates exceed int64. Range tests
// compare in 128-bit (hi, lo) form via bits.Mul64 — never materialized
// as F64 — so comparisons stay exact with no overflow to police.
func DistSqLess(a, b Vec2, r F64) bool

type Angle uint16 // 1/65536 of a turn; wraps for free, indexes sin/cos tables

func (t Angle) Sin() F64 // table lookup, 16384-entry quarter-wave table
func (t Angle) Cos() F64
func SqrtU64(v uint64) uint32 // integer Newton–Raphson, for actual ranges/speeds
```

Design notes:

- `Angle` as a binary fraction of a turn (a "BAM" — binary angular measurement) makes wrap-around free, comparison exact, and table indexing a shift — and it removes π from the codebase, the single most common source of float creep.
- Squared-distance comparison via `DistSqLess` (128-bit compare, `bits.Mul64`) is the canonical range test everywhere; actual square roots are rare (movement normalization) and go through `SqrtU64`. `bits.Mul64`/`bits.Div64` compile to single instructions on amd64 and arm64, so the 128-bit discipline costs little.
- The package ships exhaustive cross-checked tests (against `math/big` rationals) and is frozen early — every gameplay system depends on its exact behavior, so changing it after M3 is effectively a save/replay-format break.

### 2.5 Decision record (supersedes the staff recommendation)

*Revised 2026-06-11 per D-2026-06-11-1.*

The staff leaning entering the spike was 16.16 over `int32` (halved positional memory, WC3-scale range sufficiency). The owner decision selects **32.32 over `int64`** instead: overflow anxiety disappears for positions, distances, and accumulators rather than being policed by convention and lint; the multiply/divide helpers are single-instruction on both target architectures; and the doubled positional footprint is absorbed — the entire sim state remains single-digit megabytes even at the D-2026-06-11-18 capacity targets ([ECS §5.1](ecs-architecture.md)). Both fixed-point variants share the decisive property: hazards 1–2 of §2.3 disappear entirely instead of being policed. Ordered float32 is dropped; the question reopens **only** if M1 shows the 32.32 backend cannot meet the ≤ 10 ms tick budget.

### 2.6 Lua VM determinism (D-2026-06-11-8) — audit scope

*Added 2026-06-11 per D-2026-06-11-8.*

Deterministic embedded Lua (gopher-lua family) ships in v1 (M5), and world scripts execute inside the tick — so the Lua VM is gameplay code, sits inside the determinism boundary, and every hazard in §2.3 applies to it. The determinism audit of the VM is an M5 entry gate, with this scope:

- **Arithmetic.** Stock gopher-lua numbers are `float64`, which would reintroduce every Option B hazard through the back door of script math. The constraint: Lua arithmetic must either **route through the same fixed-point discipline** — the VM patched/forked so the script-visible number type is `fixed.F64` with the same Mul/Div/trig semantics (preferred) — or be **strictly confined**: float values exist only VM-internally and never reach sim state except through API bindings that accept/return `fixed.F64` and convert at the boundary with a specified rounding rule. Confinement is fragile (a script that computes a position with float math and issues a move order has already laundered platform-dependent bits into a command), so it is acceptable only if the audit can demonstrate that *every* sim-visible numeric path crosses a converting boundary; failing that, the fixed-point number type is mandatory.
- **Table iteration.** Lua `pairs`/`next` order must be fully specified and identical across runs and platforms; if gopher-lua's table implementation leans on Go map iteration anywhere observable, that implementation is replaced. This is the script-side twin of hazard 3.
- **No ambient entropy.** `os`, `io`, wall-clock, and stock `math.random` are already stripped by the R-SEC-1 hard sandbox (D-2026-06-11-20); `math.random` rebinds to the sim PRNG (§4) so script randomness is part of the single deterministic stream.
- **Quotas are counted, never timed.** The per-tick instruction and memory quotas (R-SEC-1) are counted work — like the pathfinding expansion budget, their values are part of sim semantics and the replay/version contract; a wall-clock cutoff would itself be a desync source.
- **Coroutine state is data.** A gopher-lua coroutine's suspension state is ordinary Go heap data (no native stack), which is what makes it serializable for R-SIM-6 — see [Tick & Scheduler §3](tick-and-scheduler.md).

## 3. The M1 spike design

*Revised 2026-06-11 per D-2026-06-11-1: the spike validates the decided fixed-point backend rather than arbitrating between representations.*

The spike produces the evidence that 32.32 fixed-point meets the budgets, plus the permanent regression harness. The ordered-float backend is no longer built; the harness keeps the backend seam so a float candidate *could* be re-added if the D-1 reopening condition (tick-budget failure) ever fires.

**Workload.** A miniature but representative sim: 500 entities (the low-tier guarantee scale), plus a 1,000-entity variant per the D-2026-06-11-18 stretch target, exercising fixed-point movement integration, distance checks, A\* over a 256×256 grid with the deterministic tie-breaking rules from [Pathfinding](pathfinding.md), simple combat (target acquisition + damage), and PRNG-driven events — every operation class the real sim will use, behind one backend interface.

**Reproducibility test.** Run **10,000 ticks** from a fixed seed and a scripted command stream; record the 64-bit state hash every 100 ticks (a 100-entry hash trace, not just the final hash, so divergences are localized to a 100-tick window). The trace must be byte-identical across:

- **OS:** Linux, Windows, macOS (CI runners for all three);
- **Architecture:** amd64 and arm64 (Linux/arm64 runner + macOS/arm64 runner — the arm64 leg is what catches FMA contraction);
- **Build modes:** `-race` off/on (the race detector must also report zero races), optimized and `-gcflags="-N -l"` unoptimized (catches optimization-dependent codegen), and two consecutive Go toolchain versions.

**Pass criteria:** all platform/build cells produce identical hash traces for the 32.32 fixed-point backend (expected: trivially green — Go integer arithmetic is fully specified). Performance is measured per R-SIM benchmark conventions: ns/tick on the reference low-tier machine at 500 entities and on the recommended-spec machine at 1,000, plus `AllocsPerRun == 0` per R-GC-1.

**Deliverable:** decision record in `docs/decisions/`, the spike harness promoted into the permanent CI determinism suite (R-SIM-4 headless), run on every commit thereafter.

The full test matrix:

| Axis | Cells |
|---|---|
| OS | Linux, Windows, macOS |
| Arch | amd64, arm64 (Linux + macOS arm64 runners) |
| Optimization | default, `-gcflags="all=-N -l"` |
| Race detector | off, on (`-race` must also report zero races) |
| Toolchain | current Go release, previous Go release |
| Backend | fixed-point 32.32 (ordered float32 retired per D-2026-06-11-1) |

Not every combination runs on every commit post-M1 (the steady-state CI suite runs the decided backend on Linux/amd64 + Linux/arm64 + Windows/amd64 per commit, full matrix nightly); the spike itself runs the complete matrix once per candidate change.

**Spike exit questions the validation record must answer** (*revised 2026-06-11 per D-2026-06-11-1*):

1. ns/tick and worst-tick latency for the 32.32 backend at 500 entities (low-tier reference machine) and 1,000 entities (recommended spec) against the ≤ 10 ms budget (PRD §5.3) — failure here is the *only* condition that reopens D-1.
2. Range/precision calibration: where 32.32 margins land for movement integration, squared-distance comparison, and long-running accumulators; which paths require 128-bit intermediates and whether any of them are hot in profiles.
3. Whether the lookup-table trig and `SqrtU64` precision suffice for movement normalization and projectile arcs without visible quantization artifacts at sim scale.
4. Confirmation that `AllocsPerRun == 0` holds (a backend that forces boxing or table allocations fails R-GC-1 regardless of determinism).

## 4. Seeded PRNG design

A single PRNG instance is owned by the sim, per R-SIM-2. Design:

- **Algorithm:** a small, fully specified generator implemented in-repo — PCG32 or xoshiro256\*\* — *not* `math/rand`, whose algorithm and seeding behavior have changed across Go releases (notably Go 1.20's auto-seeding and the v2 package). Owning ~30 lines of generator code removes the toolchain from the trust boundary.
- **Seeding:** the seed is part of the match setup payload (map hash + host-chosen seed), recorded in the replay header. Headless replay re-seeds identically.
- **Single stream, strict ordering:** every `Roll` call advances the one stream, so the *sequence of calls* is part of determinism. This is safe because the [tick phase order](tick-and-scheduler.md) and intra-phase entity ordering are themselves deterministic. Sub-streams per system (combat rolls vs. script `Random`) may be split from the master seed at match start if profiling shows contention between systems' call ordering and future code mobility — but each sub-stream remains single-threaded and tick-owned.
- **API boundary:** scripts get randomness only through the sim (`g.Random(...)` analogues of JASS `GetRandomInt/Real`), drawing from the sim stream. Nothing in `litd/sim` or script-visible API touches `crypto/rand`, `math/rand`, or time-derived entropy.
- **Determinism trap to document loudly:** any *conditional* PRNG call (e.g. "roll only if the unit is visible") couples the stream to the condition; conditions must themselves be deterministic state, never render or local-player state.

## 5. State-hashing strategy

The state hash is the determinism oracle — for the M1 spike, CI replay verification, and M7 lockstep desync detection (D-2026-06-11-5).

- **What is hashed:** all authoritative gameplay state — every live ECS component row in [SoA store order](ecs-architecture.md) (entity index order, which is itself deterministic), entity generation counters, order queues, buff/ability state machines, the PRNG cursor, the script scheduler's sleeper queue, pathfinding grid dynamic state, tick counter, and pending event queue. **Excluded:** anything render-side, interpolation state, caches that are provably derived (a derived cache that *isn't* provably derived is a bug the hash should catch — when in doubt, hash it during development and demote later).
- **How:** field-by-field serialization into a streaming non-cryptographic 64-bit hash (xxHash64 or FNV-1a, implemented in-repo for the same toolchain-independence reason as the PRNG). Never hash raw struct memory: padding, GOARCH layout differences, and float NaN payloads would all leak in. Each component store contributes its fields column-by-column (SoA makes this a linear scan — cache-friendly and allocation-free, satisfying R-GC-1).
- **Cadence:** full hash every tick in CI/headless mode; every N ticks (default 100) in normal play, embedded in replays as checkpoints; on-demand via debug command.
- **Divergence forensics:** in CI mode, alongside the rolling hash, per-system sub-hashes (movement hash, combat hash, …) are recorded so a divergence report names the first tick *and* the first system whose sub-hash split — turning a desync from an archaeology project into a bisect.

## 6. Replay format as determinism artifact

Replays are the cheapest, most user-visible proof of R-SIM-2, and they cost almost nothing once determinism holds: a replay is **inputs, not state**.

- **Header:** engine version, data-table content hash (R-AST-1 tables are inputs too — a balance patch changes outcomes), map hash, match seed, player roster.
- **Body:** the ordered command stream — `(tick, playerIndex, command)` tuples exactly as ingested by tick phase 1 ([Tick & Scheduler §4](tick-and-scheduler.md)).
- **Checkpoints:** the state hash every N ticks (default 100), enabling fast divergence detection without re-running to the end and giving M7 lockstep its desync-check payload for free.
- **Versioning:** replays are valid only for the exact engine version + table hash in the header. Cross-version replay is explicitly out of scope for v1 (it would require either state migration or frozen sim behavior, both heavyweight); the header makes the incompatibility detectable and reportable rather than a silent desync.

Replay verification in headless CI (R-SIM-4) — record a scripted match, replay it on every platform, compare checkpoint traces — is the integration-level twin of the unit-level 10k-tick spike test.

## 7. Enforcement summary

| Mechanism | Hazard covered |
|---|---|
| Fixed-point `fixed.F64` type, lint ban on raw arithmetic | overflow misuse, accidental float math |
| CI analyzer: no `range` over map, no `time.Now`, no `go` statement, no `math.*` in `litd/sim` | hazards 2–5 |
| Lua VM audit (§2.6): fixed-point number type or converting API boundary; specified table order; counted instruction/memory quotas | script-side float, entropy, and ordering hazards (D-2026-06-11-8/-20) |
| 10k-tick multi-platform hash-trace test on every commit | everything, end to end |
| Per-system sub-hashes | desync localization |
| In-repo PRNG + hash implementations | toolchain version drift |

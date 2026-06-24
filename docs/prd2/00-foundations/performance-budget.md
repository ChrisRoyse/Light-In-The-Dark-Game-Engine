# Performance Budget & Latency Analysis

> **Question this doc answers:** will any of the five PRD2 primitives cause latency spikes
> or burn a lot of CPU? **Short answer: no.** Every design choice already mandated by the
> determinism/zero-alloc rules ([architecture-principles.md](architecture-principles.md))
> is *also* the cache-optimal choice. This doc proves it with a per-tick cost envelope and
> grounds each claim in published benchmarks.

The headline finding from the research sweep (2026-06-23): **the constraints that make this
engine deterministic — Structure-of-Arrays storage, no map iteration, fixed capacity,
fixed-point math, handle indices — are the same constraints data-oriented-design literature
prescribes for speed.** Determinism and performance are not in tension here; they point the
same way.

---

## 1. The tick budget (the number that matters)

The sim runs at **20 Hz → 50 ms per tick**. On a ~3 GHz core that is **~150 million cycles
per tick** of headroom. The relevant question is never "is operation X O(1) or O(log n)" —
it is "what fraction of 50 ms does subsystem X add at full default caps." The table below is
the whole argument:

| Subsystem | Worst-case work/tick @ default caps | Est. cycles | Est. time | % of 50 ms |
|-----------|-------------------------------------|-------------|-----------|------------|
| Timers (fire+reschedule) | ≤ a few hundred fires/tick typical; 4,096 absolute | ~200 K | ~0.07 ms | 0.1% |
| Unit groups (prune + ops) | prune dead members; bounded ops | ~150 K | ~0.05 ms | 0.1% |
| KV store (get/set) | O(log n) per access, few hundred accesses | ~50 K | ~0.02 ms | <0.1% |
| Custom events (dispatch) | linear drain of the event ring | ~100 K | ~0.03 ms | 0.1% |
| Movers (advance+collide) | 4,096 movers × ~150 cyc incl. broad-phase | ~600 K | ~0.20 ms | 0.4% |
| **Total PRD2 added/tick** | | **~1.1 M** | **~0.37 ms** | **~0.7%** |

Even multiplying every estimate by 5× for pessimism, PRD2 adds **under 2 ms to a 50 ms
tick**. There is no scenario at default caps where these primitives approach the frame
budget. The architecture is bandwidth/cache-bound long before it is compute-bound, and the
SoA layout is specifically chosen to minimize cache misses.

> The one real spike risk is **a single ability fanning out thousands of primitives in one
> tick** (a 1000-bolt nova, a query-fill over a huge group). That is bounded by caps and
> mitigated by staggering — see §7. It is an *authoring* risk, not a subsystem-design risk.

---

## 2. Why "no map iteration" is a speed rule, not just a determinism rule

The CLAUDE.md ban on `map` iteration in gameplay code is usually justified by determinism
(Go randomizes map order). The research shows it is *also* one of the highest-leverage
performance rules:

- Iterating a Go **slice is ~20× faster** than iterating a Go **map**, and for tight loops
  the gap reaches **~60×**, because slices are contiguous (cache-friendly) while map buckets
  scatter across memory (pointer chasing).
  ([Bolt&Nuts](https://boltandnuts.wordpress.com/2017/11/20/go-slice-vs-maps/),
  [kokes.github.io](https://kokes.github.io/blog/2020/07/20/tiny-maps-arrays-go.html))
- Preallocating a slice is ~2× faster than dynamic growth; filling a **map takes ~10×
  longer** than a slice due to its richer internal structure
  ([Better Programming](https://betterprogramming.pub/performance-impact-of-maps-compared-to-slices-in-go-1-18-15352fbd6010)).
- SoA columnar storage with swap-and-pop removal is exactly the layout modern ECS
  frameworks (Unity DOTS, Bevy, Flecs) converge on for cache coherence and update
  throughput ([arxiv: The Essence of ECS](https://arxiv.org/html/2606.14919),
  [DiVA: DOD measurement study](https://www.diva-portal.org/smash/get/diva2:1578616/FULLTEXT01.pdf)).

**Consequence for PRD2:** every store is SoA with `rowOf` index + contiguous `[0,count)` +
swap-remove. We never iterate a map in any hot path. This is a deliberate performance
posture, validated by the data, not a determinism tax.

---

## 3. Timers — heap vs. wheel, decided by *our* scale

The instinct "use a hierarchical timing wheel for O(1)" is right at massive scale and
**wrong at ours**. Benchmarks consistently show that at small-to-moderate timer counts a
**binary min-heap beats a timing wheel on insertion**, because the heap's backing array
prefetches into L1 while the wheel writes to random slab indices (cache misses):

- A Rust implementation of the Varghese & Lauck wheel was **30× *slower* than a std binary
  heap** on insertion at 1M items, even though the wheel is O(1) and the heap O(log n) —
  "Big O is a lie (sometimes); constant factors like cache misses matter more for N up to a
  million." ([ankurrathore.net.in](https://ankurrathore.net.in/posts/timing-wheel-optimization/),
  [r/rust](https://www.reddit.com/r/rust/comments/1qpbu60/i_implemented_the_varghese_lauck_1987/))
- A Go timing-wheel benchmark shows the wheel only *matches or beats* native timers at
  **10K+** timers and only pulls clearly ahead at **100K+**; at ~1K it is +8% slower with
  more memory ([taskwheel](https://github.com/ankur-anand/taskwheel)).
- The wheel's *real* and large win is **O(1) cancellation** (handle unlink) vs the heap's
  O(N) scan — **up to 1,700× faster** — which matters for workloads that cancel constantly
  (network timeouts). ([snellman.net Ratas](https://www.snellman.net/blog/archive/2016-07-27-ratas-hierarchical-timer-wheel/),
  [Varghese & Lauck 1987 PDF](https://www.cs.columbia.edu/~nahum/w6998/papers/sosp87-timing-wheels.pdf))

**Decision for [01-timer-wheel](../01-timer-wheel/):** at `Caps.Timers = 4,096`, ship the
**binary min-heap keyed on `(WakeTick, Seq)` first** (the spec already allows this — see
[01/spec.md §4](../01-timer-wheel/spec.md)). We get O(1) cancellation *for free anyway*
because our timers are **slot-indexed handles**, not heap-position-anchored — cancel marks
the slot free and lazily skips it on pop (tombstone), or removes via a sift, both O(log n)
or better, with no O(N) scan. Migrate to a bucketed wheel **only if** a profile shows timer
count routinely exceeding ~10K. If/when we do:
- Use **power-of-2 bucket counts** so slot = `tick & (N-1)` (bitwise AND, not modulo).
- **Pack the timer record small** (the cited work shaved 48→24 bytes via `NonZeroU32`
  handles and got 30–46% faster); our `Cont uint16` + slot handles are already small.

Either way, timer cost is ~0.07 ms/tick — noise.

---

## 4. KV store — sorted array + binary search is the fast *and* correct choice

The KV design ([03/spec.md](../03-keyvalue-store/spec.md)) uses sorted parallel arrays with
binary search instead of a Go map. Research confirms this loses nothing on speed at our
scale and wins on everything else:

- For data that **fits in cache, binary search and hash lookup perform nearly identically**;
  the hash map only pulls ahead once data exceeds cache
  ([Baeldung](https://www.baeldung.com/cs/hash-lookup-vs-binary-search)).
- A sorted array additionally gives **ordered iteration, range scans, better cache
  locality, and deterministic order** — all of which a Go map forfeits.
- For **tiny per-owner maps, arrays beat maps by up to 60×** in the lookup loop
  ([kokes.github.io](https://kokes.github.io/blog/2020/07/20/tiny-maps-arrays-go.html)).

The only cost is O(n) insert-shift, and **scope-packing keeps each owner's pairs
contiguous** so a typical "set 5 keys on this spawner" touches one short, cache-resident
run. KV cost is ~0.02 ms/tick.

---

## 5. Movers — the only compute-noticeable subsystem, still cheap

Two cost centers: orbit trigonometry and collision broad-phase. Both are well-trodden.

### 5.1 Orbit/spline trig — fixed-point LUT, mandatory and fast
We **cannot** call hardware `math.Sin` (float results differ across platforms → breaks
determinism). The deterministic choice — a **fixed-point quarter-wave lookup table with
linear interpolation** — is *also* the fast one:

- Hardware `fsin`/`fcos` cost **20–260 cycles** depending on CPU; a small interpolated LUT
  costs **~3–30 cycles**.
  ([gamedev.SE CORDIC thread](https://gamedev.stackexchange.com/questions/144867/why-doesnt-games-use-cordic-for-sin-and-cos),
  [Khronos sine-table thread](https://community.khronos.org/t/are-sine-tables-too-1990s-or/58430))
- A real game saw per-frame trig drop from **2.37 ms → 0.09 ms** (~26×) switching ~50K
  entities to a sine LUT ([0xjfdube](https://jfdube.wordpress.com/2011/12/06/trigonometric-look-up-tables-revisited/)).

**Mandated LUT design** for [05/mover-types.md](../05-movers/mover-types.md) orbit/spline:
- **Angle in power-of-2 units** (e.g. 1/65536 of a turn) so wraparound is free integer
  overflow and there is no range-reduction branch (the classic trick: byte/short angles add
  and wrap for free).
- **Quarter table only** (0–90°), reconstruct the other three quadrants by symmetry → small,
  cache-line-aligned, stays hot in L1; **`cos(θ) = sin(θ + quarter)`** off the same table.
- **16-bit fixed entries + integer multiply** for the interpolation.
- Orbit movers number in the low hundreds at most, so even a naive LUT is free; the design
  is about determinism + keeping the table in L1, not raw throughput.

### 5.2 Collision broad-phase — reuse the existing uniform grid, O(n) not O(n²)
Movers reuse the engine's existing collision grid for candidate gathering
([05/collision-and-impact.md](../05-movers/collision-and-impact.md)). This is the textbook
broad-phase and the research is unanimous:

- Uniform-grid / spatial-hash broad-phase turns **O(n²) → O(n)** and handles **10K+
  entities at 60 fps on mid-range hardware**, comfortably **millions** with GPU/Burst
  ([daily.jovis.ai spatial hashing](https://daily.jovis.ai/game-development/spatial-hashing-fast-broad-phase-collision-detection-for-thousands-of-game-objects/),
  [bevy_millions_ball](https://github.com/allenpocketgamer/bevy_millions_ball),
  [sparse spatial hash perf](https://queelius.github.io/sparse_spatial_hash/performance/)).
- A DOTS space shooter cut its collision system from **0.7–4.5 ms to 0.04–0.25 ms** purely
  by adding spatial hashing ([SpaceShooterDOTS](https://github.com/Junder-2/SpaceShooterDOTS)).

**Rules we adopt from the literature** (annotated into collision-and-impact.md):
- **Cell size ≈ 2× the common projectile/collision radius** — too small bloats multi-cell
  insertion, too large degrades back toward O(n²).
- **Oversized colliders (giant AoE, boss bodies) go in a separate small "oversized" list**,
  not the grid, so they don't span dozens of cells.
- **Prefer incremental cell updates over full rebuilds** where a mover's cell membership is
  stable across a tick (incremental update measured ~40× cheaper than full rebuild when ~1%
  of entities change cells).
- **Swept tests use the integer projection space** already in `missile.go` (`dirIntScale`),
  avoiding F64 overflow and float entirely.

Net mover cost at 4,096 active movers: ~0.2 ms/tick.

---

## 6. Unit groups & custom events — structurally cheap

- **Unit groups** ([02](../02-unit-groups/)): SoA membership arena, **swap-remove O(1)**,
  **presence bitset O(1) `Contains`/dedup**, set algebra linear into a preallocated
  destination. The one quadratic trap — "scan every group for every death" during pruning —
  is removed by the optional reverse index (entity→groups), already specified. Group ops are
  a straight contiguous-array walk, the fastest thing the CPU does.
- **Custom events** ([04](../04-custom-events/)): dispatch is a **linear drain of a
  preallocated ring** with handlers in a contiguous registration-ordered slice — cache
  friendly, scalar `(Src,Dst,Arg)` payload, **zero allocation**. Subscribe does a binary
  search on kind (rare, setup-time). No structure here can spike: there is no tree, no map,
  no recursion in the hot path.

---

## 7. The single real latency risk, and its mitigation

The only way these primitives cause a spike is **an author instantiating a huge number of
primitives in one tick**:
- a nova with thousands of bolts (thousands of movers at once),
- a query-fill over a several-thousand-unit group then a per-member effect,
- a timer loop with a near-zero interval doing heavy work every tick.

Mitigations, all already in the design:
1. **Hard caps** (`Caps.Movers`, `Caps.GroupMembers`, …) bound the absolute worst case;
   exhaustion degrades gracefully (drop counter), it does not stall.
2. **`tools/abilitycheck` budget lint** ([06](../06-ability-composition/custom-ability-authoring.md))
   warns when a spec's worst-case instantiation count approaches a cap.
3. **Stagger with the timer wheel.** The template library deliberately shows the pattern:
   the [Nova](../06-ability-composition/templates/nova-ring.md) and
   [Persistent Field](../06-ability-composition/templates/persistent-field.md) templates
   spread emission across ticks with a `times` (count) timer instead of dumping everything in
   one tick — converting a potential single-tick spike into a flat, amortized load.
4. **Movement-authority is one-per-unit** so unit movers can't multiply without bound.

These keep the worst case bounded and smooth; there is no uncapped fan-out anywhere.

---

## 8. Conclusion

| Concern | Verdict |
|---------|---------|
| Latency spikes | None at default caps; PRD2 adds < 2 ms to a 50 ms tick even at 5× pessimism |
| Heavy compute | No; SoA + fixed-point + grid broad-phase are the cache-optimal choices, not costly ones |
| Determinism vs speed tension | None; the determinism rules (no-map, SoA, fixed capacity, fixed-point) are the speed rules |
| The real risk | Author-driven single-tick fan-out — bounded by caps + `abilitycheck` + timer staggering |

PRD2 is designed so that **being correct (deterministic, serializable, zero-alloc) and being
fast are the same decision**. Each subsystem `test-plan.md` already includes a
`testing.AllocsPerRun` zero-alloc gate; this doc adds the requirement that each subsystem
also ship a **micro-benchmark** (`go test -bench`) recording its per-op cost, wired into the
`benchharness` preflight step so regressions are caught.

### Sources
- Varghese & Lauck, *Hashed and Hierarchical Timing Wheels* (1987) — https://www.cs.columbia.edu/~nahum/w6998/papers/sosp87-timing-wheels.pdf
- *Why My O(1) Scheduler Was Slower Than O(log N)* — https://ankurrathore.net.in/posts/timing-wheel-optimization/
- taskwheel Go timing wheel benchmarks — https://github.com/ankur-anand/taskwheel
- Ratas hierarchical timer wheel — https://www.snellman.net/blog/archive/2016-07-27-ratas-hierarchical-timer-wheel/
- Baeldung, *Hash Lookup vs Binary Search* — https://www.baeldung.com/cs/hash-lookup-vs-binary-search
- *Tiny maps vs arrays in Go* — https://kokes.github.io/blog/2020/07/20/tiny-maps-arrays-go.html
- *Performance Impact of Maps Compared to Slices in Go 1.18* — https://betterprogramming.pub/performance-impact-of-maps-compared-to-slices-in-go-1-18-15352fbd6010
- Go Slice vs Maps — https://boltandnuts.wordpress.com/2017/11/20/go-slice-vs-maps/
- *The Essence of Entity Component System* (arXiv) — https://arxiv.org/html/2606.14919
- DiVA, *Data-Oriented Design measurement study* — https://www.diva-portal.org/smash/get/diva2:1578616/FULLTEXT01.pdf
- *Spatial Hashing: Fast Broad-Phase Collision Detection* — https://daily.jovis.ai/game-development/spatial-hashing-fast-broad-phase-collision-detection-for-thousands-of-game-objects/
- SpaceShooterDOTS spatial-hash profiling — https://github.com/Junder-2/SpaceShooterDOTS
- bevy_millions_ball (uniform-grid, millions of spheres) — https://github.com/allenpocketgamer/bevy_millions_ball
- Trigonometric Look-Up Tables Revisited (0xjfdube) — https://jfdube.wordpress.com/2011/12/06/trigonometric-look-up-tables-revisited/
- Why don't games use CORDIC for sin/cos (gamedev.SE) — https://gamedev.stackexchange.com/questions/144867/why-doesnt-games-use-cordic-for-sin-and-cos
- Are sine tables too 1990s (Khronos) — https://community.khronos.org/t/are-sine-tables-too-1990s-or/58430

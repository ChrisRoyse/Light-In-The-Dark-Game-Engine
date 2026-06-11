# Tick Loop & Deterministic Script Scheduler (R-SIM-1, R-SIM-4, R-EXEC-1/2/5)

**Parent:** [PRD §4.4, §5.1](../../PRD.md) · **Related:** [Determinism](determinism.md) · [ECS Architecture](ecs-architecture.md) · [Pathfinding](pathfinding.md) · [Combat & Orders](combat-and-orders.md)

---

## 1. The 20 Hz fixed timestep loop (R-SIM-1)

The simulation advances in discrete ticks of exactly **50 ms of game time** (20 Hz, WC3-compatible cadence), fully decoupled from rendering. Game time is an integer tick counter; **no gameplay system ever sees a float delta-time** — every rate in the data tables (movement speed, regen, cooldowns) is converted at load into per-tick fixed-point increments or integer tick counts. This eliminates the entire class of dt-dependent bugs and is a precondition for [bit-for-bit determinism](determinism.md).

The driver is the standard fixed-timestep accumulator:

```
accumulator += min(realFrameTime, maxFrame)   // maxFrame clamps the spiral of death
for accumulator >= tickDuration {
    sim.Step()                                // exactly one deterministic tick
    accumulator -= tickDuration
}
alpha = accumulator / tickDuration            // 0..1, render-side only
```

- `realFrameTime` is clamped (default 250 ms) so a stall produces game-time slowdown rather than an unbounded catch-up burst ("spiral of death" guard).
- Wall-clock readings exist **only in the driver loop**, never inside `sim.Step()` ([Determinism §2.3](determinism.md)).
- The tick budget is ≤ 10 ms worst case (PRD §5.3), leaving 50% headroom at 20 Hz. Provisional per-phase split, tracked by the headless benchmark suite and re-baselined in M3:

| Phase | Provisional budget |
|---|---|
| Input commands + scripts | 1.0 ms |
| Orders + abilities/buffs | 1.0 ms |
| Pathing (expansion-budgeted, see [Pathfinding §6](pathfinding.md)) | 2.0 ms |
| Movement + collision | 2.0 ms |
| Combat + projectiles | 2.0 ms |
| Events + cleanup + snapshot | 1.5 ms |
| Reserve | 0.5 ms |

### 1.1 Game speed and pause

WC3-style game speed (slow/normal/fast) and pause are **driver-level** concepts: speed scales how much real time maps to one tick (e.g. fast = 1.25× ticks per real second); pause stops feeding the accumulator. The sim itself never knows the speed — a replay recorded at fast speed replays identically at slow speed because tick content is speed-independent. Single-player pause still renders (interpolation alpha frozen); the sim simply does not step. These choices keep speed/pause out of the determinism surface entirely.

## 2. Render interpolation contract

Render runs at its own rate (target 60 FPS) and **interpolates between the two most recent completed sim states** using `alpha`. The contract between `litd/sim` and `litd/render` (PRD §4.1 hard rule: render never writes sim):

- After each tick, the sim publishes a compact **render snapshot**: per-entity position, facing, animation cue, life fraction — value-typed, copied into one of two preallocated snapshot buffers (double-buffered, zero allocation per R-GC-1). The render thread reads snapshots; it has no access to live ECS stores.
- Visual position = `lerp(prevSnapshot, currSnapshot, alpha)`; facing uses shortest-arc angular interpolation. Rendered state is therefore up to one tick (50 ms) behind authoritative state — invisible at RTS camera distance and standard for the genre.
- **Discontinuities don't interpolate:** teleports, spawns, and deaths set a "snap" flag in the snapshot so render does not smear a blink across 50 ms.
- Events that drive one-shot presentation (attack impact sound, death animation start) flow through a render-facing event queue tagged with their tick; render fires them when it crosses that tick boundary.
- The snapshot is the *entire* sim→render surface. Nothing render-side (camera, selection, hover state, local player identity) may flow back into the sim except as **commands** through the front door (§4 phase 1) — this is the same discipline that keeps replays and future lockstep viable.

## 3. The deterministic cooperative script scheduler (R-EXEC-1/2/5)

JASS semantics (PRD §4.4): script "threads" are cooperative coroutines that yield only at explicit waits, with exactly one running at a time. LitD reproduces this contract precisely, because thousands of existing WC3 design patterns (and the API's own `PolledWait`-style helpers, §4.2 D4) depend on it.

### 3.1 Mechanism

- Each script context (map script main, each suspended event handler) is a goroutine used **strictly as a coroutine**: it runs only when the scheduler hands it the baton (an unbuffered channel handoff), and the scheduler does not proceed until the coroutine yields the baton back at a wait point or returns. At any instant, at most one of {scheduler, exactly one script coroutine} is executing. There is no concurrency, only Go-flavored coroutines — this is the controlled exception to the "no goroutines in the tick" rule documented in [Determinism §2.3](determinism.md).
- **R-EXEC-5 — tick-quantized waits:** the only yield points are `Wait`-family calls. All durations quantize **up** to whole ticks (50 ms granularity); `Wait(0)` means "resume next tick". No sub-tick timing exists anywhere in the public API, so scripts cannot couple to render rate.
- **Sleeper queue:** suspended coroutines sit in a queue keyed by `(wakeTick, sequenceNumber)` where `sequenceNumber` is a monotonically increasing suspension counter. Resume order within a tick is strictly ascending by that key — fully deterministic, insertion-order-stable for equal wake ticks, and included in the [state hash](determinism.md) (the queue's contents are authoritative state).
- Timers (`g.After`, periodic timers — the JASS `timer` analogue) are entries in the same sleeper queue; a timer callback is just a coroutine spawned at its wake tick, ordered by the same key.

The baton mechanism, sketched:

```go
type coroutine struct {
    resume chan struct{} // scheduler -> script: "run"
    yield  chan yieldKind // script -> scheduler: "I waited" | "I returned"
}

// Inside a script's Wait(d):
func (ctx *scriptCtx) Wait(d Duration) {
    wake := ctx.sched.now + d.Ticks()           // quantized up, min 1
    ctx.sched.sleepers.Push(wake, ctx.nextSeq()) // deterministic key
    ctx.co.yield <- yieldWait                    // hand baton back
    <-ctx.co.resume                              // block until our wake tick
}

// Scheduler resume loop (tick phase 2):
for sched.sleepers.PeekTick() == sched.now {
    co := sched.sleepers.Pop() // ascending (wakeTick, seq)
    co.resume <- struct{}{}
    if <-co.yield == yieldReturn { sched.release(co) }
}
```

The channels are unbuffered, so each send is a synchronous rendezvous: the Go scheduler *cannot* run scheduler and script simultaneously, regardless of `GOMAXPROCS`. The coroutine goroutines and their channel pairs come from a preallocated pool (R-GC-2); spawning a handler reuses a parked goroutine rather than calling `go` in the hot path.

A worked example of the contract, end to end:

```go
g.OnEvent(litd.EventUnitDeath, func(e litd.Event) {
    e.Killer().AddGold(50)            // synchronous, this tick, this phase
    helpers.PolledWait(2*time.Second) // suspends: re-queued at now+40 ticks
    g.CreateUnit(...)                 // resumes in phase 2, 40 ticks later
})
```

Everything before the wait happens at the dispatch point in registration order; everything after happens in a later tick's script phase, in sleeper-queue order — exactly the JASS trigger-thread model, with Go syntax.

### 3.2 Event handlers (R-EXEC-2)

- `OnEvent` handlers run **synchronously at the event dispatch point**, in **deterministic registration order** per event type. Registration order is itself deterministic because registration happens in script code, which runs deterministically.
- A handler that calls a wait suspends onto the sleeper queue and resumes on a later tick like any coroutine; the dispatch loop continues with the next handler immediately (matching JASS's thread-per-firing semantics without the thread zoo).
- Condition-style filters (the `boolexpr` replacement, R-EXEC-4) are pure functions by API construction — they receive read-only state and return bool, with no access to mutating calls, so they can never need to wait and can never desync.

### 3.3 Failure containment

A script coroutine that panics is caught at the baton boundary, its context is killed, and a deterministic script-error event is logged/raised (the panic *message* is debug-only per R-GC-3, but the fact and order of the failure is deterministic state). A coroutine that never yields trips a per-resume opcode/step budget (the JASS opcode-limit analogue) in debug builds; release builds document the contract.

## 4. Order of phases within one tick

One `sim.Step()` executes these phases in fixed order (system-level detail in [ECS §6](ecs-architecture.md)):

| # | Phase | Contents |
|---|---|---|
| 1 | **Input commands** | Drain the command queue for this tick (player UI commands; in future lockstep, the agreed per-tick command set). Commands are validated, then applied — mostly by writing [orders](combat-and-orders.md). The command queue is the *only* door from the outside world into the sim. |
| 2 | **Scripts** | Scheduler resumes all coroutines with `wakeTick == now` in sleeper-queue order; fires due timers. Script effects (unit creation, orders issued) apply immediately, exactly as in JASS. |
| 3 | **Orders** | Order queues resolve into concrete intents: current order decides target point/entity for movement, attack, cast (see [Combat & Orders](combat-and-orders.md)). |
| 4 | **Movement** | Path requests serviced within budget ([Pathfinding](pathfinding.md)), grid restamps from construction, position integration, avoidance, facing. |
| 5 | **Combat** | Target acquisition, attack cycle state machines, projectile advancement and impact, damage application via deferred-effect buffers. |
| 6 | **Events** | Deterministic flush of the tick's gameplay events (deaths, damage, order completion) to handlers per §3.2; handlers may enqueue further state changes that land this phase or, if they wait, on later ticks. |
| 7 | **Cleanup** | Death resolution, corpse decay clocks, swap-remove of dead rows, pool/free-list recycling, buff expiry sweep; render snapshot published; state hash on its cadence. |

Phase order is part of the engine's semantic contract: "a unit ordered this tick moves this tick; damage dealt this tick raises events this tick; entities dying this tick disappear after events fire" — fixed, documented, and relied upon by scripts.

## 5. Headless mode (R-SIM-4)

The sim must run with **no GPU, no window, no G3N import** — enforced structurally because `litd/sim` does not import `litd/render` (PRD §4.1), and verified by a CI build of the headless binary with `CGO_ENABLED=0` working where G3N (cgo/OpenGL) could never compile.

Headless capabilities:

- **Replay verification:** load map + replay (seed + command stream), run to end, compare hash trace against the recorded checkpoints ([Determinism §5](determinism.md)). This is the CI determinism gate from M1 onward.
- **As-fast-as-possible stepping:** the accumulator driver is replaced by a tight loop calling `sim.Step()` — 10k ticks of the spike workload run in seconds, making the determinism suite cheap enough for every commit.
- **Benchmarking:** headless is the substrate for the §5.3 tick-budget and R-GC-1 zero-alloc CI gates (500-unit benchmark scene, `testing.AllocsPerRun`).
- **Scripted integration tests:** game scenarios written against `litd/api` run headless and assert on sim state — the testing story for the entire API surface (M5 audit) without a single frame rendered.

The headless driver and the windowed driver are two thin front-ends over the identical `sim.Step()`; there is no "headless variant" of any gameplay code path, so what CI verifies is what players run.

## 6. Command queue: the sim's single front door

Because phase 1 is the only entry point for external influence, its contract deserves spelling out:

- Commands are small value structs (`tick`, `playerIndex`, opcode, target variant) appended by the UI thread to a lock-protected staging buffer; the driver moves staged commands into the sim's per-tick queue **between** ticks, never during one. The lock is outside `sim.Step()`, so the sim itself remains lock-free and single-threaded.
- Command *validation* (does the player own that unit? is the ability ready?) happens inside phase 1, deterministically — an invalid command becomes a deterministic no-op, never an error path that diverges. This is mandatory for replays (a recorded command must replay to the same no-op) and is the WC3 model.
- The same queue shape is the future lockstep seam (PRD §2.2): v2 netcode will fill the per-tick queue from the network instead of the local UI, delaying ticks until all players' command sets arrive. Nothing inside `sim.Step()` will change — which is precisely why the determinism groundwork is in v1 scope while netcode is not.

## 7. Invariants summary

| Invariant | Enforced by |
|---|---|
| One tick = 50 ms game time, integer tick counter, no dt in gameplay code | code review + absence of any dt parameter in `litd/sim` signatures |
| Render reads snapshots only; commands are the only inbound path | package structure (no sim import of render), snapshot API surface |
| One runnable script coroutine at a time, deterministic resume order | unbuffered-channel baton + sleeper queue key, [hash-verified](determinism.md) |
| All waits ≥ 1 tick, quantized up | `Wait` implementation; no sub-tick API exists to misuse |
| Phase order fixed | single hand-written `Step()` function, reviewed like an ABI |
| Headless bit-identical to windowed | same `Step()`, replay-verification CI gate |

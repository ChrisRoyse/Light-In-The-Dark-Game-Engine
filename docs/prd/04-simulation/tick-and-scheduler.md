# Tick Loop & Deterministic Script Scheduler (R-SIM-1, R-SIM-4, R-SIM-6, R-EXEC-1/2/5)

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

### 3.1 Mechanism — serializable by construction (R-SIM-6): DECIDED and spike-validated

*Revised 2026-06-11 per D-2026-06-11-9 (serializable scheduler) and D-2026-06-11-8 (Lua execution). Revised again 2026-06-11 per D-2026-06-11-28: the design is **decided, not a candidate** — validated by the executed `spikes/scheduler` spike.*

Mid-game save/load is v1 scope (D-9), and R-SIM-6 makes the scheduler **serializable from day one**: suspended script threads, timers, and event subscriptions must all write into the save format and restore bit-identically — campaign cross-map persistence (D-15) rides the same mechanism. That requirement decided the implementation question, and the M1-scope spike has since **validated the decision** (D-28):

- **Rejected — baton-passing goroutines** (the original design, kept here as a historical record only): each script context is a goroutine used strictly as a coroutine via unbuffered-channel handoff. Deterministic and concurrency-free — but **it cannot satisfy R-SIM-6**: a suspended script's continuation lives on its goroutine stack, and Go provides no mechanism to inspect, serialize, or reconstruct a goroutine stack. A mid-game save taken while any script sleeps inside a `Wait` would be impossible. **Per D-28 this design is dead**, not a fallback.
- **THE design — stackless / descriptive suspension** (D-28): a suspended script thread is **data, not a stack**. Each suspension is a record `(wakeTick, seq, continuation, state)` in the sleeper queue, where the continuation is a *serializable reference* — a registered, stably-identified function plus a value-typed state payload for Go-authored scripts (never a bare Go closure, which is as unserializable as a stack), or a Lua coroutine for D-8 scripts (see below). Suspension is pushing a record; resume is invoking the continuation. The sleeper queue, timers, and subscription tables are plain data structures that serialize directly.

The two script surfaces sit differently on candidate B:

- **Go-authored scripts** express waits in explicit continuation style (`g.After(d, contID, state)` resumes a registered continuation; multi-step sequences are state machines, hand-written or generated). Mid-function blocking waits are *not* available to Go script code under this design — an accepted ergonomic cost, since Go is the systems language where explicit state machines are idiomatic.
- **Lua scripts (D-8)** keep the full JASS-style linear ergonomics: a gopher-lua coroutine's entire execution state (call-stack frames, locals, upvalues) is ordinary Go heap data — there is no native stack — so a suspended Lua coroutine serializes and restores exactly, mid-`Wait`. *(Revised 2026-06-11 per D-2026-06-11-25: the persistence mechanism is concrete — the **coroutine/LState persister patch** on the vendored gopher-lua fork serializes call frames, registry, and upvalues, with function protos referenced by chunk-id; see [Determinism §2.6](determinism.md).)* Since Lua is the creation surface (D-8), the audience that most needs `PolledWait`-in-the-middle-of-a-trigger gets it.

**Spike result (D-2026-06-11-28, `spikes/scheduler`) — the M1 decision rule is discharged:** the spike implemented scripts as descriptive suspension records (PC + locals + typed suspension), a sleep queue keyed `(wakeTick, seq)`, and event waiters FIFO by seq, then gob-serialized the **full scheduler state mid-run**, restored it, and advanced — traces and state **bit-identical** with the uninterrupted run, resume order deterministic. The goroutine-baton design is dead; stackless descriptive suspension is **the** scheduler design (R-SIM-6, D-9), not a candidate. M1's remaining scheduler work is production hardening (real save format instead of gob, pooling per R-GC-2, the Lua `suspKind` once the D-25 persister lands), not design choice.

Mechanism-independent rules (unchanged):

- **R-EXEC-5 — tick-quantized waits:** the only yield points are `Wait`-family calls. All durations quantize **up** to whole ticks (50 ms granularity); `Wait(0)` means "resume next tick". No sub-tick timing exists anywhere in the public API, so scripts cannot couple to render rate.
- **Sleeper queue:** suspension records sit in a queue keyed by `(wakeTick, sequenceNumber)` where `sequenceNumber` is a monotonically increasing suspension counter. Resume order within a tick is strictly ascending by that key — fully deterministic, insertion-order-stable for equal wake ticks, included in the [state hash](determinism.md), and serialized into the save format (R-SIM-6): the queue's contents are authoritative state twice over.
- Timers (`g.After`, periodic timers — the JASS `timer` analogue) are entries in the same sleeper queue, ordered by the same key, serialized the same way.

The suspension record, sketched:

```go
type suspension struct {
    wakeTick uint32
    seq      uint32
    kind     suspKind // goContinuation | luaCoroutine | timer
    contID   ContID   // Go path: registered continuation, ID stable across runs/builds
    state    contState // Go path: value-typed payload, fixed max size, pooled
    luaCo    *luaThread // Lua path: coroutine — heap data, persisted by the D-25 fork patch
}

// Scheduler resume loop (tick phase 2):
for sched.sleepers.PeekTick() == sched.now {
    s := sched.sleepers.Pop() // ascending (wakeTick, seq)
    sched.run(s)              // invoke continuation / resume Lua coroutine;
                              // a new Wait pushes a fresh record, else release to pool
}
```

Exactly one suspension runs at a time, on the scheduler's own thread — no goroutines, no channels, nothing the Go runtime scheduler can reorder. (The rejected baton design's sketch is preserved in git history; its determinism contract — one runnable script at a time, ascending-key resume — carries over unchanged, and the spike confirmed the stackless implementation honors it.)

A worked example of the contract, end to end — Lua, the surface where linear waits live:

```lua
g.onEvent(EVENT_UNIT_DEATH, function(e)
    e.killer:addGold(50)        -- synchronous, this tick, this phase
    helpers.polledWait(2.0)     -- suspends: record queued at now+40 ticks
    g.createUnit(...)           -- resumes in phase 2, 40 ticks later —
end)                            -- and survives a save/load taken in between
```

Everything before the wait happens at the dispatch point in registration order; everything after happens in a later tick's script phase, in sleeper-queue order — exactly the JASS trigger-thread model. The Go equivalent registers a continuation for the post-wait half.

### 3.2 Event handlers (R-EXEC-2)

- `OnEvent` handlers run **synchronously at the event dispatch point**, in **deterministic registration order** per event type. Registration order is itself deterministic because registration happens in script code, which runs deterministically.
- Subscription tables (event type → ordered handler list) are authoritative state: they serialize into the save format by stable handler ID (R-SIM-6, *revised 2026-06-11 per D-2026-06-11-9*) — a loaded save re-arms exactly the triggers that were armed.
- A handler that calls a wait suspends onto the sleeper queue and resumes on a later tick like any suspension; the dispatch loop continues with the next handler immediately (matching JASS's thread-per-firing semantics without the thread zoo).
- Condition-style filters (the `boolexpr` replacement, R-EXEC-4) are pure functions by API construction — they receive read-only state and return bool, with no access to mutating calls, so they can never need to wait and can never desync.

### 3.3 Failure containment

*Revised 2026-06-11 per D-2026-06-11-8/-20 (R-SEC-1 quotas in all builds).*

A script that panics (Go continuation) or errors (Lua) is caught at the resume boundary, its context is killed, and a deterministic script-error event is logged/raised (the panic *message* is debug-only per R-GC-3, but the fact and order of the failure is deterministic state). A script that never yields trips the per-resume **instruction quota** (the JASS opcode-limit analogue) — for Lua this is the R-SEC-1 hard-sandbox quota and is enforced in **all builds**, not just debug, because it doubles as the lockstep stall guard (§3.5); Go-side continuations are engine-trusted code covered by the debug-build step budget and review.

### 3.4 The AI scheduler domain (D-2026-06-11-6, M5.5)

*Added 2026-06-11 per D-2026-06-11-6.*

The full `commonai` port (milestone M5.5) adds a **second scheduler domain**: an isolated instance of the same scheduler machinery with its own sleeper queue, its own script contexts, and **no shared globals** with the map-script domain. The JASS AI model is reproduced structurally:

- The domains communicate **only via command-stack messaging** (R-EXEC-3): the map domain pushes commands onto per-player AI command stacks; AI scripts pop and interpret them. No direct calls and no shared mutable state cross the boundary.
- Within tick phase 2, the map-script domain runs first, then the AI domain — each draining its own sleeper queue in `(wakeTick, seq)` order. Domain order is fixed and part of the phase contract.
- Both domains serialize identically under R-SIM-6: an AI player's suspended decision loops survive a mid-game save like any trigger thread.
- AI scripts execute as Lua under the same sandbox and quota regime as map scripts (§3.5); quotas are **per domain**, so a runaway AI script cannot starve map triggers, and vice versa.

### 3.5 Lua execution and per-tick quotas (D-2026-06-11-8, R-SEC-1)

*Added 2026-06-11 per D-2026-06-11-8 and D-2026-06-11-20.*

World and AI scripts run as Lua coroutines on this scheduler — same sleeper queue, same ordering key; a Lua coroutine and a Go continuation are just two `suspKind`s of the same suspension record. Three constraints bind the Lua path:

- **Determinism:** the VM is inside the determinism boundary — fixed-point number discipline, insertion-ordered table iteration, and the vendored gopher-lua fork's four-patch plan per [Determinism §2.6](determinism.md) (*revised 2026-06-11 per D-2026-06-11-25*).
- **Quotas (R-SEC-1):** every resume runs under a per-tick **instruction quota**, and each VM under a **memory quota** — both *counted*, never timed, so enforcement is bit-identical on every machine. Exceeding a quota is a deterministic script fault handled by the §3.3 containment edge.
- **Lockstep stall guard:** because the quota is counted work, a hostile or runaway world script burns the same bounded budget on every lockstep peer (M7, D-2026-06-11-5) and faults at the same instruction on every machine — quota enforcement doubles as the stall guard, turning "one client hangs, the match hangs" into a deterministic, replayable script error. The quota values are part of sim semantics and the replay/version contract, exactly like the [pathfinding expansion budget](pathfinding.md).

## 4. Order of phases within one tick

One `sim.Step()` executes these phases in fixed order (system-level detail in [ECS §6](ecs-architecture.md)):

| # | Phase | Contents |
|---|---|---|
| 1 | **Input commands** | Drain the command queue for this tick (player UI commands; in future lockstep, the agreed per-tick command set). Commands are validated, then applied — mostly by writing [orders](combat-and-orders.md). The command queue is the *only* door from the outside world into the sim. |
| 2 | **Scripts** | Map-script domain scheduler resumes all suspensions with `wakeTick == now` in sleeper-queue order and fires due timers; then the **AI domain** (§3.4, from M5.5) does the same in its own isolated context, consuming command-stack messages (R-EXEC-3). Script effects (unit creation, orders issued) apply immediately, exactly as in JASS. *Revised 2026-06-11 per D-2026-06-11-6.* |
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
- The same queue shape is the lockstep seam (D-2026-06-11-5, milestone M7): the M7 netcode fills the per-tick queue from the network instead of the local UI, delaying ticks until all players' command sets arrive. Nothing inside `sim.Step()` will change — which is precisely why the determinism groundwork is v1-wide while netcode waits for M7.

## 7. Invariants summary

| Invariant | Enforced by |
|---|---|
| One tick = 50 ms game time, integer tick counter, no dt in gameplay code | code review + absence of any dt parameter in `litd/sim` signatures |
| Render reads snapshots only; commands are the only inbound path | package structure (no sim import of render), snapshot API surface |
| One runnable script suspension at a time, deterministic resume order | stackless scheduler runs records sequentially on its own thread + sleeper-queue key, [hash-verified](determinism.md) |
| Scheduler state is data: suspensions, timers, subscriptions serialize (R-SIM-6, D-9) | descriptive suspension records, no goroutine stacks; save→load→resume mid-`Wait` hash-trace CI fixture |
| All waits ≥ 1 tick, quantized up | `Wait` implementation; no sub-tick API exists to misuse |
| AI domain isolated from map-script domain (D-6) | separate scheduler instance, no shared globals, command-stack messaging only (R-EXEC-3), fixed domain order |
| Lua per-tick instruction/memory quotas, counted never timed (R-SEC-1, D-8/D-20) | VM instrumentation; quota values part of the replay/version contract; doubles as M7 lockstep stall guard |
| Phase order fixed | single hand-written `Step()` function, reviewed like an ABI |
| Headless bit-identical to windowed | same `Step()`, replay-verification CI gate |

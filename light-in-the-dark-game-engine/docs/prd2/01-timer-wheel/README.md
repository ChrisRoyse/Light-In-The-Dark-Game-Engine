# 01 — Serializable Timer Wheel

> **Requirement namespace:** `R-TMR-*` ([../00-foundations/requirement-ids.md](../00-foundations/requirement-ids.md))
> **Closes:** #270 (closure timers / wake records not serializable)
> **Primary tick phase:** phase 2 (scripts), folded into the scheduler drain
> **New sub-hash:** `"timers"`
> **New caps:** `Caps.Timers` (default 4,096)

---

## Problem

Time is the substrate of nearly every gameplay behavior. Cooldowns, channel durations,
spawn/respawn delays, periodic damage, buff ticks, delayed teleports, "warn 3 seconds
then explode" telegraphs, fireworks stepping, AI re-evaluation loops — all of them are
"do X after N ticks" or "do X every N ticks." The tutorial corpus uses timers in the
majority of its lessons.

Today the engine has two partial answers, neither robust:

1. **`Game.After/Every`** (`litd/api/timer.go`) — ergonomic, but the callback is a Go
   closure. The scheduler *record* (wakeTick, slot, generation) is value-typed and
   serializes, but the closure table is rebuilt by Go code, so **Go-closure timers do
   not survive save/load** (`api/timer.go:24-29`). This is the #270 defect.
2. **`PolledWait` / scheduler `After`** (`litd/sim/sched`) — value-typed and serializable
   for Lua coroutines, but it is a low-level *wait* primitive for a single suspended
   script, not a first-class, queryable, cancellable, repeating **timer object** that
   gameplay systems and abilities can own, inspect, and tear down.

## Solution

A first-class **timer wheel**: a fixed pool of serializable timer records, each
addressed by a generation-checked `TimerID`, each firing a **continuation by stable ID**
with a value payload — never a captured closure. Three modes (single / loop / count), an
optional owner entity for auto-cleanup, deterministic `(wakeTick, seq)` ordering, and a
named sub-hash. `Game.After/Every` are re-expressed on top of it; the closure form
survives only as documented save-unsafe sugar.

## What this unlocks

- **Cooldowns and channels** that survive save/load.
- **Spawners** (the canonical "respawn 10 s after cleared" loop) as a one-call loop timer
  owned by the spawner entity.
- **Telegraphed abilities** ("warn, wait, detonate") as a single-shot timer carrying the
  detonation context in its payload.
- **Periodic mover/ability ticks** without per-map Lua scheduling.
- The completion/teardown half of the [mover](../05-movers/) and
  [ability](../06-ability-composition/) systems.

## Documents

- [spec.md](spec.md) — data model, modes, lifecycle, ordering, tick integration.
- [serialization-and-hashing.md](serialization-and-hashing.md) — byte format, hash fold,
  the #270 closure→continuation migration.
- [api.md](api.md) — Go API and Lua binding.
- [test-plan.md](test-plan.md) — determinism, save/load round-trip, zero-alloc, FSV.

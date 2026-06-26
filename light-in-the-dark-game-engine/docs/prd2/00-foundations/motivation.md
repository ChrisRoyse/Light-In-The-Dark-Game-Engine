# Motivation & Gap Analysis

> Evidence base for why PRD2 prioritizes these five primitives, and why they are
> specified as engine-level subsystems rather than left to per-map script.

---

## 1. The two evidence sources

### 1.1 The tutorial corpus (`docs/theclicli-transcripts.md`)

A 56-video tutorial series for a mature WC3-lineage visual editor was transcribed and
analyzed. It is the closest public artifact to "what a complete authoring surface over
the WC3 capability set looks like in practice." Cataloguing every mechanic the
tutorials *build* (not just mention) yields a striking concentration: a handful of
primitives recur in almost every lesson.

| Primitive | Tutorials that depend on it (non-exhaustive) |
|-----------|----------------------------------------------|
| **Timers** | enemy spawner (respawn delay), NPC chat (distance poll), fireworks (frame stepping), boss state machine (skill cooldown loop), AI tick loop, item surrounding-detection (frame timer), gathering quest (progress poll) |
| **Unit groups** | one-click kill, AOE ability damage, AI team iteration, escort enemy waves, lowest-HP healer target search, audience celebration |
| **Custom key-values** | spawner (enemy type/amount/angle per area), quest state (0/1/2), equipment type tagging, NPC dialog content, mother/child spawner↔unit linkage |
| **Custom events** | boss state machine (sleep/battle/transform/death), behavior-tree AI (send AI-guard / AI-healer), equipment special-effect dispatch, "service bell" pub/sub pattern taught explicitly across 4+ videos |
| **Movers** | orbiting electric ball, fireworks rising/decay, boomerang projectiles, linear bullets, charging boss, seed-ring placement, all skillshot abilities |

The tutorials also teach a **mental model** that maps one-to-one onto these primitives:
"sequence / selection / loop" structures, "function library" (reusable parameterized
functions), and "send/receive custom events" (the service-bell metaphor). Our engine
already covers sequence/selection/loop (Lua) and the function library (Lua functions).
The missing or weak pieces are exactly timers, groups, key-values, custom events, and
general movers.

### 1.2 The codebase audit (parallel `Explore` sweeps, 2026-06-23)

| Primitive | Current state | Citation |
|-----------|---------------|----------|
| Timers | **Missing as a serializable sim primitive.** `Game.After/Every` exist but hold Go closures that do not survive save/load. | `litd/api/timer.go:24-29`; #270 |
| Unit groups | **Missing.** Only transient enumeration callbacks. | `QueryEnumUnits` (no persistent group type) |
| Key-values | **int32-only.** `UserDataStore` stores a single `int32` per unit. | `litd/sim/store_userdata.go:10` |
| Custom events | **Fixed registry.** `EventKind` is a hardcoded `iota` block; `OnEvent` fails closed on unknown kinds. No script-defined kinds. | `litd/api/event_payload.go:15-18`, `litd/api/events.go:90-93` |
| Movers | **Straight-line only.** `MissileStore` flies linear / to-point / homing with optional accel; `Arc` is render-only. No orbital/spline/parametric. Units path; missiles fly; the two motion models are disjoint. | `litd/sim/missile.go:30-50`, `litd/sim/store_missile.go` |

The audit also confirmed the *strengths* PRD2 must preserve and build on: the SoA store
pattern with generation-checked `EntityID` handles, the `statehash` registry with
ordered per-system sub-hashes (R-SIM-6), the value-typed cooperative scheduler
(`State [4]int64`, no goroutines), and the little-endian field-by-field save format.

---

## 2. Why these belong in the engine, not in per-map Lua

A reasonable objection: "Lua already has tables and closures — can't authors build
timers, groups, and key-values themselves?" They can, and that is exactly the failure
mode PRD2 prevents. If every map re-implements these in script:

1. **Determinism risk multiplies.** Each ad-hoc implementation is a fresh opportunity
   to iterate a Lua table in nondeterministic order, leak a float, or key on a
   wall-clock value. Centralizing the primitive lets us hash and gate it once.
2. **Serialization breaks silently.** A map's hand-rolled timer table that captures a
   closure will not survive save/load — the same #270 bug, re-introduced per map. An
   engine primitive serializes by construction.
3. **Zero-alloc is unenforceable.** Per-map Lua churns garbage. Engine primitives use
   preallocated pools and are covered by the `testing.AllocsPerRun` gate.
4. **AI authorability collapses.** An AI agent assembling an ability from documented
   engine primitives with typed signatures will produce correct, deterministic code far
   more reliably than one inventing a bespoke scheduler in Lua each time.
5. **Composition becomes possible.** The ability model in [06](../06-ability-composition/)
   only works because movers, timers, groups, and events are *named, addressable
   engine objects* that a declarative ability spec can reference. You cannot
   drag-and-drop a template that depends on undocumented per-map script.

---

## 3. The leverage argument (build order rationale)

The five primitives are not independent; they form a dependency lattice. PRD2 sequences
them so each one unlocks the next.

```
                 ┌─────────────────────┐
                 │ 1. Timer Wheel      │  time substrate — everything periodic/delayed
                 └──────────┬──────────┘
                            │ used by
        ┌───────────────────┼───────────────────────┐
        ▼                   ▼                         ▼
┌───────────────┐   ┌────────────────┐      ┌──────────────────┐
│ 2. Unit Groups│   │ 3. KV Store    │      │ 4. Custom Events │
│  (durable set)│   │ (per-obj attrs)│      │ (decoupled msgs) │
└───────┬───────┘   └────────┬───────┘      └─────────┬────────┘
        │                    │                        │
        └─────────┬──────────┴───────────┬────────────┘
                  ▼                       ▼
          ┌───────────────────────────────────────┐
          │ 5. Unified Movers (units + missiles)   │  motion for everything
          └───────────────────┬───────────────────┘
                              ▼
          ┌───────────────────────────────────────┐
          │ 6. Composable Abilities & Characters   │  authoring layer + templates
          └───────────────────────────────────────┘
```

- **Timers** are first because cooldowns, channels, periodic effects, and the
  spawn/respawn loop in every other system depend on a serializable delay primitive.
- **Groups, key-values, and custom events** are mutually independent and can be built in
  parallel once timers exist; each is a thin SoA store + API.
- **Movers** depend on all four: an orbital mover may carry a key-value payload, target a
  unit group, fire a custom event on completion, and schedule its own teardown on a
  timer. Movers are also the largest single subsystem and the one that most directly
  *supersedes* existing behavior (see [05](../05-movers/)).
- **Ability composition** is the capstone: it is pure assembly of 1–5 with no new sim
  state of its own beyond a declarative spec table.

This is why the roadmap ([07](../07-roadmap/milestones.md)) gates 1 first, fans out 2–4,
then lands 5 and 6.

# PRD2 — Core Simulation Primitives for Robust, Composable Gameplay

> **Status:** Draft v1 (2026-06-23)
> **Owner:** Engine / Simulation
> **Relationship to the master PRD:** This directory is a *forward* specification.
> Where `docs/PRD.md` and `docs/prd/` describe the engine as it was scoped, **PRD2 may
> contradict and supersede any prior design element that is judged "not robust enough"**
> for the gameplay goals below. When PRD2 and `docs/prd/` disagree on one of the five
> subsystems specified here, **PRD2 wins** and the older text is to be treated as
> historical. PRD2 never weakens the non-negotiable engine invariants (determinism,
> zero-alloc steady state, fixed capacity, sim/render separation); it strengthens them.

---

## 1. Why this document set exists

The engine already has a strong deterministic core (fixed-point math, SoA stores,
state hashing, lockstep netcode, save/load). What it lacks is the small set of
**general-purpose gameplay primitives** that every nontrivial map, ability, and
character behavior is built on top of. A gap analysis against a mature WC3-lineage
world editor (the "Quickly/Klickly" tutorial corpus, archived at
`docs/theclicli-transcripts.md`) showed that the same five primitives appear in
*nearly every* tutorial — spawners, quests, NPC dialog, equipment systems, AI,
projectile abilities, escort missions — and that we are missing or under-powering all
five.

PRD2 specifies those five primitives as **first-class, serializable, deterministic,
zero-alloc sim subsystems**, plus a sixth document set that shows how they *compose*
into an ability/character authoring model so that humans and AI agents can build new
abilities and units by assembling primitives rather than writing engine code.

### The five core primitives (build order)

| # | Primitive | Supersedes / closes | Why it is the foundation |
|---|-----------|---------------------|--------------------------|
| 1 | **Serializable Timer Wheel** ([01](01-timer-wheel/)) | Closes #270 (closure timers not serializable); replaces the Go-closure `Game.After/Every` posture | Time is the substrate of *every* behavior: cooldowns, spawns, periodic ticks, delays, channels. Highest leverage. |
| 2 | **Persistent Unit-Group Store** ([02](02-unit-groups/)) | Replaces transient `QueryEnumUnits` callbacks as the only grouping mechanism | The WC3 set type. AOE, "kill all", AI iteration, quest counting all need durable groups. |
| 3 | **Generic Typed Key-Value Store** ([03](03-keyvalue-store/)) | Supersedes `UserDataStore` (int32-only) | Custom per-object attributes — the backbone of spawners, quests, equipment, state machines. |
| 4 | **Script-Defined Custom Events** ([04](04-custom-events/)) | Extends the fixed `EventKind` registry | Decoupled message-passing between triggers/behaviors; the spine of behavior trees and FSMs. |
| 5 | **Unified Parametric Movers** ([05](05-movers/)) | **Supersedes** the straight-line-only `MissileStore` motion and the path-only unit motion | One motion model for units *and* projectiles: linear, homing, orbital, spline, custom. The creativity unlock. |

### The composition layer

| # | Layer | Document |
|---|-------|----------|
| 6 | **Ability & Character Composition** | [06](06-ability-composition/) — abilities declaratively spawn projectiles, attach movers, and run effects on collision; ships **drag-and-drop ability templates**. |

---

## 2. Design north stars (the "robust" bar)

Every subsystem in PRD2 MUST satisfy all of the following. These are acceptance gates,
not aspirations. They are derived from the existing engine invariants (R-SIM-*,
R-GC-*, R-EXEC-* in the master PRD) and tightened.

1. **Deterministic** — identical inputs ⇒ bit-identical state hash on every platform
   (R-SIM-1). No `map` iteration in gameplay order, no wall-clock, no float, no
   goroutines. All randomness via the sim PRNG.
2. **Serializable** — every byte of subsystem state round-trips through save/load and
   is folded into the determinism fingerprint in a **canonical, registration-stable
   order** (R-SIM-6). No Go closures in persisted state (this is the #270 lesson).
3. **Fixed-capacity, zero-alloc at steady state** — pools sized once at `NewWorld`;
   no `make`/`append`-growth mid-match; per-tick scratch reused via `buf[:0]`
   (R-GC-1, R-GC-2). Exhaustion is a gameplay outcome (creation fails), never a panic.
4. **Handle-safe** — all references are generation-checked `EntityID`-style handles;
   stale handles resolve to a safe no-op (R-API-5).
5. **Composable & authorable** — exposed through the public Go API *and* the Lua
   binding with value-type signatures (no G3N types, R-API-1..6), so the same
   capability is reachable by an AI agent writing Lua and by engine Go code.
6. **Hashable in isolation** — each subsystem registers its own named sub-hash in the
   `statehash` registry so divergence localizes to one system (`FirstDivergence`).

7. **Fast by construction** — the determinism rules above (SoA, no-map, fixed capacity,
   fixed-point) are *also* the cache-optimal choices per the data-oriented-design and
   game-engine literature. PRD2 adds **< 2 ms to a 50 ms tick at default caps even at 5×
   pessimism**; no subsystem approaches the frame budget. The only spike risk is
   author-driven single-tick fan-out, which is capped, lint-warned, and timer-staggered.
   Full analysis with benchmarks in
   [00-foundations/performance-budget.md](00-foundations/performance-budget.md).

See [00-foundations/architecture-principles.md](00-foundations/architecture-principles.md)
for the concrete patterns each spec must follow,
[00-foundations/performance-budget.md](00-foundations/performance-budget.md) for the latency
proof, and
[00-foundations/requirement-ids.md](00-foundations/requirement-ids.md) for the new
requirement-ID namespaces this document set introduces.

---

## 3. Index

### 00 — Foundations
- [Motivation & Gap Analysis](00-foundations/motivation.md) — the evidence from the tutorial corpus and the codebase audit.
- [Architecture Principles](00-foundations/architecture-principles.md) — the store pattern, hashing discipline, and zero-alloc rules every PRD2 subsystem inherits.
- [Performance Budget & Latency Analysis](00-foundations/performance-budget.md) — per-tick cost envelope proving PRD2 adds < 2 ms to a 50 ms tick; research-backed (timing wheels, spatial hashing, fixed-point trig, SoA vs maps).
- [Requirement IDs](00-foundations/requirement-ids.md) — `R-TMR-*`, `R-UGR-*`, `R-KV-*`, `R-EVT-*`, `R-MOV-*`, `R-ABL-*` namespaces.
- [Glossary](00-foundations/glossary.md) — shared vocabulary.

### 01 — Serializable Timer Wheel
- [Overview](01-timer-wheel/README.md)
- [Specification](01-timer-wheel/spec.md)
- [Serialization & Hashing](01-timer-wheel/serialization-and-hashing.md)
- [Public API & Lua Binding](01-timer-wheel/api.md)
- [Test & Verification Plan](01-timer-wheel/test-plan.md)

### 02 — Persistent Unit-Group Store
- [Overview](02-unit-groups/README.md)
- [Specification](02-unit-groups/spec.md)
- [Public API & Lua Binding](02-unit-groups/api.md)
- [Test & Verification Plan](02-unit-groups/test-plan.md)

### 03 — Generic Typed Key-Value Store
- [Overview](03-keyvalue-store/README.md)
- [Specification](03-keyvalue-store/spec.md)
- [Public API & Lua Binding](03-keyvalue-store/api.md)
- [Test & Verification Plan](03-keyvalue-store/test-plan.md)

### 04 — Script-Defined Custom Events
- [Overview](04-custom-events/README.md)
- [Specification](04-custom-events/spec.md)
- [Public API & Lua Binding](04-custom-events/api.md)
- [Test & Verification Plan](04-custom-events/test-plan.md)

### 05 — Unified Parametric Movers
- [Overview](05-movers/README.md)
- [Specification](05-movers/spec.md)
- [Mover Type Catalog](05-movers/mover-types.md)
- [Collision & Impact](05-movers/collision-and-impact.md)
- [Public API & Lua Binding](05-movers/api.md)
- [Test & Verification Plan](05-movers/test-plan.md)

### 06 — Ability & Character Composition
- [Overview](06-ability-composition/README.md)
- [The Composable Ability Model](06-ability-composition/ability-model.md)
- [Custom Ability Authoring & Templates](06-ability-composition/custom-ability-authoring.md)
- [Template Library](06-ability-composition/templates/README.md)

### 07 — Roadmap
- [Milestones & Dependency Order](07-roadmap/milestones.md)
- [Acceptance & FSV](07-roadmap/acceptance-and-fsv.md)

---

## 4. How to read this set

- **Engine implementers** start at [00-foundations](00-foundations/) then read specs in
  build order (01 → 05).
- **Ability/map authors and AI agents** start at [06](06-ability-composition/) and use
  the templates; dip into 01–05 only when a template needs a new primitive parameter.
- **Reviewers** use each `test-plan.md` as the acceptance checklist and
  [07-roadmap/acceptance-and-fsv.md](07-roadmap/acceptance-and-fsv.md) for the gate.

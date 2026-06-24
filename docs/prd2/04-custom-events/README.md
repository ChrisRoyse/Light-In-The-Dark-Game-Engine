# 04 — Script-Defined Custom Events

> **Requirement namespace:** `R-EVT-*`
> **Extends:** the fixed `EventKind` registry (`litd/api/event_payload.go:15-18`,
> `litd/sim/events.go`)
> **Primary tick phase:** phase 6 (events) — same ring and ordering as built-in events
> **New sub-hash:** `"customevents"`
> **New caps:** `Caps.CustomEventKinds` (default 256)

---

## Problem

The tutorial corpus teaches decoupled message passing as a core skill — the "service
bell" metaphor recurs across at least four videos. A producer "rings a bell" (emits a
named event); decoupled consumers react when they "hear" it. This is the spine of:
- **State machines** (boss sends `sleep`/`battle`/`transform`/`death`; the boss's per-state
  handlers react).
- **Behavior trees** (dispatcher sends `AI-guard` / `AI-healer`; unit-type handlers run
  their branch).
- **Equipment effects** (pickup sends a custom event; a special-effect handler renders the
  buff).
- Any "when X happens anywhere, anyone interested reacts" pattern.

Our engine has a **fixed** `EventKind` enum (42 hardcoded kinds) and `OnEvent` **fails
closed on unknown kinds** (`api/events.go:90-93`). There is no way for a script to define a
new event type. Authors fake it with shared Lua flags polled in timers — coupled, racy,
and nondeterministic.

## Solution

Let scripts **register a named custom event kind** at world setup, receiving a stable
`EventKindID` that lives in the *same ID space* and flows through the *same dispatch path*
as built-in events. Emitting and subscribing are identical to built-in events; a handler
cannot tell a custom kind from a built-in one except by id. The name→id registry is
serialized so subscriptions survive save/load. Rich parameters ride in an optional KV bag
so the hot path stays scalar.

## Design highlights

- **One ID space, one dispatch path (R-EVT-6).** Custom kinds are just ids ≥ the first
  custom id. `Emit` and the per-kind subscription lists work unchanged. Determinism
  (emission order × registration order, overflow-drops) is inherited verbatim from the
  existing `events.go` machinery.
- **Serializable registry (R-EVT-2).** The name→id table is part of saved state. A handler
  subscribed to `"boss.transform"` re-binds to the same id on load.
- **Scalar-fast, KV-rich (R-EVT-4).** The payload keeps the existing `(Src, Dst, Arg
  int64)` shape; for more than one scalar, the emitter attaches a KV bag handle
  ([03](../03-keyvalue-store/)) or a group handle ([02](../02-unit-groups/)) in `Arg`.
- **Wait-able (R-EVT-8).** The scheduler's `WaitEvent` extends to custom kinds, so a Lua
  coroutine can `WaitForEvent("boss.transform")` and resume deterministically.

## What this unlocks

- Drop-in state machines and behavior trees without polling.
- The [ability](../06-ability-composition/) layer emits `"ability.impact"`,
  `"mover.complete"`, etc., that map logic can hook without touching engine code.
- Cross-system choreography (a kill emits `"objective.progress"`; the quest system and the
  UI both react).

## Documents

- [spec.md](spec.md) — registry, id space, dispatch, payload, waiting, serialization.
- [api.md](api.md) — Go API and Lua binding.
- [test-plan.md](test-plan.md) — determinism, ordering, save/load, FSV.

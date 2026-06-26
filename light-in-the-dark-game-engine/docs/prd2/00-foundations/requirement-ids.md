# Requirement-ID Namespaces

> PRD2 introduces six new requirement-ID namespaces. They follow the master PRD's
> convention (`R-<AREA>-<n>`) and are referenced from each subsystem spec and from the
> acceptance gate. IDs are stable once published; add new IDs by incrementing, never by
> renumbering.

These namespaces are **additive** to the master PRD's `R-SIM-*`, `R-RND-*`, `R-GC-*`,
`R-EXEC-*`, `R-FSV-*`, `R-API-*`. Where a PRD2 requirement strengthens or overrides an
existing one, it says so explicitly and cites the superseded ID.

---

## R-TMR-* — Timer Wheel ([01](../01-timer-wheel/))

| ID | Requirement |
|----|-------------|
| R-TMR-1 | Three timer modes exist: **single** (fire once after N ticks), **loop** (fire every N ticks until cancelled), **count** (fire every N ticks exactly K times). |
| R-TMR-2 | Timer state is fully serializable: no Go closures in persisted state. A timer references a **continuation by stable ID + a value payload `[4]int64`**, never a captured closure. Closes #270. |
| R-TMR-3 | Timer wake order is deterministic: ordered by `(wakeTick, sequence)`; ties broken by monotonic allocation sequence. |
| R-TMR-4 | Timer resolution is the sim tick (50 ms / 20 Hz); sub-tick durations quantize **up** with a one-tick floor (matches `api/timer.go`). |
| R-TMR-5 | Timers are a fixed pool (`Caps.Timers`); exhaustion returns an invalid `TimerID` + drop counter; zero alloc, no panic. |
| R-TMR-6 | A timer carries an optional **owner `EntityID`**; when the owner dies, the timer is auto-cancelled in the cleanup phase (prevents leaks in spawn/ability code). |
| R-TMR-7 | The timer sub-hash is registered in `HashSystems` as `"timers"` and mirrors its save format field-for-field. |
| R-TMR-8 | `Game.After/Every` are re-expressed on top of the wheel; the closure form is retained only for transient, non-persisted convenience and is documented as save-unsafe. |

## R-UGR-* — Unit Groups ([02](../02-unit-groups/))

| ID | Requirement |
|----|-------------|
| R-UGR-1 | A **persistent group** is a durable, handle-addressed ordered set of `EntityID`. |
| R-UGR-2 | Membership is unique (no duplicate entity in one group) and **insertion-ordered**; iteration order is the insertion order and is serialized. |
| R-UGR-3 | Dead members are auto-pruned in the cleanup phase; iteration never yields a stale handle. |
| R-UGR-4 | Group operations are O(1) add (with membership check), O(1) swap-remove, O(n) ordered iterate; no allocation per op at steady state. |
| R-UGR-5 | Set algebra is provided: union, intersect, difference, clear, copy — all into a destination group, zero-alloc. |
| R-UGR-6 | Query-fill operations populate a group from a spatial/predicate query (radius, rect, owner, type) deterministically. |
| R-UGR-7 | Groups are a fixed pool (`Caps.UnitGroups`) backed by a shared membership arena (`Caps.GroupMembers`); exhaustion ⇒ invalid handle / dropped member + counter. |
| R-UGR-8 | Sub-hash `"unitgroups"` registered; canonical order = (group slot order, member insertion order). |

## R-KV-* — Generic Typed Key-Value Store ([03](../03-keyvalue-store/))

| ID | Requirement |
|----|-------------|
| R-KV-1 | A key-value pair attaches to any sim object identified by an `EntityID` (unit, item, destructible, region, projectile) **and** to global/player scopes. |
| R-KV-2 | A value is a **tagged union** over the sim's primitive types: `int64`, `fixed.F64`, `bool`, interned `string`, `EntityID`, `Vec2`, `GroupID`, `TimerID`. No arbitrary Go interface in hashed state. |
| R-KV-3 | Keys are **interned strings** (stable integer IDs); the intern table is serialized so key IDs are stable across save/load. |
| R-KV-4 | Read of an absent key returns the type's zero value and an `ok=false`; write upserts. Both are O(log n) via binary search over sorted `(owner,key)` pairs (no Go map in hashed state). |
| R-KV-5 | Supersedes `UserDataStore`: the legacy int32 `GetUnitUserData/SetUnitUserData` API is re-expressed as a KV pair under a reserved key, preserving back-compat. |
| R-KV-6 | KV pairs are a fixed pool (`Caps.KVPairs`); exhaustion ⇒ write fails + drop counter. |
| R-KV-7 | Sub-hash `"kv"` registered; canonical order = ascending `(ownerEntity, keyID)`. |
| R-KV-8 | Pairs whose owner entity dies are pruned in cleanup (unless under the global/player scope). |

## R-EVT-* — Script-Defined Custom Events ([04](../04-custom-events/))

| ID | Requirement |
|----|-------------|
| R-EVT-1 | Scripts may **register a named custom event kind** at world setup, receiving a stable `EventKindID`. |
| R-EVT-2 | The custom-kind registry is serialized (name → ID) so handler subscriptions survive save/load (R-SIM-6). |
| R-EVT-3 | Custom events dispatch through the **existing emission ring and ordering** (`events.go`): emission order × per-kind registration order; overflow drops deterministically. |
| R-EVT-4 | A custom event carries the standard payload (`Src`, `Dst`, `Arg int64`) plus an optional **KV bag** (a `GroupID`/`KVPairs` handle) for richer parameters, keeping the hot path scalar. |
| R-EVT-5 | Registration is bounded (`Caps.CustomEventKinds`); re-registering an existing name returns the existing ID (idempotent). |
| R-EVT-6 | Built-in `EventKind`s and custom kinds share one ID space and one dispatch path; a handler cannot tell them apart except by kind ID. |
| R-EVT-7 | Sub-hash `"customevents"` registers the name→ID table and the per-kind subscription lists in canonical (ascending kind) order. |
| R-EVT-8 | Scripts may both **emit** and **wait on** custom kinds (the scheduler `WaitEvent` path extends to custom kinds). |

## R-MOV-* — Unified Parametric Movers ([05](../05-movers/))

| ID | Requirement |
|----|-------------|
| R-MOV-1 | A **mover** is a serializable motion controller that drives the transform of *either a unit or a missile/projectile* through a deterministic, fixed-point parametric path. |
| R-MOV-2 | Mover kinds at minimum: `Linear`, `Homing`, `Point`, `Orbit` (unit-anchored & point-anchored), `Arc/Ballistic`, `Spline` (waypoint Catmull-Rom), `Custom` (script step callback via continuation ID). |
| R-MOV-3 | All mover math is fixed-point (`fixed.F64`, `fixed.Vec2`, `fixed.Angle`); no float; collision distance tests use exact 128-bit `DistSq`/`RadiusSq`. |
| R-MOV-4 | A mover may carry a **collision policy** (hit mask, pierce count, radius, decay) and on collision execute an **effect payload** (`data.EffectList`) and/or emit an event — this is what makes projectile abilities composable. |
| R-MOV-5 | A mover has a **completion policy**: expire, loop, detonate, or invoke a completion continuation; completion is deterministic and serializable. |
| R-MOV-6 | Movers **supersede** the straight-line-only missile motion: the existing `MissileStore` flight becomes the `Linear`/`Homing`/`Point` mover kinds; no projectile bypasses the mover system. |
| R-MOV-7 | Movers may drive units (orbiting guardians, knockback, dashes, charges) under a **movement-authority flag** that suspends normal pathing while the mover owns the transform. |
| R-MOV-8 | Movers are a fixed pool (`Caps.Movers`) shared by units and missiles; exhaustion ⇒ invalid `MoverID` + drop counter. |
| R-MOV-9 | Sub-hash `"movers"` registered; the missile sub-hash is folded in or retired so motion hashes once. |
| R-MOV-10 | A mover with an owner auto-cancels when the owner dies (units) or is consumed on impact/expiry (missiles). |

## R-ABL-* — Ability & Character Composition ([06](../06-ability-composition/))

| ID | Requirement |
|----|-------------|
| R-ABL-1 | An ability is authored as a **declarative spec** that composes primitives 1–5: cast → (optional) spawn projectile → attach mover → on-collision run effects / emit event → on-complete cleanup. |
| R-ABL-2 | Ability specs are **data**, not code: serializable, hashable, diffable, and shippable as a template file droppable into a map. |
| R-ABL-3 | The engine ships a **template library** of ready abilities (orbital strike, homing bolt, nova ring, chain bounce, boomerang, persistent field) usable as-is or as an edit base. |
| R-ABL-4 | A character/unit references abilities by ID; granting an ability is data-only and requires no engine change. |
| R-ABL-5 | Authoring is reachable from both Go and Lua with identical results; an AI agent can produce a working new ability by editing a template's parameters. |
| R-ABL-6 | Ability execution introduces **no new sim state** beyond the spec table and the primitives it instantiates; determinism and zero-alloc follow from 1–5. |

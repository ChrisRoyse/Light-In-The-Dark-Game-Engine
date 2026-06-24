# Glossary

Shared vocabulary for PRD2. Where a term already has a precise meaning in the master PRD
or the codebase, this restates it for convenience and links the canonical source.

| Term | Definition |
|------|------------|
| **Tick** | The fixed 50 ms (20 Hz) simulation step. All timing in PRD2 is measured in ticks. See `litd/sim/step.go`. |
| **EntityID** | 32-bit packed handle `[generation:8 | index:24]` identifying any sim object. Stale (generation-mismatched) handles resolve to a no-op. `litd/sim/entity.go`. |
| **SoA store** | Structure-of-Arrays component store: parallel column slices, a `rowOf` sparse index, a `count` of contiguous live rows, swap-remove on deletion. The standard sim storage shape. |
| **Sub-hash** | A named per-system contribution to the global determinism fingerprint, registered in `HashSystems` (`litd/sim/hash.go`). Localizes divergence to one system. |
| **Canonical order** | An iteration order that is a pure function of state and identical on all machines / across save-load. Never Go-map order. |
| **Continuation ID (`ContID`)** | A stable integer naming a resumable function registered with the scheduler. Used instead of a Go closure so wake records serialize. `litd/sim/sched/sched.go`. |
| **State payload (`[4]int64`)** | The value-typed, serializable argument bundle carried by a suspended continuation or a timer. No pointers, no closures. |
| **Timer** | A serializable, handle-addressed delay that fires a continuation after N ticks, optionally repeating (loop) or repeating K times (count). PRD2 [01](../01-timer-wheel/). |
| **Timer wheel** | The data structure backing timers: a deterministic min-ordered schedule keyed on `(wakeTick, seq)`, drained per tick. |
| **Unit group / `GroupID`** | A durable, ordered, duplicate-free set of `EntityID` with set algebra and query-fill. PRD2 [02](../02-unit-groups/). The WC3 "group" type. |
| **KV pair** | A `(owner, key, value)` triple where value is a tagged union over sim primitives. Generic per-object custom attributes. PRD2 [03](../03-keyvalue-store/). |
| **Interned string** | A string stored once in a serialized intern table and referenced by a stable integer ID, so string keys/values hash and serialize as integers. |
| **Custom event kind / `EventKindID`** | A script-registered event type sharing the built-in event dispatch path. PRD2 [04](../04-custom-events/). |
| **Service-bell pattern** | The pub/sub idiom taught in the tutorial corpus: a producer "rings a bell" (emits an event); decoupled consumers react on receipt. Realized by custom events. |
| **Mover / `MoverID`** | A serializable parametric motion controller that owns the transform of a unit or projectile and advances it deterministically each tick. PRD2 [05](../05-movers/). |
| **Movement authority** | The flag indicating a mover currently owns a unit's transform, suspending normal pathing for the duration (dashes, knockbacks, orbits). |
| **Collision policy** | A mover's hit mask + radius + pierce + decay describing what it can hit and how many times. |
| **Effect payload (`data.EffectList`)** | A compiled list of effect primitives (damage/heal/buff/area/chain) run at a context (source/target/point). Shared by abilities and triggers. `litd/sim/effect.go`. |
| **Completion policy** | What a mover does when its path ends: expire, loop, detonate, or invoke a completion continuation. |
| **Ability spec** | A declarative, serializable description of an ability as a composition of primitives 1–5. PRD2 [06](../06-ability-composition/). |
| **Template** | A shippable, drag-and-drop ability spec file usable as-is or as an edit base. |
| **Drop counter** | A deterministic, hashed counter incremented when a bounded pool refuses an operation (instead of allocating/panicking). Mirrors the missile-ring and damage-buffer posture. |
| **R-GC-1 / R-GC-2** | Zero-alloc steady state / fixed capacity no mid-match growth. Master PRD. |
| **R-SIM-6** | Serialization + hashing ordering discipline. Master PRD. |
| **R-EXEC-1** | Script logic runs on the deterministic cooperative scheduler, never free goroutines. Master PRD. |
| **#270** | The tracking issue for "Go-closure timers / scheduler wake records not serializable." PRD2 [01](../01-timer-wheel/) closes it. |

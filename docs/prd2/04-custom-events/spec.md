# Custom Events — Specification

## 1. Background: the existing event machinery (unchanged)

From the audit (`litd/sim/events.go`):

```go
type Event struct { Kind uint16; Src, Dst EntityID; Arg int64 }
type kindSubs struct { kind uint16; list []HandlerID } // registration order = dispatch order
```

- `Subscribe(kind, handler)` appends to the per-kind list (binary-searched, kind-sorted).
- `Emit(ev)` queues into a fixed ~4,097-event ring; overflow drops deterministically.
- Dispatch (phase 6) iterates queued events in emission order; per event, per-kind handlers
  run in registration order. Handlers emitting mid-flush land later in the same ring.
- `SubsSnapshot()` yields the canonical (ascending kind, registration-ordered handlers)
  form for save/hash.

PRD2 **does not change any of this.** It only adds a way to mint new `kind` values.

## 2. The kind id space (R-EVT-6)

```
0                                  reserved (no-kind)
1 .. KBuiltinMax                   built-in EventKinds (the existing iota block)
KBuiltinMax+1 .. KBuiltinMax+Caps.CustomEventKinds   custom, script-registered
```

A `uint16` kind is enough (existing `Event.Kind` is already `uint16`). Custom ids are
assigned sequentially from `KBuiltinMax+1` as names are registered.

## 3. The custom-kind registry

```go
// litd/sim/customevent.go (new)

type CustomEventRegistry struct {
    // name → id, kept sorted by name id for canonical iteration. Interns names like the KV store.
    names    internTable   // name string → uint32 nameId (serialized)
    nameToKind []uint16    // indexed by nameId → kind (KBuiltinMax+1 ..); -1 if unregistered
    nextKind uint16        // next custom id to assign
    count    uint16        // registered custom kinds
}
```

- **`RegisterEventKind(name) EventKindID`** (R-EVT-1): interns `name`; if already
  registered, returns the existing id (**idempotent**, R-EVT-5); else assigns `nextKind++`
  and records the mapping. Returns invalid id (0) and increments `customEventDropped` if
  `count == Caps.CustomEventKinds`.
- Registration happens at **world setup** (deterministic, before the first tick) in the
  common case; mid-match registration is allowed but discouraged (it must be driven by
  deterministic sim logic, never wall-clock/UI).

## 4. Dispatch (R-EVT-3)

Identical to built-in events. To emit a custom kind:

```go
w.Emit(Event{Kind: kindID, Src: src, Dst: dst, Arg: arg})
```

It enters the same ring, obeys the same ordering and overflow-drop rules. Subscribers use
the same `Subscribe(kindID, handler)`. A handler receives the same `Event`/`EventView` and
cannot distinguish a custom kind from a built-in one except by inspecting `Kind`.

## 5. Payload: scalar-fast, KV-rich (R-EVT-4)

The hot path stays `(Src, Dst, Arg int64)` — three machine words, zero alloc. For richer
parameters the emitter chooses one of:

- **`Arg` as a scalar** (a damage amount, an enum, a count).
- **`Arg` as a `GroupID`** — pass a set of units (e.g. `"wave.spawned"` carrying the wave
  group). The handler reads the group ([02](../02-unit-groups/)).
- **`Arg` as a KV-bag owner handle** — the emitter sets several KV pairs under a transient
  entity/global key and passes that owner in `Arg`; the handler reads them
  ([03](../03-keyvalue-store/)). Used when an event needs many named parameters.

This keeps the common case allocation-free while still supporting arbitrary structured
payloads, without ever putting a variable-size struct in the event ring.

## 6. Waiting on custom kinds (R-EVT-8)

The scheduler already supports `WaitEvent(EventID, cont, state)` and the Lua
`WaitForEvent(kind)` yield (`luabind/sched.go:223-252`). PRD2 widens the accepted kind
range to include custom ids. A coroutine waiting on `"boss.transform"` is parked on that
kind and resumes, in FIFO `seq` order among waiters, when the kind next fires —
deterministically.

## 7. Serialization & hashing (R-EVT-2, R-EVT-7)

Two things serialize:
1. **The custom-kind registry** — the name intern table and `nameToKind`, in name-id order.
   This makes saved subscriptions (which reference kind ids) meaningful on load.
2. **The subscription tables** — already serialized today via `SubsSnapshot()`; custom
   kinds simply appear as additional entries in the canonical (ascending kind) snapshot.

Sub-hash `"customevents"`: write `count`, `customEventDropped`, `nextKind`; the name intern
table in id order; `nameToKind` in name-id order. The subscription lists continue to be
hashed by the existing events sub-hash (or are moved under `"customevents"` if the audit
shows they aren't yet hashed — TBD against `events.go` at implementation time).

> **Determinism caveat for mid-match registration.** Because custom ids are assigned in
> registration order, two runs must register the same names in the same order to get the
> same ids. Registering all custom kinds at world setup (the recommended pattern)
> guarantees this. A late, conditionally-registered kind whose registration depends on
> nondeterministic input would diverge — the lint flags registration outside setup.

## 8. Capacity & exhaustion (R-EVT-5)

- `Caps.CustomEventKinds` (256) bounds the number of distinct custom kinds.
- Re-registering an existing name is free and returns the existing id.
- Exhaustion ⇒ invalid id + `customEventDropped++`. Emitting/subscribing on an invalid id is
  a no-op.

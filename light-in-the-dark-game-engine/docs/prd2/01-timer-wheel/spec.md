# Timer Wheel — Specification

## 1. Data model

```go
// litd/sim/timer.go  (new)

type TimerID uint32 // [ generation:8 | index:24 ]; stale ⇒ no-op (R-API-5)

type TimerMode uint8
const (
    TimerSingle TimerMode = iota // fire once, then free
    TimerLoop                    // fire every Interval forever until cancelled
    TimerCount                   // fire every Interval exactly Remaining times, then free
)

// TimerStore is an SoA pool (00-foundations/architecture-principles.md §1).
type TimerStore struct {
    // --- columns ---
    Mode      []uint8       // TimerMode
    Interval  []uint32      // ticks between fires (>=1; 0 quantized to 1)
    WakeTick  []uint32      // absolute sim tick of next fire
    Remaining []uint32      // TimerCount: fires left; else unused
    Seq       []uint32      // monotonic allocation sequence (tie-break)
    Cont      []uint16      // ContID — stable continuation, NOT a closure (R-TMR-2)
    State     [][4]int64    // value payload passed to the continuation
    Owner     []EntityID    // optional; 0 = unowned. Auto-cancel on owner death (R-TMR-6)
    Gen       []uint8       // generation for the slot (handle validation)
    live      []bool        // slot occupied?

    free  []int32           // free-list (LIFO); serialized for slot-stable reload
    count int32             // live timer count (for hashing/iteration convenience)
    nextSeq uint32          // monotonic; never reset within a match

    // schedule index: a min-ordered structure keyed on (WakeTick, Seq).
    // Implemented as a 4-byte-bucketed hierarchical timing wheel for O(1) amortized
    // insert/advance, with a tie-break heap per bucket. See §4.
    wheel timerWheel
}
```

### Field notes
- `Cont` is a `ContID` registered with the scheduler at world setup
  (`litd/sim/sched`), exactly like every other resumable function. This is the crux of
  R-TMR-2: the timer names *what to run* by a stable integer, and carries *the data it
  needs* in `State [4]int64`. Both serialize trivially.
- `State [4]int64` mirrors the scheduler's existing `State` type
  (`sched.go:22-25`). Four int64 slots is enough to carry, e.g., an owner `EntityID`, an
  ability/effect-list ref, a target `EntityID`, and a scalar — the common ability/spawn
  payload. Richer payloads use a KV bag handle (one int64) → see [03](../03-keyvalue-store/).
- `Owner` enables the spawn/ability leak-prevention rule: when the owner entity dies, the
  cleanup phase frees the timer so a dead spawner stops spawning.

## 2. Lifecycle

```
Create ─► (scheduled in wheel at WakeTick) ─► Fire(s) ─► Free
              ▲                                   │
              └────── reschedule (loop/count) ◄───┘
```

1. **Create.** Allocate a slot from `free` (LIFO). Assign `Gen` (incremented on reuse),
   `Seq = nextSeq++`, compute `WakeTick = now + max(1, Interval)`, store `Cont`,
   `State`, `Owner`, `Mode`, `Remaining`. Insert into the wheel. Return
   `TimerID(gen,index)`. If `free` is empty ⇒ return invalid `TimerID(0)` and increment
   `timerDropped` (R-TMR-5). **Zero alloc.**
2. **Fire.** During phase-2 drain, every timer with `WakeTick == now` fires in
   `(WakeTick, Seq)` order: the scheduler runs `Cont(State)`. Firing a timer may itself
   create timers; those are scheduled for a strictly later tick (`max(1, …)` floor), so
   the drain terminates (matches the scheduler's "suspensions pushed during drain wake
   next tick" rule, `sched.go:189-195`).
3. **Reschedule / Free.**
   - `TimerSingle`: free the slot after firing.
   - `TimerLoop`: `WakeTick += Interval`; re-insert.
   - `TimerCount`: `Remaining--`; if `Remaining == 0` free, else `WakeTick += Interval`
     and re-insert.
4. **Cancel.** `CancelTimer(id)` validates generation, removes from the wheel, frees the
   slot, bumps `Gen`. Idempotent: cancelling an already-free/stale timer is a no-op.
5. **Auto-cancel.** In phase 7 (cleanup), for each entity that died this tick, any owned
   timers are cancelled. Implemented by scanning the dead-entity list against
   `Owner` via the `rowOf`-style reverse index (no map iteration).

## 3. Determinism & ordering (R-TMR-3)

The only nondeterminism risk is fire order among timers waking on the same tick. Resolved
by the total order `(WakeTick, Seq)` with `Seq` a monotonic allocation counter that never
repeats within a match. This is identical in spirit to the scheduler's `recordLess`
(`sched.go:221-226`). Because `Seq` is assigned at creation and creation order is itself
deterministic (driven by deterministic sim logic), fire order is fully determined.

`nextSeq` is **part of saved state** so that post-load creations continue the sequence
without collision.

## 4. The wheel structure

For scale (thousands of timers, mostly short intervals) a flat heap is acceptable but a
**hierarchical timing wheel** is preferred for O(1) amortized insert/advance:

- A small ring of buckets (e.g. 256) at tick granularity for near-future timers.
- Coarser rings for far-future timers, cascaded down on rollover.
- Within a bucket, a `(Seq)`-ordered intrusive list gives the tie-break order.

The wheel is a **derived index**, not primary state: on load it is rebuilt by re-inserting
every live timer in slot order. Therefore the wheel itself is *not* serialized or hashed —
only the timer columns are (see [serialization-and-hashing.md](serialization-and-hashing.md)).
This keeps the hash independent of the index implementation, so the wheel can be tuned
without changing the fingerprint.

> **Implementation latitude.** A first cut MAY ship a binary min-heap keyed on
> `(WakeTick, Seq)` (simplest, obviously correct) and migrate to the bucketed wheel later;
> because the index is non-hashed, this migration cannot change determinism.
>
> **Performance note (research-backed).** At our scale (`Caps.Timers = 4,096`) the heap is
> not merely acceptable — it is likely *faster* than a wheel. Benchmarks show a binary heap
> beats a timing wheel on insertion below ~10K timers (the heap's contiguous backing
> prefetches into L1; the wheel writes random slab slots → cache misses), with the wheel only
> winning clearly at 100K+ and mainly on O(1) cancellation. We already get cheap cancellation
> from slot-indexed handles (tombstone + lazy skip), so **ship the heap; adopt a power-of-2
> bucketed wheel only if a profile shows timer counts routinely exceeding ~10K.** Full
> analysis: [../00-foundations/performance-budget.md §3](../00-foundations/performance-budget.md#3--timers--heap-vs-wheel-decided-by-our-scale).

## 5. Tick integration

Phase 2 (`phaseScripts`) already drains the scheduler. The timer drain is invoked at the
same point and shares the scheduler's continuation registry:

```
phaseScripts(now):
    timers.advance(now)      // fire all timers with WakeTick == now, in (WakeTick,Seq) order
    scheduler.drain(now)     // existing coroutine/continuation drain
```

Co-locating timer fire with the scheduler drain means there is **one** wake-ordering
authority, eliminating cross-system ordering ambiguity. (A timer's continuation may resume
a Lua coroutine, so the two must share an order anyway.)

## 6. Relationship to `Game.After/Every` (R-TMR-8)

`Game.After(d, fn)` and `Game.Every(d, fn)` remain for ergonomics but are reclassified:

- They allocate a timer whose `Cont` points at a **closure-trampoline continuation** that
  looks up the Go closure in a transient, *non-serialized* side table.
- A world saved while such a timer is pending will, on load, **drop** the closure timer and
  log it (deterministically — the drop is recorded). This is the documented, narrow
  save-unsafe path, acceptable only for transient UI/debug use.
- The **serializable** path is `Game.AfterCont(d, contID, payload)` /
  `Game.LoopCont(...)` / `Game.CountCont(...)`, which gameplay, ability, and mover code
  MUST use. The ability/template layer never uses the closure form.

## 7. Capacity & exhaustion (R-TMR-5)

- `Caps.Timers` default 4,096 (tunable per world). Sized once at `NewWorld`.
- Exhaustion: `CreateTimer*` returns `TimerID(0)` and increments the hashed `timerDropped`
  counter. Callers treat an invalid ID as "timer not created" (e.g., an ability that could
  not schedule its teardown falls back to immediate cleanup).
- No `append`-growth, no per-create allocation. The `State [4]int64` and all columns are
  preallocated.

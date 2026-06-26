# Timer Wheel — Serialization & Hashing

> Implements R-TMR-2 and R-TMR-7. The governing rule: **what is hashed == what is saved**,
> in struct-declaration field order, with no Go closures anywhere in persisted state.

---

## 1. The #270 migration in one picture

```
BEFORE (#270 defect)                       AFTER (PRD2)
────────────────────                       ────────────
timer ─► Go closure  ✗ not serializable    timer ─► ContID (uint16)   ✓ serializes
         captures fn, vars                          + State [4]int64   ✓ serializes
                                                    (continuation looked up in the
                                                     registry rebuilt at world setup)
```

A continuation is registered **once at world construction** by Go code (the same way the
scheduler registers its continuations today). The registry maps `ContID → func(*World,
State)`. Saving a timer saves its `ContID` and `State`; loading re-binds `ContID` against
the freshly built registry. No function pointer ever touches the save file.

This is exactly the posture the scheduler already documents for coroutine wakes
(`luabind/sched.go:25-29`); PRD2 generalizes it to standalone timers, which is the half
#270 left open.

## 2. Serialized form

Timers serialize as a block within the sim save (`litd/sim/save.go`), after the scheduler
block (so continuation IDs are valid when timers load). Little-endian, fixed width, field
order = struct declaration order.

```
TIMER BLOCK
  u32  count                 // number of LIVE timers
  u32  nextSeq               // monotonic sequence cursor (must persist, R-TMR-3)
  u32  timerDropped          // hashed drop counter
  repeat count times, in slot-ascending order of live slots:
    u32  packedHandle        // (Gen<<24 | index) — lets load restore exact slot
    u8   Mode
    u32  Interval
    u32  WakeTick
    u32  Remaining
    u32  Seq
    u16  Cont
    i64  State[0]
    i64  State[1]
    i64  State[2]
    i64  State[3]
    u32  Owner                // EntityID
  FREE-LIST
    u32  freeLen
    repeat freeLen: u32 slotIndex   // LIFO order preserved (slot-stable reload, R-SIM-6)
```

### Load procedure
1. Allocate the store at `Caps.Timers` (from the save header caps).
2. For each serialized live timer, write its columns into the slot named by
   `packedHandle.index`, set `live[index]=true`, `Gen[index]=packedHandle.gen`.
3. Restore `free` exactly (LIFO order) and `nextSeq`, `timerDropped`.
4. Rebuild the wheel index by inserting every live timer in ascending slot order
   (deterministic; the wheel is non-hashed, §4 of the spec).
5. Re-bind each `Cont` against the continuation registry; an unknown `ContID` (e.g. a
   dropped closure-trampoline) is logged and the timer is freed deterministically.

## 3. Hash fold (R-TMR-7)

Registered in `HashSystems` as `"timers"`, appended in the fixed PRD2 order
(`timers, unitgroups, kv, customevents, movers`). The sub-hash mirrors the save block,
minus the redundant free-list bytes that the canonical iteration already implies:

```go
// litd/sim/hash.go (addition)
ht := h.next() // "timers"
ts := w.Timers
ht.WriteU32(uint32(ts.count))
ht.WriteU32(ts.nextSeq)
ht.WriteU32(ts.timerDropped)
// iterate LIVE timers in ascending slot order (canonical, not wheel order):
for idx := int32(0); idx < int32(len(ts.live)); idx++ {
    if !ts.live[idx] { continue }
    ht.WriteU32(uint32(ts.Gen[idx])<<24 | uint32(idx))
    ht.WriteU8(ts.Mode[idx])
    ht.WriteU32(ts.Interval[idx])
    ht.WriteU32(ts.WakeTick[idx])
    ht.WriteU32(ts.Remaining[idx])
    ht.WriteU32(ts.Seq[idx])
    ht.WriteU16(ts.Cont[idx])
    for k := 0; k < 4; k++ { ht.WriteI64(ts.State[idx][k]) }
    ht.WriteU32(uint32(ts.Owner[idx]))
}
ht.WriteU32(uint32(len(ts.free)))
for _, s := range ts.free { ht.WriteU32(uint32(s)) }
```

### Why slot-ascending, not wheel order
Hashing in slot order makes the fingerprint **independent of the wheel implementation**
(§4 of the spec). Two engines that schedule identical timers but use different index
structures (heap vs. bucketed wheel) MUST produce the same hash. Slot order is a pure
function of allocation history, which is itself deterministic.

### Free-list inclusion
The free-list LIFO order is hashed because it steers *future* slot assignments, and two
diverging free-lists would silently produce divergent handles on the next create. This
mirrors how the missile/buff pools hash their free-lists.

## 4. Invariants verified by tests

- **Mirror invariant:** a property test asserts the byte stream produced by the save
  serializer equals the byte stream fed to the hasher for the same store (modulo the
  intentional free-list-only bytes), guarding against field-order drift.
- **Round-trip identity:** `hash(save → load) == hash(original)` for any timer
  population, including mid-fire states.
- **Continuation re-bind:** every `ContID` used by gameplay/ability code is present in the
  registry on load; a test enumerates registered IDs and fails if an ability template
  references an unregistered one.

## 5. Author rule — capture refs, not handles, across a tick (#663/#667)

The same "no Go closures in persisted state" discipline applies one level up, to **Lua**
script closures. A `Game_Every` / `Game_After` / `OnEvent` callback's captured upvalues
are serialized into the save (#464). Entity-backed handles — `Unit`, `Player` — marshal
fine (they round-trip through a stable `HandleRef`). **Non-entity-backed handles do not:**
an `Ability`, `Camera`, or raw `Timer` handle has no stable ref, so capturing one as a
closure upvalue makes `savegame.Write` fail at the marshal seam.

```lua
-- ✗ BAD: the Ability HANDLE is captured → save dies (#663).
local ab = Unit_AddAbility(hero, smiteRef)
Game_Every(0.5, function() Unit_CastAbility(hero, ab, target) end)

-- ✓ GOOD: capture the REF (an int) + entity-backed handles; re-derive inside.
local smiteRef = Game_AbilityRef("warden_smite")   -- a plain int
Game_Every(0.5, function() Unit_CastAbilityRef(hero, smiteRef, target) end)
```

`Unit_CastAbilityRef(caster, ref, target)` (#667) folds the re-derive (idempotent
`Unit_AddAbility`) + cast into one verb so a script never holds an `Ability` handle. The
marshal seam now fails *loudly and actionably* when this rule is broken — the error names
the captured type and points at this pattern (see `litd/luabind/handle_marshal.go`,
regression `handle_marshal_dx_test.go`). Worked example: `worlds/firstclash/melee/vigil.lua`.

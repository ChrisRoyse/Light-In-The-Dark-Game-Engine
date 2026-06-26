# Triggers & Events — JASS → Go Mapping

> Part of the [JASS API mapping](README.md). Governing rules: PRD [§4.2 dedup D1–D5, §4.3 R-API-4, §4.4 R-EXEC-1/2](../../../PRD.md).

## Surface size (grep survey, 2026-06-11)

| Source | Approx. count | Notes |
|---|---|---|
| `common.j` natives | **~130** | trigger CRUD, ~40 `TriggerRegister*Event` natives, conditions/actions/boolexpr (`And`/`Or`/`Not`/`Condition`/`Filter`), event-response getters, `TriggerSleepAction` |
| `blizzard.j` BJs | **~80** | `TriggerRegister*BJ` location/preset wrappers, `TriggerRegisterAnyUnitEventBJ`, `ConditionalTriggerExecute`, `PolledWait` |

## Representative JASS signatures

```jass
native CreateTrigger                  takes nothing returns trigger
native TriggerRegisterTimerEvent      takes trigger whichTrigger, real timeout, boolean periodic returns event
native TriggerRegisterUnitEvent       takes trigger whichTrigger, unit whichUnit, unitevent whichEvent returns event
native TriggerAddCondition            takes trigger whichTrigger, boolexpr condition returns triggercondition
native TriggerAddAction               takes trigger whichTrigger, code actionFunc returns triggeraction
native TriggerSleepAction             takes real timeout returns nothing
native And                            takes boolexpr operandA, boolexpr operandB returns boolexpr
native DestroyBoolExpr                takes boolexpr e returns nothing

function TriggerRegisterAnyUnitEventBJ takes trigger trig, playerunitevent whichEvent returns nothing
function TriggerRegisterTimerEventPeriodic takes trigger trig, real timeout returns event
function ConditionalTriggerExecute takes trigger trig returns nothing
function PolledWait takes real duration returns nothing
```

## Canonical Go surface

Per **R-API-4**, the entire object zoo — `trigger`, `event`, `triggercondition`,
`triggeraction`, `boolexpr`, `conditionfunc`, `filterfunc` — collapses into one
subscription primitive plus Go closures:

```go
type Event struct{ /* typed payload union: kind + responses */ }
type Subscription struct{ /* cancellable registration */ }

func (g *Game) OnEvent(kind EventKind, h func(Event), opts ...EventOption) Subscription
// opts: WithUnit(u), WithPlayer(p), WithRegion(r), WithFilter(func(Event) bool), ...

func (s Subscription) Cancel()           // replaces DestroyTrigger / DisableTrigger
func (s Subscription) SetEnabled(b bool) // EnableTrigger/DisableTrigger

// Event responses become typed payload accessors (replaces ~50 GetTriggering*/GetEvent* natives):
func (e Event) Unit() Unit         // GetTriggerUnit
func (e Event) Killer() Unit       // GetKillingUnit
func (e Event) Player() Player     // GetTriggerPlayer
func (e Event) Damage() float64    // GetEventDamage
func (e Event) SpellTarget() Vec2  // GetSpellTargetX/Y/Loc
```

`TriggerSleepAction`/`PolledWait` survive as `helpers.Wait(d)` (D4) — a handler that
waits suspends onto the deterministic scheduler and resumes on a later tick (R-EXEC-2),
quantized to 50 ms ticks (R-EXEC-5).

## Dedup rules applied

| Rule | Application | Example |
|---|---|---|
| **D1** | passthrough register BJs dropped | `TriggerRegisterTimerExpireEventBJ` → `OnEvent(EventTimerExpired, ...)` |
| **D2** | preset registrations collapse onto options | `TriggerRegisterTimerEventPeriodic`/`...Single` → `OnEvent(..., Every(d))` / `Once(d)` |
| **D3** | `...Loc`/rect/region register variants → `Vec2`/value options | `TriggerRegisterEnterRectSimple` → `OnEvent(EventRegionEnter, WithRect(r))` |
| **D4** | real-logic BJs kept once in helpers | `PolledWait` → `helpers.Wait`; `TriggerRegisterAnyUnitEventBJ` (registers for all 16+ players) → built-in: omit `WithPlayer` and you get all players |
| **D5** | event-response getter families → typed `Event` payload accessors | `GetTriggerUnit`/`GetAttacker`/`GetDyingUnit` → `e.Unit()`, `e.Attacker()` per event kind |

`boolexpr` combinators (`And`/`Or`/`Not`) and `DestroyBoolExpr` are **tombstoned**: Go
closures compose with `&&`/`||` and are GC-managed. Filters must be pure (R-EXEC-2);
the API gives filters a read-only event view to enforce wait-free conditions.

## Subsystem dependencies

- **sim** (primary): event bus is core sim infrastructure — every other category emits through it. Handlers run synchronously at the emission point in deterministic registration order (R-EXEC-1/2). Zero-alloc event payloads (R-GC-3: value types, pooled).
- **render**: none directly; UI/input events arrive as commands into the sim tick, then publish as events.
- **asset**: none.

## Porting hazards

1. **Determinism is won or lost here.** Registration order must be stable (ordered slice, never Go map iteration — R-SIM-2); a handler registering another handler mid-dispatch must have defined semantics (joins next dispatch).
2. **Re-entrancy.** WC3 spawns a new "thread" per trigger firing; an event fired inside a handler nests. Cap nesting depth (WC3 had opcode limits) and document recursion semantics.
3. **Waits inside handlers** suspend the *handler coroutine*, not the sim. After resume, event payload must still be valid — payloads are value snapshots, not live pointers.
4. **`GetTriggeringTrigger` self-reference** patterns (trigger turning itself off) map to the `Subscription` returned at registration; closures capture it.
5. **Event-kind explosion**: ~90 distinct `unitevent`/`playerunitevent`/`gameevent` constants. The manifest (R-AST-4) must map each to an `EventKind` + payload schema; missing one silently drops capability — audit gate covers this.
6. **Damage-event mutation** (`BlzSetEventDamage`) means some "responses" are writable — `Event` needs a controlled mutable subset for damage-modification handlers.

# Custom Events — Public API & Lua Binding

---

## 1. Go API (`litd/api`)

```go
// EventKindID names an event kind (built-in or custom). Value type.
type EventKindID uint16

// Register at world setup (idempotent). Returns invalid id if the kind pool is full.
func (g *Game) RegisterEvent(name string) EventKindID

// Emit — scalar-fast forms
func (g *Game) Emit(kind EventKindID, src, dst Unit, arg int64)
func (g *Game) EmitGroup(kind EventKindID, src Unit, group Group)   // arg = group handle
func (g *Game) EmitBag(kind EventKindID, src Unit, bag KVBag)       // arg = KV-bag owner

// Subscribe — identical to built-in OnEvent
func (g *Game) On(kind EventKindID, fn func(ev Event)) Subscription

// Within a handler, custom payload accessors
func (ev Event) Kind() EventKindID
func (ev Event) Source() Unit
func (ev Event) Target() Unit
func (ev Event) Arg() int64
func (ev Event) Group() Group      // when emitted via EmitGroup
func (ev Event) Bag() KVBag        // when emitted via EmitBag
```

`KVBag` is a thin handle over a transient KV owner (a reserved global sub-namespace or a
short-lived marker entity) used to pass named parameters:

```go
bag := g.NewBag()
bag.SetInt("damage", 250)
bag.SetString("element", "fire")
g.EmitBag(kindAbilityImpact, caster, bag)   // handler reads ev.Bag().GetInt("damage")
```

## 2. Lua binding (`litd/luabind`)

This is the surface that makes the "service bell" pattern trivial for AI authors.

```lua
-- at setup: declare custom kinds (idempotent; returns a stable id)
local E_TRANSFORM = RegisterEvent("boss.transform")
local E_AI_GUARD  = RegisterEvent("ai.guard")
local E_IMPACT    = RegisterEvent("ability.impact")

-- subscribe
OnEvent(E_TRANSFORM, function(ev)
    SetUnitModel(EventSource(ev), "boss_phase2")
end)

-- emit (scalar-fast)
EmitEvent(E_TRANSFORM, boss)                       -- src only
EmitEvent(E_AI_GUARD, dispatcher, guardUnit)       -- src + dst

-- emit with a group payload
EmitGroupEvent(E_IMPACT, caster, hitGroup)
OnEvent(E_IMPACT, function(ev)
    GroupEach(EventGroup(ev), function(u) FlashUnit(u) end)
end)

-- emit with named params (KV bag)
local bag = NewBag()
SetKV(bag, "damage", 250)
SetKV(bag, "element", "fire")
EmitBagEvent(E_IMPACT, caster, bag)
OnEvent(E_IMPACT, function(ev)
    local b = EventBag(ev)
    DealTypedDamage(EventSource(ev), GetKV(b, "damage"), GetKV(b, "element"))
end)

-- wait on a custom kind from a coroutine (deterministic resume)
WaitForEvent(E_TRANSFORM)
```

> **Name strings, not magic numbers.** Authors register by readable name and keep the
> returned id in a local. The id is stable for the match and across save/load, so saved
> handler subscriptions re-bind correctly.

## 3. Mapping to WC3 / JASS

WC3 has no first-class user event type; the idiom is a **shared trigger fired manually**
(`TriggerExecute`, or a `udg_` boolean variable event `gg_trg_X`). PRD2 replaces those
fragile idioms:

| WC3 idiom | PRD2 |
|-----------|------|
| `udg_flag = true` then `TriggerEvaluate` on a variable-change event | `RegisterEvent("name")` + `EmitEvent`/`OnEvent` |
| `TriggerExecute(gg_trg_Boss)` to invoke another trigger | `EmitEvent(kind, …)` (decoupled, many subscribers) |
| Game cache / hashtable signaling | `EmitBagEvent` with a KV bag |

## 4. Worked patterns

### Boss state machine (tutorial: state-machine boss)
```lua
local E = {
    sleep = RegisterEvent("boss.sleep"),
    battle = RegisterEvent("boss.battle"),
    transform = RegisterEvent("boss.transform"),
    death = RegisterEvent("boss.death"),
}
OnEvent(E.sleep,     function(ev) BossEnterSleep(EventSource(ev)) end)
OnEvent(E.battle,    function(ev) BossEnterBattle(EventSource(ev)) end)
OnEvent(E.transform, function(ev) BossEnterTransform(EventSource(ev)) end)
OnEvent(E.death,     function(ev) BossEnterDeath(EventSource(ev)) end)

-- the per-state tick decides transitions and rings the right bell:
function BossTick(boss)
    if UnitLifeFraction(boss) < 0.5 and GetKV(boss, "phase") == 1 then
        SetKVInt(boss, "phase", 2)
        EmitEvent(E.transform, boss)
    end
end
```

### Behavior-tree dispatch (tutorial: BT AI)
```lua
local E_GUARD  = RegisterEvent("ai.guard")
local E_HEALER = RegisterEvent("ai.healer")
-- once per second, classify and dispatch
Every(1.0, function()
    GroupEach(aiTeam, function(u)
        if UnitType(u) == "guard"  then EmitEvent(E_GUARD,  dispatcher, u) end
        if UnitType(u) == "healer" then EmitEvent(E_HEALER, dispatcher, u) end
    end)
end)
OnEvent(E_GUARD,  function(ev) GuardBehavior(EventTarget(ev)) end)
OnEvent(E_HEALER, function(ev) HealerBehavior(EventTarget(ev)) end)
```

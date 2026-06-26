# Writing triggers (ECA)

**Audience:** world authors (human and AI) writing Lua against the engine's
sandbox (`litd/luabind`). **Status:** reference for the Trigger/ECA substrate
(ADRs #451/#452/#453, epic #454).

A *trigger* is the engine's unit of authored behaviour: an **E**vent that fires
it, optional **C**onditions that gate it, and **A**ctions that run when it fires.
It is the same Event-Condition-Action model as the WC3 World Editor. `OnEvent`
and the world's periodic callbacks are thin sugar over this substrate; everything
below runs on the deterministic cooperative scheduler (no free goroutines, all
randomness from the sim PRNG).

> Every fenced `lua` example on this page is executed against the real engine by
> `TestDocExamplesRunAndLintFSV` (#484). They are not illustrations — they run.

## 1. The shape of a trigger

```lua-norun
local t = CreateTrigger()                    -- a new, empty trigger
TriggerRegisterUnitEvent(t, someUnit, "death") -- E: what fires it
TriggerAddCondition(t, function(e) return true end) -- C: gate (all must pass)
TriggerAddAction(t, function(e) end)         -- A: what it does
```

- **Events** wire the trigger to the sim. `TriggerRegisterUnitEvent(t, unit,
  "death")` fires `t` when that unit dies; `TriggerRegisterPlayerUnitEvent`,
  `TriggerRegisterEnterRegion`, and `TriggerRegisterTimerEvent` cover the other
  families.
- **Conditions** are predicates. The trigger runs its actions only if *every*
  condition returns true. Compose And/Or/Not in plain Lua (§4).
- **Actions** are the effect. Inside an action the `e` argument is the event
  payload: `Event_Unit(e)`, `Event_Target(e)`, `Event_Source(e)`,
  `Event_Player(e)`, `Event_Damage(e)`, `Event_Buff(e)`, … (see §5).

## 2. A first trigger: count enemy deaths

A trigger registered on a unit's death, gated by a condition (the dier is P1's
enemy), incrementing a scoreboard counter in `Game_Storage`.

<!-- fsv doc.kills == 1 @1 -->
```lua
local store = Game_Storage()
local enemy = Game_CreateUnit(Game_Player(2), Game_UnitType("hfoo"), { x = 0, y = 0 }, 0)

local t = CreateTrigger()
TriggerRegisterUnitEvent(t, enemy, "death")
TriggerAddCondition(t, function(e)
	return Player_IsEnemy(Game_Player(1), Unit_Owner(Event_Unit(e)))
end)
TriggerAddAction(t, function(e)
	Storage_SetInt(store, "doc", "kills", Storage_GetInt(store, "doc", "kills") + 1)
end)

Unit_Kill(enemy) -- the death fires t; condition passes; counter -> 1
```

## 3. `OnEvent`: trigger sugar for global event kinds

When you want to react to *any* unit of a given event kind (not one registered
unit), `OnEvent(kind, fn)` subscribes a handler. `kind` is the numeric event kind
(table below). It is exactly a trigger with a global event and a single action.

```lua
local store = Game_Storage()
OnEvent(1, function(e) -- 1 = unit death
	Storage_SetInt(store, "doc", "anydeath", 1)
end)
local u = Game_CreateUnit(Game_Player(2), Game_UnitType("hfoo"), { x = 10, y = 10 }, 0)
Unit_Kill(u)
```

Common kinds (full list in [event-coverage.md](event-coverage.md)):

| kind | meaning |
|---|---|
| 1 | unit death |
| 2 | unit damaged |
| 17 | buff expired |
| 26 | ability cast |
| 27 | ability effect |
| 34 | buff applied |
| 35 | buff refreshed |

Reading a buff event's payload (kind 34 = buff applied):

<!-- fsv doc.applied == 1 @1 -->
```lua
local store = Game_Storage()
OnEvent(34, function(e) -- buff applied
	if not Event_FromAura(e) then
		Storage_SetInt(store, "doc", "applied", Event_BuffStacks(e))
	end
end)
local v = Game_CreateUnit(Game_Player(2), Game_UnitType("hfoo"), { x = 20, y = 0 }, 0)
Unit_ApplyBuff(v, Game_BuffType("burn"))
```

## 4. Composing conditions (And / Or / Not)

Conditions are ordinary Lua functions returning a boolean, so And/Or/Not is just
Lua. The example builds `A and (B or C)` and verifies all four truth cases land
the way the boolean algebra says.

<!-- fsv doc.composed == 1011 @0 -->
```lua
local store = Game_Storage()
local function gate(A, B, C) return A and (B or C) end
-- pack the 4 interesting rows into one int so the runner can check them at once:
-- want T,F,T,T for (T,T,F),(F,T,T),(T,F,T),(T,T,T)
local bits = ""
for _, row in ipairs({ {true,true,false}, {false,true,true}, {true,false,true}, {true,true,true} }) do
	bits = bits .. (gate(row[1], row[2], row[3]) and "1" or "0")
end
Storage_SetInt(store, "doc", "composed", tonumber(bits))
```

## 5. Initially-off triggers

A trigger can be created disabled and switched on later by another trigger —
the WC3 "Initially On" unchecked idiom. `DisableTrigger(t)` makes it inert;
`EnableTrigger(t)` arms it. A disabled trigger does not run its actions even when
its event fires.

<!-- fsv doc.guard == 1 @1 -->
```lua
local store = Game_Storage()
local victim = Game_CreateUnit(Game_Player(2), Game_UnitType("hfoo"), { x = 30, y = 0 }, 0)

local t = CreateTrigger()
TriggerRegisterUnitEvent(t, victim, "death")
TriggerAddAction(t, function(e)
	Storage_SetInt(store, "doc", "guard", Storage_GetInt(store, "doc", "guard") + 1)
end)
DisableTrigger(t)   -- initially off
EnableTrigger(t)    -- armed by a later trigger
Unit_Kill(victim)   -- now it fires -> 1 (had it stayed disabled, 0)
```

`TriggerEvaluate(t)` runs only the conditions (returns the And of them);
`TriggerExecute(t)` runs the actions *bypassing* conditions — useful for one
trigger to invoke another.

## 6. A complete spell, purely in triggers

The firebolt spell ships as a worked example: `worlds/dev-sandbox/scripts/
spell_firebolt.lua`. A data ability (`data/abilities/firebolt.toml`) binds its
effect-edge behaviour to a trigger *by name* (`BindTriggerName`, #478); casting
the ability fires the trigger with the cast as its event. This block needs the
firebolt data ability + burn buff installed (the dev-sandbox world stands those
up), so it is shown, not run here — it is covered end-to-end by the firebolt FSV
(#479).

```lua-norun
local FIREBOLT_DAMAGE = 30
local t = CreateTrigger()
TriggerAddCondition(t, function(e)
	return Player_IsEnemy(Unit_Owner(Event_Unit(e)), Unit_Owner(Event_Target(e)))
end)
TriggerAddAction(t, function(e)
	local caster, target = Event_Unit(e), Event_Target(e)
	Unit_Damage(caster, target, FIREBOLT_DAMAGE)
	Unit_ApplyBuff(target, Game_BuffType("burn"))
	Game_EmitSpellCue(target) -- non-hashing render cue (#449)
end)
BindTriggerName("firebolt", t)
```

`Game_EmitSpellCue` is a presentation cue on the non-hashing render channel
(#449): it never perturbs the sim hash, so an audio/VFX-on match hashes identical
to a headless one.

## 7. Lifecycle & persistence

Triggers, `OnEvent` handlers, and periodic callbacks are all part of a world's
saveable state: a mid-game save serializes the suspended scheduler and every
authored closure, and a load re-runs the world then restores over it (#481). Keep
actions deterministic (no wall clock, no `os`, no free coroutines) and they
round-trip a save bit-identically.

See also: [combat-overrides.md](combat-overrides.md) for damage/attack/armor
hooks, and [event-coverage.md](event-coverage.md) for the full event-kind table.

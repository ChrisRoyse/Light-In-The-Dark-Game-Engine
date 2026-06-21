# Combat overrides (damage / attack / armor)

**Audience:** world authors customizing the combat math. **Status:** reference for
the programmable-combat surface (ADR #453; issues #474/#475).

Damage in LitD flows through a named **stage pipeline**: a raw amount enters,
each stage may read and rewrite it, and the survivor is applied to the victim.
You override combat by *replacing a stage* with your own Lua, and by tuning the
attack-type × armor-type coefficient matrix and the armor-reduction curve. All of
it is deterministic and runs on the sim scheduler.

> Every fenced `lua` example here is executed against the real engine by
> `TestDocExamplesRunAndLintFSV` (#484).

## 1. The damage event

Inside a damage stage (or an `OnEvent(2, …)` damaged handler) the event exposes
the full packet:

| read | write |
|---|---|
| `DamageEvent_Amount(e)` | `DamageEvent_SetAmount(e, n)` |
| `DamageEvent_RawAmount(e)` | — |
| `DamageEvent_AttackType(e)` | `DamageEvent_SetAttackType(e, "holy")` |
| `DamageEvent_ArmorType(e)` | `DamageEvent_SetArmorType(e, "unarmored")` |
| `DamageEvent_Source(e)`, `DamageEvent_Unit(e)` | — |
| `DamageEvent_Flags(e)` | — |
| | `DamageEvent_ApplyCoefficient(e)` (recompute amount from current types) |

The base pipeline has five named stages, in order: `coeff-lookup` (attack×armor
coefficient), `armor-reduction` (armor curve), `handicap`, `script-modifier`
(the conventional spot for custom gameplay math), `clamp` (final non-negative
clamp). `ReplaceDamageStage(name, fn)` overrides one of these *existing* names —
an unknown name is a loud error.

## 2. Replace a stage: blanket damage scaling

`ReplaceDamageStage(name, fn)` installs `fn` as the named stage. Here a stage
halves every incoming amount; a real attack of 40 lands as 20. The applied amount
is observed from an `OnEvent(2)` damaged handler.

<!-- fsv doc.dmg == 20 @1 -->
```lua
local store = Game_Storage()
ReplaceDamageStage("script-modifier", function(e)
	DamageEvent_SetAmount(e, DamageEvent_Amount(e) / 2)
end)
OnEvent(2, function(e) -- unit damaged
	Storage_SetInt(store, "doc", "dmg", Event_Damage(e))
end)
local a = Game_CreateUnit(Game_Player(1), Game_UnitType("hfoo"), { x = 0, y = 0 }, 0)
local v = Game_CreateUnit(Game_Player(2), Game_UnitType("hfoo"), { x = 200, y = 0 }, 0)
Unit_Damage(a, v, 40)
```

## 3. Switch attack type mid-pipeline

Attack/armor types form a coefficient matrix (`DefineCombat`): in this world
`normal` is 100% and `holy` is 200% vs the lone armor type. A stage reads the
attack type, switches it to `holy`, and re-applies the coefficient: 30 raw
becomes 60 applied.

<!-- fsv doc.holy == 60 @1 -->
```lua
local store = Game_Storage()
ReplaceDamageStage("coeff-lookup", function(e)
	DamageEvent_SetAttackType(e, "holy")
	DamageEvent_ApplyCoefficient(e)
end)
OnEvent(2, function(e)
	Storage_SetInt(store, "doc", "holy", Event_Damage(e))
end)
local a = Game_CreateUnit(Game_Player(1), Game_UnitType("hfoo"), { x = 0, y = 0 }, 0)
local v = Game_CreateUnit(Game_Player(2), Game_UnitType("hfoo"), { x = 200, y = 0 }, 0)
Unit_Damage(a, v, 30) -- stage upgrades it to holy and re-applies the coefficient
```

## 4. Immunity: zero out a stage

Setting the amount to 0 in a stage makes a unit take no damage from that path —
the basis for damage immunities and shields.

<!-- fsv doc.immune == 0 @1 -->
```lua
local store = Game_Storage()
Storage_SetInt(store, "doc", "immune", 999) -- sentinel; the handler overwrites it
ReplaceDamageStage("script-modifier", function(e)
	DamageEvent_SetAmount(e, 0)
end)
OnEvent(2, function(e)
	Storage_SetInt(store, "doc", "immune", Event_Damage(e))
end)
local a = Game_CreateUnit(Game_Player(1), Game_UnitType("hfoo"), { x = 0, y = 0 }, 0)
local v = Game_CreateUnit(Game_Player(2), Game_UnitType("hfoo"), { x = 200, y = 0 }, 0)
Unit_Damage(a, v, 100)
```

## 5. Armor reduction coefficient

`SetArmorReduction(coeff)` tunes the diminishing-returns curve that converts an
armor value into a damage-reduction fraction (#474). It is a global combat knob;
set it once at world setup. (Shown rather than asserted — its effect needs units
with nonzero armor stats configured in the data table.)

```lua-norun
SetArmorReduction(0.06) -- WC3-like 6% per armor point, diminishing
```

## 6. Determinism

Stage callbacks must be pure with respect to the sim: read the event, do integer/
fixed math, write the event. No wall clock, no `os`, no Lua `math.random` (use the
sim PRNG surface). A combat override authored this way hashes identically headless
and rendered, and round-trips a mid-game save (#481).

See also: [triggers.md](triggers.md) for the ECA model these handlers plug into.

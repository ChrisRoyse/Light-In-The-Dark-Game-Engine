# dev-sandbox

The minimal runtime-loadable world (#268) — edit the Lua and reload, no engine
rebuild. It also ships the **canonical trigger-authored spell** (#479), proving
the ECA path is sufficient for a real spell (ADR #452).

## Firebolt — a complete spell, purely in triggers

`data/abilities/firebolt.toml` declares the ability but authors **no** static
effect list. Instead it binds its behavior to a named trigger (#478):

```toml
[[ability]]
id = "firebolt"
trigger = "firebolt"   # mutually exclusive with [[ability.effects]]
cast-point = 0.5
cast-range = 500
```

`scripts/spell_firebolt.lua` is the whole spell — cast event → condition →
actions — against the public `litd/api` surface only:

```lua
local t = CreateTrigger()

-- Condition: the target must be an enemy of the caster.
TriggerAddCondition(t, function(e)
	return Player_IsEnemy(Unit_Owner(Event_Unit(e)), Unit_Owner(Event_Target(e)))
end)

-- Actions: damage, the burn DoT, and a non-hashing VFX cue.
TriggerAddAction(t, function(e)
	local caster, target = Event_Unit(e), Event_Target(e)
	Unit_Damage(caster, target, 30)
	Unit_ApplyBuff(target, Game_BuffType("burn"))
	Game_EmitSpellCue(target)   -- render channel, never perturbs the hash (#449)
end)

BindTriggerName("firebolt", t)  -- the name the firebolt ability references
```

When a unit casts firebolt, the cast machine fires this trigger at the EFFECT
edge with the cast as its event: `Event_Unit(e)` is the caster (the
GetSpellAbilityUnit idiom) and `Event_Target(e)` is the target.

### Key facts

- **Deterministic.** The trigger runs on the cooperative scheduler with the sim
  PRNG; two runs hash identically, and a mid-burn save/load resumes bit-exact.
- **Enemy-only.** Casting at an ally is a no-op — the condition gates the
  actions (the ECA contract).
- **Non-hashing VFX.** `Game_EmitSpellCue` stages a one-shot render cue on the
  presentation channel (#449); an audio/VFX-on game hashes identically to one
  without.

See `cmd/litd/spell_firebolt_test.go` for the full FSV (damage + burn + render
cue, the ally-block, a lethal bolt's death + buff cleanup, determinism, and the
save/resume round-trip).

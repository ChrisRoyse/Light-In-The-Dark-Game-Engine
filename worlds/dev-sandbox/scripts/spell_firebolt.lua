-- spell_firebolt.lua (#479): a complete custom spell authored PURELY via the
-- trigger ECA surface — cast event -> condition -> actions — proving the path is
-- sufficient for a real spell (ADR #452). No engine code; pure Lua + data.
--
-- The firebolt ability (data/abilities/firebolt.toml) binds its EFFECT-edge
-- behavior to the trigger named "firebolt" (#478). When a unit casts firebolt
-- the cast machine fires this trigger with the cast as its event:
--   * Event_Unit(e)   = the caster  (the GetSpellAbilityUnit idiom)
--   * Event_Target(e) = the spell's target
--
-- Condition: the target must be an enemy of the caster (a heal-the-ally misfire
-- is blocked). Actions: deal damage, apply the burn DoT, and stage a one-shot
-- VFX cue on the NON-HASHING render channel (#449) so the visual never perturbs
-- determinism.

local FIREBOLT_DAMAGE = 30

local t = CreateTrigger()

-- Condition: target is an enemy of the caster.
TriggerAddCondition(t, function(e)
	local caster, target = Event_Unit(e), Event_Target(e)
	return Player_IsEnemy(Unit_Owner(caster), Unit_Owner(target))
end)

-- Actions: damage + burn + VFX. Types are resolved here (fire time), so loading
-- this module never depends on the burn buff being installed yet.
TriggerAddAction(t, function(e)
	local caster, target = Event_Unit(e), Event_Target(e)
	Unit_Damage(caster, target, FIREBOLT_DAMAGE)
	Unit_ApplyBuff(target, Game_BuffType("burn"))
	Game_EmitSpellCue(target) -- non-hashing render cue at the impact
end)

-- Bind the trigger to the name the firebolt ability references (#478).
BindTriggerName("firebolt", t)

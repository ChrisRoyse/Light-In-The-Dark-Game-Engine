-- worlds/firstflame-slice/main.lua — the First Flame vertical slice (#482).
--
-- Proves the whole gameplay stack runs on the Trigger/ECA substrate end to end:
--   * ABILITY  — a hero casts firebolt (a data ability whose effect is a bound
--                trigger, scripts/spell_firebolt.lua); damage + burn buff + cue.
--   * ATTACK   — soldiers auto-attack on acquisition; weapon hits raise
--                EventUnitDamaged, observed by an OnEvent(2) trigger.
--   * BUFF     — the burn DoT applied by firebolt is observed via OnEvent(34).
--   * BEACON   — a control point captured by uncontested presence (the First
--                Flame territory mechanic), driven by a Game_Every periodic.
--   * WIN/LOSE — capturing the beacon wins for the holder, defeats the enemy.
--
-- Deterministic, sandbox-clean, no wall clock, no Lua RNG. Every observable is
-- published to Game_Storage so a host (the #482 autotest) can read state.

local store = Game_Storage()
local P1, P2 = Game_Player(1), Game_Player(2)
Player_SetAlliance(P1, P2, 0) -- mutual enemies (flag 0 = not allied)
Player_SetAlliance(P2, P1, 0)

require("scripts/spell_firebolt") -- installs the firebolt ability trigger (#478)

-- Beacon control point (a fixed light in the dark; no map needed for the slice).
local BX, BY = 600, 600
local LIGHT_RADIUS = 280
local CAPTURE_STEPS = 6 -- uncontested 0.25s steps to capture
local NEUTRAL = -1
local beacon = { owner = NEUTRAL, progress = 0, won = false }

local function publishBeacon(step)
	Storage_SetInt(store, "slice", "beacon_owner", beacon.owner)
	Storage_SetInt(store, "slice", "beacon_progress", beacon.progress)
	Storage_SetInt(store, "slice", "beacon_state", beacon.owner ~= NEUTRAL and 1 or 0)
	Storage_SetInt(store, "slice", "step", step)
end
publishBeacon(0)

-- Spawn the combatants: P1 hero + soldier near the beacon, one P2 soldier as
-- the firebolt target and melee opponent.
local hero = Game_CreateUnit(P1, Game_UnitType("hero"), { x = BX - 60, y = BY }, 90)
local ally = Game_CreateUnit(P1, Game_UnitType("soldier"), { x = BX - 30, y = BY - 30 }, 90)
local foe = Game_CreateUnit(P2, Game_UnitType("soldier"), { x = BX + 30, y = BY + 30 }, 270)

-- ATTACK observer: count applied damage packets (auto-attacks + firebolt).
Storage_SetInt(store, "slice", "hits", 0)
OnEvent(2, function(e) -- EventUnitDamaged
	Storage_SetInt(store, "slice", "hits", Storage_GetInt(store, "slice", "hits") + 1)
end)

-- BUFF observer: the firebolt burn is a non-aura buff applied to the foe.
Storage_SetInt(store, "slice", "burned", 0)
OnEvent(34, function(e) -- EventBuffApplied
	if not Event_FromAura(e) then
		Storage_SetInt(store, "slice", "burned", 1)
	end
end)

-- ABILITY: grant + cast firebolt at the foe (the cast fires the bound trigger).
local fb = Unit_AddAbility(hero, Game_AbilityRef("firebolt"))
Unit_CastAbility(hero, fb, foe)

-- BEACON + WIN/LOSE: periodic uncontested-presence capture.
local step = 0
Game_Every(0.25, function()
	step = step + 1
	if not beacon.won then
		local claimant, contested = NEUTRAL, false
		for _, u in ipairs(Game_UnitsInRange({ x = BX, y = BY }, LIGHT_RADIUS)) do
			if Unit_Alive(u) then
				local idx = Player_Slot(Unit_Owner(u))
				if claimant == NEUTRAL then
					claimant = idx
				elseif idx ~= claimant then
					contested = true
				end
			end
		end
		if claimant ~= NEUTRAL and not contested then
			beacon.progress = beacon.progress + 1
			if beacon.progress >= CAPTURE_STEPS then
				beacon.owner = claimant
				beacon.won = true
				Storage_SetInt(store, "slice", "victory_step", step)
				Game_Victory(Game_Player(claimant))
				Game_Defeat(Game_Player(claimant == 1 and 2 or 1), "beacon captured")
			end
		elseif contested then
			beacon.progress = 0 -- frozen/contested resets accrual
		end
	end
	publishBeacon(step)
end)

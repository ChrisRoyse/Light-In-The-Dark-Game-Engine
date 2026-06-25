-- Vigil race start-script (#639): returns a per-player setup function the
-- bootstrap calls. Pure content — it drives only the public melee_* / Game_*
-- Lua verbs (#637), so it validates the public API. Swap this file to re-theme
-- Vigil's opener with zero engine edits. Faction + AI strategy are TOML text
-- (the canonical data format); the bindings parse + validate them fail-closed.
local FACTION = [==[
name = "Vigil"
gold = 500
lumber = 150
food_cap = 12
town_hall = "bastion"
[workers]
code = "lamplighter"
count = 5
]==]

local STRATEGY = [==[
name = "Vigil"
[economy]
gold_workers = 5
wood_workers = 0
normal_pct = 100
[army]
soldier_type = 1
maintain = 8
[waves]
size = 6
[[build]]
type = 3
count = 1
]==]

return function(player, pspec)
	melee_StartingResources(player, FACTION)
	melee_StartingUnits(player, FACTION)
	local loc = Player_StartLocation(player)
	melee_SpawnHero(player, "beacon_warden", { x = loc.x, y = loc.y + 100.0 }, 270)
	if pspec.controller == "cpu" then
		Game_AttachMeleeAI(player, STRATEGY, { gold_id = 0, wood_id = 1 }, pspec.difficulty)
	end
end

-- Unbound race start-script (#640): the Unbound mirror of melee/vigil.lua. Same
-- public-API-only contract; only the unit codes, economy, and AI strategy
-- differ (the faction is data, not code — D2/D4).
local FACTION = [==[
name = "Unbound"
gold = 500
lumber = 150
food_cap = 12
town_hall = "fire_kraal"
[workers]
code = "forager"
count = 5
]==]

local STRATEGY = [==[
name = "Unbound"
[economy]
gold_workers = 5
wood_workers = 0
normal_pct = 100
[army]
soldier_type = 1
maintain = 8
[waves]
size = 5
[[build]]
type = 3
count = 1
]==]

return function(player, pspec)
	melee_StartingResources(player, FACTION)
	melee_StartingUnits(player, FACTION)
	local loc = Player_StartLocation(player)
	melee_SpawnHero(player, "ember_chief", { x = loc.x, y = loc.y + 100.0 }, 270)
	if pspec.controller == "cpu" then
		Game_AttachMeleeAI(player, STRATEGY, { gold_id = 0, wood_id = 1 }, pspec.difficulty)
	end
end

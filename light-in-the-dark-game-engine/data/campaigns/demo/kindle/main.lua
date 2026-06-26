-- Mission 1 "Kindle the Gate" (#312). Ser Caldus relights the gate-beacon and
-- drives back the first Dark wisp. The mission spawns Caldus as a real
-- progression hero, runs a short tick-driven arc (the kindling), and on
-- completion promotes him to level 3, grants the Ember Ward, persists his record
-- to the campaign store for carry-over, and declares victory. Flow logic lives in
-- the script on the deterministic scheduler (D-5), not in the sim tick.
--
-- SoT the campaign on-complete hook reads back: category "campaign:demo",
-- keys demo:caldus:level (his live hero level) and demo:caldus:ember (1 = holds
-- the Ember Ward). We persist the level the hero SYSTEM reports, not a constant.

local CAT = "campaign:demo"
local GATE_TICKS = 20 -- 1.0 s of kindling before the gate catches
local REWARD_LEVEL = 3

local p0 = Game_Player(0)
local store = Game_Storage()

-- Spawn Ser Caldus (a real hero: CreateUnit then SetHeroLevel(1) attaches the
-- level-1 progression row) and the Dark wisp he routs.
local caldus = Game_CreateUnit(p0, Game_UnitType("caldus"), { x = 256, y = 256 }, 90)
Unit_SetHeroLevel(caldus, 1)
Game_CreateUnit(Game_Player(1), Game_UnitType("dark_wisp"), { x = 420, y = 320 }, 270)

local t = 0
local done = false

Game_Every(0.05, function()
	t = t + 1
	if done then
		return
	end
	if t >= GATE_TICKS then
		done = true
		-- The gate catches: Caldus is hardened by the fight (level 3) and recovers
		-- the Ember Ward from the beacon-shrine.
		Unit_SetHeroLevel(caldus, REWARD_LEVEL)
		Unit_AddItemByType(caldus, Game_ItemType("ember_ward"))
		-- Persist the live hero state as the carry-over record (read the level back
		-- from the hero system so the SoT matches what actually happened).
		Storage_SetInt(store, CAT, "demo:caldus:level", Unit_HeroLevel(caldus))
		Storage_SetInt(store, CAT, "demo:caldus:ember", 1)
		Storage_SetInt(store, CAT, "demo:kindle:tick", t)
		Game_Victory(p0)
	end
end)

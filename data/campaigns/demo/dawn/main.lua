-- Mission 2 "Hold the Dawn" (#312). The dawn counterattack: Ser Caldus arrives
-- carrying everything he earned at the gate. The mission reads the carry-over the
-- campaign committed for this mission (hero level + item presence) and instantiates
-- Caldus at that level holding the Ember Ward, then a distinct win condition — hold
-- the line for a short watch rather than kindle's relight-and-rout.
--
-- The carried hero LEVEL and item-count are integers (Storage_GetInt). The hero
-- name / item names are string carry keys, not yet readable from Lua (see the
-- Storage_GetString gap issue) — but the campaign's hero is Ser Caldus and his
-- gate reward is the Ember Ward, so int reads + a presence check are sufficient.
-- Loaded standalone (no carry committed) Caldus simply arrives at level 1.

local CAT = "campaign:demo"
local HOLD_TICKS = 15

local store = Game_Storage()
local p0 = Game_Player(0)

local spawned = false
local t = 0
local victoryAt = -1

Game_Every(0.05, function()
	t = t + 1
	if not spawned then
		spawned = true
		local count = Storage_GetInt(store, CAT, "carry:dawn:hero-count")
		local level, haveLevel = Storage_GetInt(store, CAT, "carry:dawn:hero:0:level")
		local itemCount = Storage_GetInt(store, CAT, "carry:dawn:hero:0:item-count")
		if not haveLevel or level < 1 then
			level = 1
		end
		-- Instantiate the carried hero at his earned level.
		local caldus = Game_CreateUnit(p0, Game_UnitType("caldus"), { x = 256, y = 256 }, 90)
		Unit_SetHeroLevel(caldus, level)
		if itemCount ~= nil and itemCount > 0 then
			Unit_AddItemByType(caldus, Game_ItemType("ember_ward"))
		end
		-- Persist proof of what dawn actually instantiated (SoT for the carry leg).
		Storage_SetInt(store, CAT, "dawn:caldus:level", Unit_HeroLevel(caldus))
		Storage_SetInt(store, CAT, "dawn:carried-heroes", count or 0)
		victoryAt = t + HOLD_TICKS
	elseif victoryAt >= 0 and t >= victoryAt then
		victoryAt = -1
		Storage_SetInt(store, CAT, "dawn:held", 1)
		Game_Victory(p0)
	end
end)

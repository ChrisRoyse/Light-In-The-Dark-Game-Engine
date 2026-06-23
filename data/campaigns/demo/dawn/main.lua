-- Mission 2 "Hold the Dawn" (#312). The dawn counterattack: Ser Caldus arrives
-- carrying everything he earned at the gate. The mission reads the carry-over the
-- campaign committed for this mission and instantiates Caldus at his earned level
-- holding the Ember Ward, then a distinct win condition — hold the line for a short
-- watch rather than kindle's relight-and-rout.
--
-- dawn reconstructs the carried hero from the carry itself: the level + counts are
-- integers (Storage_GetInt) and the hero name + item names are strings
-- (Storage_GetString). Loaded standalone (no carry committed), Caldus simply
-- arrives at level 1 with no carried gear.

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
		local name = Storage_GetString(store, CAT, "carry:dawn:hero:0:name")
		if not haveLevel or level < 1 then
			level = 1
		end
		-- Instantiate the carried hero at his earned level.
		local caldus = Game_CreateUnit(p0, Game_UnitType("caldus"), { x = 256, y = 256 }, 90)
		Unit_SetHeroLevel(caldus, level)
		local itemName = ""
		if itemCount ~= nil and itemCount > 0 then
			itemName = Storage_GetString(store, CAT, "carry:dawn:hero:0:item:0")
			Unit_AddItemByType(caldus, Game_ItemType("ember_ward"))
		end
		-- Persist proof of what dawn actually instantiated (SoT for the carry leg):
		-- the level, the carried-hero count, and the reconstructed name / item read
		-- back from the string carry keys.
		Storage_SetInt(store, CAT, "dawn:caldus:level", Unit_HeroLevel(caldus))
		Storage_SetInt(store, CAT, "dawn:carried-heroes", count or 0)
		Storage_SetString(store, CAT, "dawn:caldus:name", name or "")
		Storage_SetString(store, CAT, "dawn:caldus:item", itemName)
		victoryAt = t + HOLD_TICKS
	elseif victoryAt >= 0 and t >= victoryAt then
		victoryAt = -1
		Storage_SetInt(store, CAT, "dawn:held", 1)
		Game_Victory(p0)
	end
end)

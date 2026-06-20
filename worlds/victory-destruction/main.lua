-- victory-destruction: a prototype of the base-destruction win/lose condition
-- (#200, path b), built entirely against the bound litd/api Lua surface. A
-- competitor whose "hall" structures are all destroyed is defeated; the last
-- competitor still holding a hall wins. The terminal result is staged through
-- Game_Victory/Game_Defeat and resolved by the sim's deterministic phase-6 pass
-- (#344) — exactly one outcome per player, latched, never two winners.
--
-- Beacon-hold victory (#200 path a) composes on top of worlds/beacon-capture's
-- published owner/hold state; this file isolates the destruction path so it can
-- be FSV'd headlessly without the First Flame map.
--
-- All evaluation is on tick boundaries on the deterministic scheduler; no
-- wall-clock, no map iteration in gameplay.

local HALL = Game_UnitType("hall")
local COMPETITORS = { 1, 2 } -- the scenario's players (a map would derive these)
local PLAYING = 0            -- MatchResult.Playing
local resolved = false

local store = Game_Storage()

Game_Every(0.25, function()
	if resolved then
		return
	end

	-- Count each competitor's living halls.
	local halls = {}
	for _, u in ipairs(Game_AllUnits()) do
		if Unit_Type(u) == HALL then
			local s = Player_Slot(Unit_Owner(u))
			halls[s] = (halls[s] or 0) + 1
		end
	end

	-- Defeat any still-playing competitor with no halls left; collect survivors.
	local survivors = {}
	for _, s in ipairs(COMPETITORS) do
		local p = Game_Player(s)
		if Player_Result(p) == PLAYING then
			if (halls[s] or 0) == 0 then
				Game_Defeat(p, "all halls destroyed")
			else
				survivors[#survivors + 1] = s
			end
		end
	end

	-- The match is decided once at most one competitor remains. Exactly one
	-- survivor wins (deterministic single result via #344); zero survivors is a
	-- draw — mutual elimination, where every competitor's last hall fell on the
	-- same scan and all were Defeated above. Latch `resolved` in BOTH cases: a
	-- draw is still a terminal outcome, so gating only on survivors==1 would leave
	-- `resolved` false forever after a double-KO even though no one is still
	-- playing. A draw declares no victor.
	if #survivors <= 1 then
		if #survivors == 1 then
			Game_Victory(Game_Player(survivors[1]))
		end
		resolved = true
	end
	Storage_SetInt(store, "match", "resolved", resolved and 1 or 0)
end)

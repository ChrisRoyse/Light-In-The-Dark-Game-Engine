-- firstclash bootstrap (#638): read the match descriptor the host parsed from
-- match.toml (Game_MatchSpec), set each player's start location, then run that
-- player's race start-script (melee/<race>.lua) — the swappable custom-game
-- layer. A mod overrides setup by swapping a melee/<race>.lua file, zero engine
-- edits. require() resolves ONLY sibling world chunks (sandboxed, no fs), and
-- raises loudly if a roster race has no melee/<race>.lua — never a silent skip.
local spec = Game_MatchSpec()

-- Start locations per slot (no terrain map in firstclash; positions are world
-- data here). Two opposing corners so the bases do not overlap.
local STARTS = {
	[0] = { x = 600.0,  y = 600.0  },
	[1] = { x = 3400.0, y = 3400.0 },
}

-- spec.players is slot-ascending (LoadMatchSpec guarantees it); keep that order
-- so the victory tiebreak (lower slot wins exact ties) is deterministic.
local roster = {}
for _, p in ipairs(spec.players) do
	local player = Game_Player(p.slot)
	local s = STARTS[p.slot]
	if s == nil then
		error("firstclash: no start location for slot " .. tostring(p.slot))
	end
	Player_SetStartLocation(player, { x = s.x, y = s.y })
	local setup = require("melee/" .. p.race) -- loud error if the script is missing
	setup(player, p)
	roster[#roster + 1] = player
end

-- Wire the resolution layer: decisive last-standing (#646) + the 24,000-tick
-- score backstop (#647), so the AI-vs-AI match always terminates with a winner.
require("melee/victory")(roster)

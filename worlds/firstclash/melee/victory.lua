-- firstclash victory layer (#646 + #647). Returned as install(players); the
-- bootstrap calls it once both players are set up. It wires TWO resolution paths
-- so an AI-vs-AI match always terminates (the epic thesis):
--
--   #646 decisive: the standard last-standing rule — the moment one player has
--   no units left it is defeated and the lone survivor wins (fires on
--   elimination, the "real" win).
--
--   #647 backstop: a one-shot timer at tick 24,000 (20 in-game minutes; 50 ms
--   tick). If the match is still undecided then (e.g. a turtle stalemate), it
--   score-decides deterministically so there is ALWAYS exactly one winner — no
--   stalemate. Score = live unit count (dominant) then gold; exact ties go to the
--   lower slot (players are passed slot-ascending, and the scan keeps the first
--   maximum), so the outcome is a pure function of sim state (R-SIM-2): no float,
--   no map iteration.
local TIMEOUT_SECONDS = 1200.0 -- 24,000 ticks * 50 ms

-- liveUnits counts player p's living units via a transient group query.
local function liveUnits(p)
	local grp = NewGroup()
	GroupFillOwner(grp, p, { aliveOnly = true })
	local n = GroupCount(grp)
	DestroyGroup(grp)
	return n
end

-- score is the deterministic integer rank: unit count dominates, gold breaks
-- ties. (Lumber is intentionally omitted — not yet on the Lua surface, see the
-- filed gap; gold alone keeps the tiebreak deterministic.)
local function score(p)
	return liveUnits(p) * 1000000 + Player_Gold(p)
end

return function(players)
	melee_VictoryDefeatConditions(players) -- #646

	-- #647: a Game_Every timer whose FIRST fire lands at tick 24,000 (one period),
	-- self-stopped so it behaves as a one-shot. Game_Every is chosen over
	-- Game_After deliberately: it is backed by a serializable periodic-timer
	-- trigger (#464), so the timeout survives a mid-match save/load (#652) —
	-- Game_After's one-shot callback is not yet save-serializable (#270 class).
	Game_Every(TIMEOUT_SECONDS, function(timer)
		local wi = 1
		for i = 2, #players do
			if score(players[i]) > score(players[wi]) then
				wi = i
			end
		end
		Game_Victory(players[wi])
		for i = 1, #players do
			if i ~= wi then
				Game_Defeat(players[i], "firstclash: 24,000-tick score-decide timeout")
			end
		end
		Timer_Stop(timer) -- one-shot: do not re-fire after the decision
	end)
end

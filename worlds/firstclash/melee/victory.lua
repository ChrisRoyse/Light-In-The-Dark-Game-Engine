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

-- score is the deterministic integer rank: live unit count dominates, then gold,
-- then lumber breaks remaining ties. All three are on the Lua surface
-- (Player_Gold / Player_Lumber), and all are deterministic sim reads — so the
-- rank is total and reproducible (R-SIM-2).
local function score(p)
	return liveUnits(p) * 1000000000 + Player_Gold(p) * 1000 + Player_Lumber(p)
end

return function(players)
	melee_VictoryDefeatConditions(players) -- #646

	-- #647: a Game_Every timer whose FIRST fire lands at tick 24,000 (one period),
	-- self-stopped so it behaves as a one-shot. Game_After is now equally save-safe
	-- (#661 backs it with the serializable single-fire timer wheel), so either verb
	-- survives a mid-match save/load (#652); this keeps the original Game_Every
	-- (#464) form to avoid re-baselining the firstclash determinism fixtures for a
	-- no-op behavioral change.
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

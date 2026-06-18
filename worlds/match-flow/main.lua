-- match-flow: a prototype of the match flow state machine (#201) — the
-- setup → countdown → play → terminal loop that makes the First Flame a game and
-- not just a scene. Built purely against the bound litd/api Lua surface and run
-- on the deterministic scheduler (flow state lives in the script, NOT in the sim
-- tick — D-5). It consumes the single game-result substrate (#200/#344): during
-- PLAY it watches the local player's result and latches to TERMINAL when a
-- victory/defeat resolves.
--
-- Scope: this is the headless flow LOGIC. The victory/defeat SCREEN, faction-pick
-- and countdown UI (via g.UI(), R-UI-1), locale-table strings (D-17), and the
-- main menu are the UI/content half — gated, and verified by screenshot, not here.
-- Each world load is a fresh match, so per-match reset is structural (no teardown
-- leak): a second LoadWorld starts cleanly at SETUP.

local SETUP, COUNTDOWN, PLAY, TERMINAL = 0, 1, 2, 3
local SETUP_TICKS = 5
local COUNTDOWN_TICKS = 20 -- 1.0 s
local LOCAL = Game_Player(1)
local store = Game_Storage()

local state = -1
local t = 0
local enteredAt = 0
local startedAt = -1

local function setState(s)
	state = s
	enteredAt = t
	Storage_SetInt(store, "match", "state", state)
end

local function publish()
	Storage_SetInt(store, "match", "tick", t)
end
setState(SETUP)
publish()

Game_Every(0.05, function()
	t = t + 1
	if state == SETUP then
		if t - enteredAt >= SETUP_TICKS then
			setState(COUNTDOWN)
		end
	elseif state == COUNTDOWN then
		if t - enteredAt >= COUNTDOWN_TICKS then
			startedAt = t
			Storage_SetInt(store, "match", "startedat", startedAt)
			setState(PLAY)
		end
	elseif state == PLAY then
		local r = Player_Result(LOCAL) -- 0 playing, 1 won, 2 lost, 3 left
		if r ~= 0 then
			Storage_SetInt(store, "match", "result", r)
			Storage_SetInt(store, "match", "duration", t - startedAt)
			setState(TERMINAL)
		end
	end
	-- TERMINAL is latched: no further transitions.
	publish()
end)

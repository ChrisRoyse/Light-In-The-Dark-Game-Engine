-- flicker-cycle: a prototype of the Flicker (beacon pulse bright/dim cycle, the
-- Veil's day/night analogue, #170), built purely against the bound litd/api Lua
-- surface. The dim phase applies an empowerment buff to every unit; returning to
-- bright strips it. The cycle is anchored to a TICK COUNT (not wall clock), so it
-- is lockstep-deterministic (D-5).
--
-- Scope: this isolates the SCRIPTABLE half of #170 — the data-driven dim-phase
-- empowerment (dogfooding the buff surface, #267) — published to Storage as the
-- headless FSV's source of truth. The sight_day/sight_night narrowing flows
-- through unit DATA fields (no Lua sight setter exists by design, per the spec's
-- "no new engine surface"); beacon-VFX dim state + ambient drop + screenshots are
-- the render half, gated on the First Flame map (#174) + VFX assets.
--
-- No per-tick allocation concern is optimized here (prototype); the real
-- worlds/firstflame/scripts/flicker.lua will hoist the unit scan.

local BRIGHT_TICKS = 60          -- 3.0 s
local DIM_TICKS = 40             -- 2.0 s
local CYCLE = BRIGHT_TICKS + DIM_TICKS
local BRIGHT, DIM = 0, 1
local EMPWR = Game_BuffType("dimpwr")

local t = 0                      -- tick counter (incremented once per sim tick)
local lastPhase = BRIGHT
local transitions = 0
local store = Game_Storage()

local function phaseAt(tick)
	if (tick % CYCLE) >= BRIGHT_TICKS then
		return DIM
	end
	return BRIGHT
end

local function publish()
	Storage_SetInt(store, "flicker", "phase", lastPhase)
	Storage_SetInt(store, "flicker", "tick", t)
	Storage_SetInt(store, "flicker", "transitions", transitions)
end
publish()

-- 0.05 s == one 20 tps tick: this fires once per tick, deterministically.
Game_Every(0.05, function()
	t = t + 1
	local ph = phaseAt(t)
	if ph == DIM then
		-- Empower every unit for the dim phase (idempotent: only the unbuffed,
		-- so units trained mid-dim are picked up too).
		for _, u in ipairs(Game_AllUnits()) do
			if not Unit_HasBuff(u, EMPWR) then
				Unit_ApplyBuff(u, EMPWR)
			end
		end
	elseif ph ~= lastPhase then
		-- Just returned to bright: strip the empowerment.
		for _, u in ipairs(Game_AllUnits()) do
			Unit_RemoveAllBuffs(u)
		end
	end
	if ph ~= lastPhase then
		transitions = transitions + 1
		lastPhase = ph
	end
	publish()
end)

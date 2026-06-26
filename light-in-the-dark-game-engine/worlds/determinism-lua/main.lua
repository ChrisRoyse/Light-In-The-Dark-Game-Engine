-- determinism-lua: the G5.7 determinism scenario world (#271).
--
-- It deliberately exercises every determinism-sensitive Lua surface so the
-- 10k-tick state hash is a meaningful regression oracle:
--   * math.random  — bound to the sim PRNG (R-SIM-2, #263/#400), so draws are
--                     deterministic and counted in sim state;
--   * pairs()      — iteration order is deterministic in the LITD gopher-lua
--                     fork (TestLuaPairsDeterministicOrder);
--   * string.format — pure-Go strconv formatting of numbers;
--   * coroutines + PolledWait — run on the deterministic cooperative scheduler;
--   * OnEvent      — sim-event dispatch into Lua.
--
-- The host binds "hfoo" (data/units) and a RandomSource; nothing here depends on
-- wall-clock, map iteration in gameplay, or any nondeterministic source.

Game_SetTimeOfDay(9.0)

-- Count unit deaths dispatched back into Lua (observable cross-check).
deaths = 0
OnEvent(1, function() deaths = deaths + 1 end) -- 1 = EventUnitDeath

-- Spawn four units at PRNG-chosen positions.
local units = {}
for i = 1, 4 do
	local x = 100 + math.random() * 400
	local y = 100 + math.random() * 400
	units[i] = Game_CreateUnit(Game_Player(0), Game_UnitType("hfoo"), { x = x, y = y }, 0)
end

-- pairs() iteration + string.format over the spawned set (deterministic order).
local tags = {}
for k, u in pairs(units) do
	tags[k] = string.format("u%d@%.2f", k, Unit_Position(u).x)
end

-- Each unit walks east in 5-unit steps, three ticks apart, via its own coroutine.
for i = 1, 4 do
	Run(function()
		for step = 1, 20 do
			local p = Unit_Position(units[i])
			Unit_SetPosition(units[i], { x = p.x + 5, y = p.y })
			PolledWait(0.15)
		end
	end)
end

-- One coroutine kills unit 1 at t = 1s (20 ticks), firing exactly one death event.
Run(function()
	PolledWait(1.0)
	Unit_Kill(units[1])
end)

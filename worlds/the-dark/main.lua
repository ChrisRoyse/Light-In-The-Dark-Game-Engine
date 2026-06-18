-- the-dark: a prototype of the Dark's escalating spawn waves (#172), built purely
-- against the bound litd/api Lua surface. The Dark is neutral-hostile and never
-- builds or gathers — it manifests in waves whose pressure scales with the number
-- of extinguished (dark) beacons: more dark beacons → shorter wave interval AND
-- higher creature tier. All beacons lit → zero pressure. Waves spawn ONLY at dark
-- beacons (never inside a lit radius). All spawn randomness draws from the sim
-- PRNG (Game_RandomInt), so runs are deterministic per seed (R-SIM-2).
--
-- Beacon control is the input: per-beacon lit state is read from Storage
-- ("beacon"/"lit_<i>", 1=lit/0=dark), which a real map (#174) publishes from the
-- beacon mechanic (#169, worlds/beacon-capture). Here the harness sets it to
-- script the escalation scenarios. Wave log is published to Storage (the headless
-- FSV's SoT). Creature models/tiers are the asset half (blocked), stubbed here as
-- three unit types.

local BEACONS = { { x = 500, y = 500 }, { x = 1500, y = 500 }, { x = 1000, y = 1500 } }
local TIERS = { "gloam_whelp", "gloam_stalker", "gloam_horror" } -- tier 1..3
local BASE_INTERVAL = 60 -- ticks between waves at one dark beacon
local STEP = 15          -- interval shrinks this much per additional dark beacon
local MIN_INTERVAL = 20
local JITTER = 40        -- spawn scatter (sim units) around a beacon

local dark = Game_NeutralHostile()
local store = Game_Storage()

local t = 0
local lastWave = 0
local waves = 0
local totalSpawned = 0
local lastX, lastY = -1, -1
local lastBeaconX = -1 -- the dark beacon the last wave spawned at (validity SoT)

local function darkBeacons()
	local out = {}
	for i, b in ipairs(BEACONS) do
		if Storage_GetInt(store, "beacon", "lit_" .. i) ~= 1 then
			out[#out + 1] = b
		end
	end
	return out
end

local function intervalFor(n)
	local iv = BASE_INTERVAL - (n - 1) * STEP
	if iv < MIN_INTERVAL then
		iv = MIN_INTERVAL
	end
	return iv
end

local function publish(darkCount, interval, tier)
	Storage_SetInt(store, "dark", "waves", waves)
	Storage_SetInt(store, "dark", "darkcount", darkCount)
	Storage_SetInt(store, "dark", "interval", interval)
	Storage_SetInt(store, "dark", "tier", tier)
	Storage_SetInt(store, "dark", "total", totalSpawned)
	Storage_SetInt(store, "dark", "lastx", lastX)
	Storage_SetInt(store, "dark", "lasty", lastY)
	Storage_SetInt(store, "dark", "lastbeaconx", lastBeaconX)
end
publish(0, 0, 0)

Game_Every(0.05, function()
	t = t + 1
	local db = darkBeacons()
	local n = #db
	if n == 0 then
		publish(0, 0, 0) -- all beacons lit → minimal pressure, no waves
		return
	end
	local interval = intervalFor(n)
	local tier = n
	if tier > #TIERS then
		tier = #TIERS
	end
	if t - lastWave >= interval then
		lastWave = t
		waves = waves + 1
		for _, b in ipairs(db) do
			local count = tier + Game_RandomInt(0, 1) -- composition jitter (sim PRNG)
			for _ = 1, count do
				local sx = b.x + Game_RandomInt(-JITTER, JITTER)
				local sy = b.y + Game_RandomInt(-JITTER, JITTER)
				Game_CreateUnit(dark, Game_UnitType(TIERS[tier]), { x = sx, y = sy }, 0)
				totalSpawned = totalSpawned + 1
				lastX, lastY, lastBeaconX = sx, sy, b.x
			end
		end
	end
	publish(n, interval, tier)
end)

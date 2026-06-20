-- beacon-capture: a self-contained prototype of the Beacon control-point
-- mechanic (#169), built ENTIRELY against the bound litd/api Lua surface — the
-- dogfooding gate (milestones.md §10: any capability this needs and lacks is an
-- M5 defect, not a private hook). It is map-independent (one beacon at a fixed
-- point); worlds/firstflame/scripts/beacon.lua (#169, gated on the First Flame
-- map + VFX) will instantiate this same logic per placed beacon.
--
-- Mechanic (per #169 spec):
--   * A beacon is a world point with a light radius.
--   * Each capture tick, the units inside the radius are scanned. If exactly one
--     player owns units there (no enemy contest), that player accrues capture
--     progress; a contested radius FREEZES progress; an empty radius holds.
--   * At the capture threshold the beacon becomes lit for that player and stamps
--     the owner's vision over the light radius (fog-of-war integration, API only).
--   * State (owner index, progress, lit) is published to Storage so a headless
--     FSV can read it as the source of truth.
--
-- All times/radii are integer sim units; randomness, were any used, would route
-- through the sim PRNG (R-SIM-2). No wall-clock, no map iteration.

local BEACON = { x = 1000, y = 1000 }
local LIGHT_RADIUS = 250
local CAPTURE_TICKS = 40          -- 2.0 s at 20 tps
local TICK_SECONDS = 0.25         -- scan 4x/second (5-tick quantized)
local TICK_PROGRESS = 5           -- progress per scan; 8 scans (2 s) → 40

local NEUTRAL = -1
local owner = NEUTRAL             -- player index currently lighting the beacon, or NEUTRAL
local progress = 0
local challenger = NEUTRAL        -- player currently accruing progress (per-challenger capture)
local fogMod = nil                -- persistent vision modifier, created once on capture
local store = Game_Storage()

-- publish() writes the beacon's state to Storage — the headless FSV's SoT.
local function publish()
	Storage_SetInt(store, "beacon", "owner", owner)
	Storage_SetInt(store, "beacon", "progress", progress)
	Storage_SetInt(store, "beacon", "lit", owner ~= NEUTRAL and 1 or 0)
end
publish()

-- contender() scans the light radius and returns the index of the single player
-- contesting it, or NEUTRAL when the radius is empty or contested by enemies.
local function contender()
	local units = Game_UnitsInRange(BEACON, LIGHT_RADIUS)
	local claimant = NEUTRAL
	for _, u in ipairs(units) do
		local p = Unit_Owner(u)
		local idx = Player_Slot(p)
		if claimant == NEUTRAL then
			claimant = idx
		elseif idx ~= claimant and Player_IsEnemy(Game_Player(claimant), p) then
			return NEUTRAL          -- mutually-hostile presence → contested
		end
	end
	return claimant
end

-- The capture loop: a deterministic repeating timer (Game_After-family).
Game_Every(TICK_SECONDS, function()
	local who = contender()
	if who == NEUTRAL then
		-- empty or contested: progress frozen (held, not decayed, for this proto).
	elseif who == owner then
		-- already lit by the contender: nothing to do (modifier persists).
	else
		-- Per-challenger accrual: a claimant different from the one who built up the
		-- current progress (e.g. after a contest froze a rival's charge and that
		-- rival left) starts from zero instead of inheriting the lead.
		if who ~= challenger then
			challenger = who
			progress = 0
		end
		progress = progress + TICK_PROGRESS
		if progress >= CAPTURE_TICKS then
			-- Ownership transfer. Stop the PREVIOUS owner's persistent vision first
			-- (else its fog modifier leaks and the old owner keeps seeing the
			-- beacon forever); then reset progress to 0 so a future challenger must
			-- earn the full capture duration again — without the reset progress
			-- stays clamped at CAPTURE_TICKS and the next sole contender flips
			-- ownership in a single scan (a 2 s capture stolen in 0.25 s).
			if fogMod ~= nil then
				FogModifier_Stop(fogMod)
				FogModifier_Destroy(fogMod)
			end
			owner = who
			progress = 0
			challenger = NEUTRAL -- captured; next challenger accrues fresh
			-- Light it: a persistent fog modifier reveals the radius to the owner
			-- (survives vision recomputes, unlike a per-tick SetFogState stamp).
			fogMod = Game_NewFogModifier(Game_Player(owner), 2, -- 2 = FogVisible
				{ cx = BEACON.x, cy = BEACON.y, radius = LIGHT_RADIUS })
			FogModifier_Start(fogMod)
		end
	end
	publish()
end)

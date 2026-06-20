-- firstflame-beacon: the beacon control-point mechanic (#169) running on the REAL
-- First Flame map (#174) — proving the map→world wiring (#410) end to end. Unlike
-- worlds/beacon-capture (which hardcodes a beacon point), this reads the beacon
-- placements straight from the loaded map via Game_MapBeacons() and captures the
-- central beacon at its authored map cell. This is the shape of the eventual
-- worlds/firstflame/scripts/beacon.lua (one capture loop per placed beacon).

local LIGHT_RADIUS = 250
local CAPTURE_TICKS = 40
local TICK_PROGRESS = 5
local NEUTRAL = -1

local beacons = Game_MapBeacons()
assert(#beacons > 0, "firstflame-beacon: map has no beacons")
local B = beacons[1] -- beacons are id-sorted; id 1 is the central prize

local owner = NEUTRAL
local progress = 0
local fogMod = nil
local store = Game_Storage()

local function publish()
	Storage_SetInt(store, "beacon", "owner", owner)
	Storage_SetInt(store, "beacon", "progress", progress)
	Storage_SetInt(store, "beacon", "lit", owner ~= NEUTRAL and 1 or 0)
	Storage_SetInt(store, "beacon", "x", B.x) -- map-sourced world coords (SoT proof)
	Storage_SetInt(store, "beacon", "y", B.y)
end
publish()

local function contender()
	local claimant = NEUTRAL
	for _, u in ipairs(Game_UnitsInRange({ x = B.x, y = B.y }, LIGHT_RADIUS)) do
		local p = Unit_Owner(u)
		local idx = Player_Slot(p)
		if claimant == NEUTRAL then
			claimant = idx
		elseif idx ~= claimant and Player_IsEnemy(Game_Player(claimant), p) then
			return NEUTRAL
		end
	end
	return claimant
end

Game_Every(0.25, function()
	local who = contender()
	if who == NEUTRAL then
	elseif who == owner then
	else
		progress = progress + TICK_PROGRESS
		if progress >= CAPTURE_TICKS then
			-- Ownership transfer: stop the previous owner's persistent vision (else
			-- its fog modifier leaks) and reset progress to 0, so a future challenger
			-- must earn the full capture duration rather than flip in a single scan
			-- (progress was clamped at CAPTURE_TICKS → next contender hit the
			-- threshold instantly). Same fix as worlds/beacon-capture.
			if fogMod ~= nil then
				FogModifier_Stop(fogMod)
				FogModifier_Destroy(fogMod)
			end
			owner = who
			progress = 0
			fogMod = Game_NewFogModifier(Game_Player(owner), 2,
				{ cx = B.x, cy = B.y, radius = LIGHT_RADIUS })
			FogModifier_Start(fogMod)
		end
	end
	publish()
end)

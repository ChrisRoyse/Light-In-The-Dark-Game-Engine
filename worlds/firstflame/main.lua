-- worlds/firstflame/main.lua — Beacon control points (#169) on the real First
-- Flame map (#174), read from the loaded map via Game_MapBeacons() (#410), no
-- hardcoded coordinates. The core territory mechanic: light = safety = vision =
-- territory (identity.md §3). Pure Lua + sim-PRNG-free deterministic logic on
-- the cooperative scheduler (R-EXEC-1), sandbox-clean (R-SEC-1).
--
-- This is the canonical beacon world the First Flame slice ships (the eventual
-- worlds/firstflame/scripts/beacon.lua once a multi-chunk world-composition
-- mechanism exists; today a world is a single main.lua entry). The 3D beacon
-- model + light-state VFX + screenshots are the render/asset half of #169.

-- Authored beacon parameters (integer sim units, tick-quantized). A "tick" here
-- is one Game_Every(0.25s) step = 5 sim ticks.
local LIGHT_RADIUS = 250 -- world units; capture range == granted vision radius
local CAPTURE_STEPS = 8 -- steps of uncontested presence to capture (2.0s)
local ACCRUE = 1 -- progress gained per uncontested step by a lone claimant
local DECAY = 1 -- progress lost per step when a neutral beacon is abandoned
local NEUTRAL = -1

-- State per map beacon (id-sorted by Game_MapBeacons → deterministic order).
local beacons = {}
for _, b in ipairs(Game_MapBeacons()) do
	beacons[#beacons + 1] = {
		id = b.id, x = b.x, y = b.y,
		owner = NEUTRAL, progress = 0, fog = nil,
	}
end
assert(#beacons > 0, "firstflame: map has no beacons")

local store = Game_Storage()

-- state code: 0 neutral, 1 lit(owned). (extinguished is reserved for the Dark
-- escalation hook, #172, which consumes the lit→dark transition.)
local function publish(i, b)
	local key = "beacon" .. i
	Storage_SetInt(store, key, "id", b.id)
	Storage_SetInt(store, key, "owner", b.owner)
	Storage_SetInt(store, key, "progress", b.progress)
	Storage_SetInt(store, key, "state", b.owner ~= NEUTRAL and 1 or 0)
	Storage_SetInt(store, key, "x", b.x)
	Storage_SetInt(store, key, "y", b.y)
end

for i, b in ipairs(beacons) do
	publish(i, b)
end

-- scan returns (claimant Player_Slot or NEUTRAL, contested bool). Contested =
-- two mutually-enemy players have units in radius → capture frozen.
local function scan(b)
	local claimant = NEUTRAL
	local contested = false
	for _, u in ipairs(Game_UnitsInRange({ x = b.x, y = b.y }, LIGHT_RADIUS)) do
		local p = Unit_Owner(u)
		local idx = Player_Slot(p)
		if claimant == NEUTRAL then
			claimant = idx
		elseif idx ~= claimant and Player_IsEnemy(Game_Player(claimant), p) then
			contested = true
		end
	end
	return claimant, contested
end

local function light(b, slot)
	b.owner = slot
	b.progress = CAPTURE_STEPS
	b.fog = Game_NewFogModifier(Game_Player(slot), 2, { cx = b.x, cy = b.y, radius = LIGHT_RADIUS })
	FogModifier_Start(b.fog)
end

Game_Every(0.25, function()
	for i, b in ipairs(beacons) do
		local claimant, contested = scan(b)
		if contested then
			-- frozen: neither progress nor ownership changes.
		elseif claimant == NEUTRAL then
			-- abandoned: an unowned beacon's progress decays; a lit beacon is
			-- RETAINED (owner keeps it after leaving — retention).
			if b.owner == NEUTRAL and b.progress > 0 then
				b.progress = b.progress - DECAY
				if b.progress < 0 then b.progress = 0 end
			end
		elseif claimant == b.owner then
			-- owner present on its own lit beacon: nothing to do.
		else
			-- a single non-owner claimant accrues toward capture.
			b.progress = b.progress + ACCRUE
			if b.progress >= CAPTURE_STEPS then
				light(b, claimant)
			end
		end
		publish(i, b)
	end
end)

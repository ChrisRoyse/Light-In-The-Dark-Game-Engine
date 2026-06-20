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
	-- Ownership transfer. Stop the PREVIOUS owner's persistent vision first (else
	-- its fog modifier leaks and the old owner keeps seeing the beacon forever),
	-- then reset progress to 0 so a future challenger must earn the full capture
	-- duration again. Leaving progress clamped at CAPTURE_STEPS would let the next
	-- sole non-owner claimant accrue CAPTURE_STEPS+ACCRUE >= CAPTURE_STEPS and flip
	-- ownership in a single 0.25 s step (a 2 s capture stolen in one scan).
	if b.fog ~= nil then
		FogModifier_Stop(b.fog)
		FogModifier_Destroy(b.fog)
	end
	b.owner = slot
	b.progress = 0
	b.fog = Game_NewFogModifier(Game_Player(slot), 2, { cx = b.x, cy = b.y, radius = LIGHT_RADIUS })
	FogModifier_Start(b.fog)
end

-- Beacon-control victory (#200 path a): hold >= HOLD_THRESHOLD beacons for
-- HOLD_STEPS continuous steps. The hold timer resets the instant a player drops
-- below the threshold. The terminal result is emitted EXACTLY ONCE; a
-- simultaneous tie is broken deterministically by lowest player slot.
local HOLD_THRESHOLD = 2 -- beacons held simultaneously
local HOLD_STEPS = 12 -- continuous steps (3.0s) to convert a hold into victory
local PLAYERS = 2 -- competitors (slots 0..PLAYERS-1)

local holdSteps = {} -- slot -> consecutive steps at/above threshold
for s = 0, PLAYERS - 1 do
	holdSteps[s] = 0
end
local decided = false

local function publishFlow(winner)
	Storage_SetInt(store, "match", "decided", decided and 1 or 0)
	Storage_SetInt(store, "match", "winner", winner)
	for s = 0, PLAYERS - 1 do
		Storage_SetInt(store, "hold", "p" .. s, holdSteps[s])
	end
end
publishFlow(NEUTRAL)

local function ownedCount(slot)
	local n = 0
	for _, b in ipairs(beacons) do
		if b.owner == slot then n = n + 1 end
	end
	return n
end

local function evalVictory()
	if decided then return end
	local winner = NEUTRAL
	for s = 0, PLAYERS - 1 do -- ascending slot = deterministic tie-break
		if ownedCount(s) >= HOLD_THRESHOLD then
			holdSteps[s] = holdSteps[s] + 1
		else
			holdSteps[s] = 0
		end
		if winner == NEUTRAL and holdSteps[s] >= HOLD_STEPS then
			winner = s
		end
	end
	if winner ~= NEUTRAL then
		decided = true
		Game_Victory(Game_Player(winner))
		for s = 0, PLAYERS - 1 do
			if s ~= winner then Game_Defeat(Game_Player(s), "out-held at the beacons") end
		end
	end
	publishFlow(winner)
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
	evalVictory()
end)

-- the-flame: a prototype of the Unbound portable flame (#171), built purely
-- against the bound litd/api Lua surface. A pyre_wagon carries a MOVING light: a
-- fog-of-war stamp + a friendly aura buff that travel with the wagon. The flame
-- is the faction's answer to fixed beacons — and its radius is immune to the
-- Flicker dim (#170), because the carried light is its own source.
--
-- Each tick the aura is cleared from everyone and re-applied to friendlies inside
-- any LIVE wagon's flame radius. That single rule gives three required behaviours
-- for free: (a) wagon death extinguishes the flame next tick (a dead wagon is not
-- live → nobody re-buffed); (b) the aura is non-stacking (cleared-then-applied
-- once per unit); (c) leaving the radius drops the aura next tick. The wagon's
-- current position is published to Storage as the headless tracking SoT.
--
-- VISION is intentionally NOT a per-tick Game_SetFogState stamp here: a manual
-- fog stamp is wiped by the next visibility recompute (verified — FogStateAt read
-- back Fogged, not Visible), so it cannot provide persistent mobile light. The
-- flame's light is the pyre_wagon's intrinsic SightDay/SightNight (data fields,
-- units.go) — the engine already maintains unit sight as the unit moves, with no
-- new engine surface, and the flame's Flicker-immunity (#170) is simply
-- SightNight == SightDay on the wagon. This script owns only the data-driven AURA.
--
-- Aura values / radius are the data half (flame params, #171). Models + the ≤8
-- VFX light-cap interaction are the asset/render half (blocked).

local WAGON = Game_UnitType("pyre_wagon")
local AURA = Game_BuffType("flame_aura")
local FLAME_RADIUS = 200
local store = Game_Storage()

Game_Every(0.05, function()
	-- Collect live wagons.
	local wagons = {}
	for _, u in ipairs(Game_AllUnits()) do
		if Unit_Type(u) == WAGON and Unit_Alive(u) then
			wagons[#wagons + 1] = u
		end
	end

	-- Clear the aura everywhere, then re-apply within live flame radii.
	for _, u in ipairs(Game_AllUnits()) do
		Unit_RemoveBuff(u, AURA)
	end

	local cx, cy, lit = -1, -1, 0
	for _, w in ipairs(wagons) do
		local pos = Unit_Position(w)
		local owner = Unit_Owner(w)
		cx, cy, lit = pos.x, pos.y, 1
		-- Friendly aura: non-enemy units in range get the (non-stacking) buff.
		-- (Vision is the wagon's SightDay/SightNight data, not a fog stamp — see header.)
		for _, u in ipairs(Game_UnitsInRange({ x = pos.x, y = pos.y }, FLAME_RADIUS)) do
			if not Player_IsEnemy(owner, Unit_Owner(u)) and not Unit_HasBuff(u, AURA) then
				Unit_ApplyBuff(u, AURA)
			end
		end
	end

	Storage_SetInt(store, "flame", "lit", lit)
	Storage_SetInt(store, "flame", "x", cx)
	Storage_SetInt(store, "flame", "y", cy)
	Storage_SetInt(store, "flame", "wagons", #wagons)
end)

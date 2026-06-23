-- the-flame: the Unbound portable flame (#171), built purely against the bound
-- litd/api Lua surface. A pyre_wagon carries a MOVING light — the faction's
-- answer to fixed beacons — made of two data-driven halves:
--
--   VISION is the wagon's intrinsic sight (data fields sight-day/sight-night in
--   data/units/units.toml). The engine already maintains a unit's revealed area
--   as it moves, so the light travels with the wagon for free — no fog stamp.
--   A manual Game_SetFogState stamp would be wiped by the next visibility
--   recompute (verified: it read back Fogged, not Visible), so it cannot provide
--   persistent mobile light. The flame's Flicker-immunity (#170) is simply
--   sight-night == sight-day on the wagon: the dim phase never shrinks it.
--
--   The AURA is this script's job — and it leans on the sim aura SYSTEM
--   (litd/sim/aura.go), not a hand-rolled range scan. flame_aura's data carries
--   an aura block (radius, child flame_warmth, linger); a single live instance
--   of it on the wagon makes the engine maintain the warmth child on every ally
--   in radius, the carrier included, and expire it `linger` ticks after its last
--   in-range evaluation. That hands us every required behaviour for free:
--     (a) wagon death extinguishes the flame — a dead carrier has no transform,
--         so it radiates nothing and the children expire after linger (no ghost
--         light lingering on the corpse);
--     (b) walk out of radius -> the child stops refreshing -> expires after linger;
--     (c) non-stacking -- `refresh` stacking keeps ONE warmth child per ally even
--         under two overlapping wagons (armor +1, never +2);
--     (d) enemies are excluded -- the aura system is ally-team only.
--
-- So the per-tick rule is just: keep flame_aura applied to every LIVE wagon.
-- `refresh` stacking makes the re-apply idempotent (one instance, duration
-- reset), so the parent never lapses while the wagon lives and stops the instant
-- it dies. The wagon's position + count are published to Storage as the headless
-- tracking SoT. Models + the <=8 VFX light-cap interaction are the asset/render
-- half (blocked).

local WAGON = Game_UnitType("pyre_wagon")
local FLAME = Game_BuffType("flame_aura")
local store = Game_Storage()

Game_Every(0.05, function()
	local n, lit, cx, cy = 0, 0, -1, -1
	for _, u in ipairs(Game_AllUnits()) do
		if Unit_Type(u) == WAGON and Unit_Alive(u) then
			n = n + 1
			-- One live flame_aura per wagon; the engine propagates the warmth.
			-- refresh stacking => idempotent, so this never stacks the parent.
			Unit_ApplyBuff(u, FLAME)
			local pos = Unit_Position(u)
			cx, cy, lit = pos.x, pos.y, 1
		end
	end
	Storage_SetInt(store, "flame", "wagons", n)
	Storage_SetInt(store, "flame", "lit", lit)
	Storage_SetInt(store, "flame", "x", cx)
	Storage_SetInt(store, "flame", "y", cy)
end)

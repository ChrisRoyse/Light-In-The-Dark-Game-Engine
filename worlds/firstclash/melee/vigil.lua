-- Vigil race start-script (#639): returns a per-player setup function the
-- bootstrap calls. Pure content — it drives only the public melee_* / Game_*
-- Lua verbs (#637), so it validates the public API. Swap this file to re-theme
-- Vigil's opener with zero engine edits. Faction + AI strategy are TOML text
-- (the canonical data format); the bindings parse + validate them fail-closed.
local FACTION = [==[
name = "Vigil"
gold = 500
lumber = 150
food_cap = 12
town_hall = "bastion"
[workers]
code = "lamplighter"
count = 5
]==]

local STRATEGY = [==[
name = "Vigil"
[economy]
gold_workers = 5
wood_workers = 0
normal_pct = 100
[army]
soldier_type = 1
maintain = 8
[waves]
size = 6
[[build]]
type = 3
count = 1
]==]

return function(player, pspec)
	melee_StartingResources(player, FACTION)
	melee_StartingUnits(player, FACTION)
	local loc = Player_StartLocation(player)
	local hero = melee_SpawnHero(player, "beacon_warden", { x = loc.x, y = loc.y + 100.0 }, 270)

	-- #641: scripted hero auto-cast — give the warden its Battle nuke (Warden's
	-- Smite: 60 magic dmg, 600 range, 75 mana, 6s cd) and cast it at the nearest
	-- enemy in range whenever it is off cooldown with mana. Validates the public
	-- ability API end-to-end from Lua (D6). Deterministic: the 0.5s timer is
	-- tick-quantized and serializable (#464, survives save/load — #652), and
	-- Unit_CastAbility fails CLOSED when on cooldown / out of mana / out of range,
	-- so nothing fires spuriously. In the turtle opener no enemy is ever within
	-- 600 of the hero, so this stays inert until the lines actually meet.
	-- Capture only save-serializable values in the timer closure: the hero (an
	-- entity-backed unit handle), the ability REF (a plain int), and the slot
	-- (int). An Ability HANDLE must NOT be captured — it is not marshalable
	-- through the entity-backed seam, so holding it in a Game_Every callback
	-- breaks mid-match save (#652). Unit_AddAbility is idempotent (returns the
	-- existing instance), so re-deriving the handle inside the callback is free
	-- and save-safe.
	local smiteRef = Game_AbilityRef("warden_smite")
	local slot = pspec.slot
	Unit_AddAbility(hero, smiteRef) -- grant the ability once at setup
	Game_Every(0.5, function()
		if not Valid(hero) or Unit_Life(hero) <= 0 then
			return
		end
		local grp = NewGroup()
		GroupFillRadius(grp, Unit_Position(hero), 600.0, { aliveOnly = true, enemyOf = slot })
		local target = GroupFirst(grp)
		DestroyGroup(grp)
		if Valid(target) then
			Unit_CastAbility(hero, Unit_AddAbility(hero, smiteRef), target) -- no-op until ready
		end
	end)

	if pspec.controller == "cpu" then
		Game_AttachMeleeAI(player, STRATEGY, { gold_id = 0, wood_id = 1 }, pspec.difficulty)
	end
end

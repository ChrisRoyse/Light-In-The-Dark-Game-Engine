package luabind

// melee_bindings.go — hand-written Lua bindings for the data-driven melee setup
// library (litd/api/helpers/melee), the D4 keep for blizzard.j's melee BJ
// family. These are the "custom-game / mod" verbs a per-race start-script
// (melee/<race>.lua) calls so a match is authorable purely in Lua — the run
// thereby validates the public API end-to-end (#637, ultimate-test-plan §9
// Phase 1, G3).
//
// Naming follows the generated manifest convention (symbol '.'→'_'): the three
// free-function helpers are melee_StartingResources / melee_StartingUnits /
// melee_VictoryDefeatConditions, matching their bindings_gen.go descriptors so
// the Lua name cannot drift from the Go symbol. AttachMeleeAI is the Game method
// Game_AttachMeleeAI (parity with Game_AttachAI); SpawnHero is the loud,
// fail-closed hero-spawn helper melee_SpawnHero.
//
// Faction and AI-strategy data ride in as TOML TEXT (the canonical content
// format, D2) and are parsed through the existing validated loaders
// (melee.LoadFactionBytes / aimelee.LoadStrategyBytes). That reuses the helper's
// own fail-closed validation — a malformed table, an unknown key, or a negative
// resource is rejected by the loader and surfaced as a Lua error, never coerced
// to a zero value (the register.go ABI rule). No new sim mutation path: each
// binding wraps exactly one helper / api verb.

import (
	aimelee "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/ai/melee"
	melee "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api/helpers/melee"
	lua "github.com/yuin/gopher-lua"
)

// argFactionTOML reads arg i as TOML text and parses+validates it into a
// *melee.Faction, raising a Lua error (fail-closed) on read/parse/validation
// failure — including the negative-resource case the loader rejects.
func argFactionTOML(L *lua.LState, i int) *melee.Faction {
	f, err := melee.LoadFactionBytes([]byte(L.CheckString(i)))
	if err != nil {
		L.ArgError(i, err.Error())
		return nil
	}
	return f
}

// bindMeleeStartingResources: melee_StartingResources(player, factionTOML).
// Seeds the player's gold/lumber/food cap from the faction table.
func (b gameBinder) bindMeleeStartingResources(L *lua.LState) int {
	p := argPlayer(L, 1)
	f := argFactionTOML(L, 2)
	melee.StartingResources(b.g, p, f)
	return 0
}

// bindMeleeStartingUnits: melee_StartingUnits(player, factionTOML) -> {units}.
// Drops town hall + workers + extras at the player's start location. A missing
// unit-table row aborts the whole spawn (all-or-nothing) and raises a Lua error
// (fail-closed) — never a half-built base silently returned.
func (b gameBinder) bindMeleeStartingUnits(L *lua.LState) int {
	p := argPlayer(L, 1)
	f := argFactionTOML(L, 2)
	units, err := melee.StartingUnits(b.g, p, f)
	if err != nil {
		L.RaiseError("%s", err.Error())
		return 0
	}
	L.Push(handleSliceToLua(L, units))
	return 1
}

// bindMeleeVictoryDefeat: melee_VictoryDefeatConditions(players[]).
// Installs the standard last-standing victory/defeat rule over the listed
// players (re-evaluated on every unit death, plus an initial t=0 check).
func (b gameBinder) bindMeleeVictoryDefeat(L *lua.LState) int {
	players := argPlayerSlice(L, 1)
	melee.VictoryDefeatConditions(b.g, players)
	return 0
}

// argMeleeConfig reads arg i as a aimelee.Config options table (all keys
// optional, numeric). Self/Difficulty are intentionally NOT read — AttachMeleeAI
// overrides them from the player and difficulty arguments so they cannot
// disagree with the attachment.
func argMeleeConfig(L *lua.LState, i int) aimelee.Config {
	var c aimelee.Config
	t, ok := L.Get(i).(*lua.LTable)
	if !ok {
		return c
	}
	num := func(k string) (float64, bool) {
		if v, ok := t.RawGetString(k).(lua.LNumber); ok {
			return float64(v), true
		}
		return 0, false
	}
	if v, ok := num("gold_id"); ok {
		c.GoldID = int(v)
	}
	if v, ok := num("wood_id"); ok {
		c.WoodID = int(v)
	}
	if v, ok := num("gather_x"); ok {
		c.GatherX = int32(v)
	}
	if v, ok := num("gather_y"); ok {
		c.GatherY = int32(v)
	}
	if v, ok := num("enemy_x"); ok {
		c.EnemyX = int32(v)
	}
	if v, ok := num("enemy_y"); ok {
		c.EnemyY = int32(v)
	}
	if v, ok := num("formation_radius"); ok {
		c.FormationRadius = int32(v)
	}
	if v, ok := num("gather_ticks"); ok {
		c.GatherTicks = uint32(v)
	}
	return c
}

// bindAttachMeleeAI: Game_AttachMeleeAI(player, strategyTOML, configTable,
// difficulty). Loads+validates the AI strategy from TOML text, then installs the
// melee RTS controller as a live computer player. An invalid player, or a
// malformed strategy, raises a Lua error (fail-closed) — no controller is
// installed in either case.
func (b gameBinder) bindAttachMeleeAI(L *lua.LState) int {
	p := argPlayer(L, 1)
	if !p.Valid() {
		L.ArgError(1, "Game_AttachMeleeAI: player is invalid (no controller installed)")
		return 0
	}
	strat, err := aimelee.LoadStrategyBytes([]byte(L.CheckString(2)))
	if err != nil {
		L.ArgError(2, err.Error())
		return 0
	}
	cfg := argMeleeConfig(L, 3)
	d := argDifficulty(L, 4)
	b.g.AttachMeleeAI(p, strat, cfg, d)
	return 0
}

// bindSpawnHero: melee_SpawnHero(player, heroCode, pos, facing) -> Unit.
// Loud, fail-closed hero spawn: an unknown or non-hero code raises a Lua error
// and spawns nothing (unit count unchanged).
func (b gameBinder) bindSpawnHero(L *lua.LState) int {
	p := argPlayer(L, 1)
	code := L.CheckString(2)
	pos := argVec2(L, 3)
	facing := argAngle(L, 4)
	u, err := melee.SpawnHero(b.g, p, code, pos, facing)
	if err != nil {
		L.RaiseError("%s", err.Error())
		return 0
	}
	L.Push(pushHandle(L, u))
	return 1
}

// registerMelee installs the melee setup verbs. Called from Register (g != nil).
func registerMelee(L *lua.LState, b gameBinder) {
	L.SetGlobal("melee_StartingResources", L.NewFunction(b.bindMeleeStartingResources))
	L.SetGlobal("melee_StartingUnits", L.NewFunction(b.bindMeleeStartingUnits))
	L.SetGlobal("melee_VictoryDefeatConditions", L.NewFunction(b.bindMeleeVictoryDefeat))
	L.SetGlobal("Game_AttachMeleeAI", L.NewFunction(b.bindAttachMeleeAI))
	L.SetGlobal("melee_SpawnHero", L.NewFunction(b.bindSpawnHero))
}

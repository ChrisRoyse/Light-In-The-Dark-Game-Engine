package luabind

// matchspec.go — exposes a loaded match descriptor (litd/match.MatchSpec, #635)
// to Lua worlds, the Lua half of the #638 main.lua bootstrap. The HOST reads +
// validates match.toml (match.LoadMatchSpec) and calls RegisterMatchSpec; the
// world's main.lua only READS it via Game_MatchSpec() — scripts get no
// filesystem access, so this adds no sandbox surface (the same posture as
// RegisterMap, map.go). Fail-closed: a world that asks for the spec when none
// was loaded gets a loud error, never an empty/zero default.

import (
	matchpkg "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/match"
	lua "github.com/yuin/gopher-lua"
)

// RegisterMatchSpec installs Game_MatchSpec() on L. When spec is non-nil the verb
// returns the descriptor as a Lua table:
//
//	{ seed=, victory="beacon|hall|score", time_limit_ticks=,
//	  players = { {slot=, race=, controller="cpu|user", difficulty=, ai_strategy=}, ... } }
//
// players is a 1-based array ordered by slot ascending (deterministic). When spec
// is nil the verb is still installed but raises when called (a world expecting a
// match.toml that the host did not load is a bug, not an empty match). Call after
// Register.
func RegisterMatchSpec(L *lua.LState, spec *matchpkg.MatchSpec) {
	// Game_MatchSpec is host infrastructure (a Go-function global) installed per
	// world AFTER Register captured the builtin-global baseline — fold it into the
	// baseline so SaveScripts does not mistake it for world data and fail closed on
	// the unpersistable Go value (mirrors the require shim, #481).
	if s := getScheduler(L); s != nil {
		s.markBuiltinGlobal("Game_MatchSpec")
	}
	L.SetGlobal("Game_MatchSpec", L.NewFunction(func(L *lua.LState) int {
		if spec == nil {
			L.RaiseError("Game_MatchSpec: no match.toml loaded (host did not call RegisterMatchSpec)")
			return 0
		}
		t := L.CreateTable(0, 4)
		t.RawSetString("seed", lua.LNumber(spec.Seed))
		t.RawSetString("victory", lua.LString(spec.Victory.String()))
		t.RawSetString("time_limit_ticks", lua.LNumber(spec.TimeLimitTicks))
		players := L.CreateTable(len(spec.Players), 0)
		for i, p := range spec.Players {
			e := L.CreateTable(0, 5)
			e.RawSetString("slot", lua.LNumber(p.Slot))
			e.RawSetString("race", lua.LString(p.Race))
			e.RawSetString("controller", lua.LString(p.Controller.String()))
			e.RawSetString("difficulty", lua.LNumber(int(p.Difficulty)))
			e.RawSetString("ai_strategy", lua.LString(p.AIStrategy))
			players.RawSetInt(i+1, e)
		}
		t.RawSetString("players", players)
		L.Push(t)
		return 1
	}))
}

package luabind

// Runtime effect-primitive registration bindings (#477): a script registers a
// named effect/Action during setup and invokes it from a trigger Action.
//
//	RegisterEffect("lifesteal", function(src, tgt) ... end)  -- setup only
//	RunEffect("lifesteal", attacker, victim)                 -- in a trigger Action
//
// The Lua fn is a named Action (S1): it runs inside the sim phase that calls
// RunEffect, protected, and a throwing effect reports via OnScriptError without
// aborting the tick. Only the NAME hashes/serializes; the closure is re-bound by
// re-running setup on load.

import (
	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	lua "github.com/yuin/gopher-lua"
)

func registerScriptEffects(L *lua.LState, g *api.Game) {
	// RegisterEffect(name, fn) — setup-only; raises a Lua error if refused so a
	// modder sees the failure rather than a silently-absent primitive.
	L.SetGlobal("RegisterEffect", L.NewFunction(func(L *lua.LState) int {
		name := L.CheckString(1)
		fn := L.CheckFunction(2)
		err := g.RegisterEffect(name, func(inv api.EffectInvocation) {
			src := pushHandle(L, inv.Source())
			tgt := pushHandle(L, inv.Target())
			if e := L.CallByParam(lua.P{Fn: fn, NRet: 0, Protect: true}, src, tgt); e != nil {
				if s := getScheduler(L); s != nil {
					s.reportError(e)
				}
			}
		})
		if err != nil {
			L.RaiseError("RegisterEffect: %v", err)
		}
		return 0
	}))

	// RunEffect(name, source, target) -> bool — invoke a registered effect.
	L.SetGlobal("RunEffect", L.NewFunction(func(L *lua.LState) int {
		ok := g.RunEffect(L.CheckString(1), argUnit(L, 2), argUnit(L, 3))
		L.Push(lua.LBool(ok))
		return 1
	}))

	// EffectRegistered(name) -> bool
	L.SetGlobal("EffectRegistered", L.NewFunction(func(L *lua.LState) int {
		L.Push(lua.LBool(g.EffectRegistered(L.CheckString(1))))
		return 1
	}))

	// Game_EmitSpellCue(unit) -> bool — stage a one-shot spell VFX cue on the
	// non-hashing render channel (#479). A trigger Action calls this so render
	// plays an impact effect without perturbing the state hash.
	L.SetGlobal("Game_EmitSpellCue", L.NewFunction(func(L *lua.LState) int {
		L.Push(lua.LBool(g.EmitSpellCue(argUnit(L, 1))))
		return 1
	}))
}

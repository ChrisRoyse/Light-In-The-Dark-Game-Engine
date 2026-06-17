package luabind

// events.go bridges the OnEvent trigger surface to Lua (#269: "OnEvent handlers
// registered from Lua dispatch in global registration order"). It installs two
// game-bound globals:
//
//	sub = OnEvent(kind, fn)  register fn to fire on events of kind (an integer
//	                         EventKind constant); returns a Subscription handle.
//	Cancel(sub)              stop a subscription from firing further.
//
// A Lua handler is wrapped in a Go func(Event) and registered through
// Game.OnEvent, so it joins the SAME ordered subscriber list as Go handlers and
// fires in the same registration order on the sim goroutine during the event
// phase — one total order across Go and Lua (execution-model.md §2.4). The
// handler runs to completion (no in-handler wait in this v1); an error in it is
// routed through OnScriptError, never swallowed.

import (
	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	lua "github.com/yuin/gopher-lua"
)

// registerScriptEvents installs OnEvent/Cancel on L, bound to g. Called by
// Register when g != nil.
func registerScriptEvents(L *lua.LState, g *api.Game) {
	L.SetGlobal("OnEvent", L.NewFunction(func(L *lua.LState) int {
		kind := api.EventKind(uint16(L.CheckInt(1)))
		fn := L.CheckFunction(2)
		sub := g.OnEvent(kind, func(ev api.Event) {
			callEventHandler(L, fn, ev)
		})
		L.Push(handleToLua(L, sub)) // Subscription handle (pass to Cancel)
		return 1
	}))
	L.SetGlobal("Cancel", L.NewFunction(func(L *lua.LState) int {
		sub, ok := handleArg(L, 1).(api.Subscription)
		if !ok {
			L.ArgError(1, "expected Subscription userdata")
		}
		sub.Cancel()
		return 0
	}))
}

// callEventHandler invokes the Lua handler fn with the firing Event as a
// userdata argument. It runs on the sim goroutine during event dispatch (the
// host LState is otherwise idle then), protected so a handler error surfaces
// through OnScriptError instead of unwinding the sim.
func callEventHandler(L *lua.LState, fn *lua.LFunction, ev api.Event) {
	ud := L.NewUserData()
	ud.Value = ev
	if err := L.CallByParam(lua.P{Fn: fn, NRet: 0, Protect: true}, ud); err != nil {
		if s := getScheduler(L); s != nil {
			s.reportError(err)
		}
	}
}

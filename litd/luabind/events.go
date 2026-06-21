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
	"time"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	lua "github.com/yuin/gopher-lua"
)

// registerScriptEvents installs OnEvent/Cancel on L, bound to g. Called by
// Register when g != nil.
func registerScriptEvents(L *lua.LState, g *api.Game) {
	L.SetGlobal("OnEvent", L.NewFunction(func(L *lua.LState) int {
		kind := api.EventKind(uint16(L.CheckInt(1)))
		fn := L.CheckFunction(2)
		var sub api.Subscription
		if s := getScheduler(L); s != nil {
			// Route through the scheduler so the registration is recorded for
			// save/load (#433); identical trampoline either way.
			sub = registerScriptHandler(L, g, s, kind, fn)
		} else {
			sub = g.OnEvent(kind, func(ev api.Event) {
				callEventHandler(L, fn, ev)
			})
		}
		L.Push(pushHandle(L, sub)) // Subscription handle (pass to Cancel)
		return 1
	}))
	// Game_After(secs, fn) schedules fn to run once after secs of GAME time
	// (#267). The generated dispatch defers Game.After for its func() callback;
	// this binds it through the same scheduler the coroutine bridge uses
	// (g.After), with the callback's errors routed to OnScriptError — exactly the
	// OnEvent posture. Returns the Timer handle (Timer_Pause/Stop/... are bound).
	// Pending callbacks are not yet save-serializable (same #270 limit as Run).
	L.SetGlobal("Game_After", L.NewFunction(func(L *lua.LState) int {
		secs := float64(L.CheckNumber(1))
		fn := L.CheckFunction(2)
		timer := g.After(time.Duration(secs*float64(time.Second)), func() {
			if err := L.CallByParam(lua.P{Fn: fn, NRet: 0, Protect: true}); err != nil {
				if s := getScheduler(L); s != nil {
					s.reportError(err)
				}
			}
		})
		L.Push(pushHandle(L, timer))
		return 1
	}))
	// Game_Every(secs, fn) runs fn every secs of GAME time until stopped (#267).
	// Since #464 it is backed by a serializable periodic-timer Trigger, not a
	// Go-closure timer: the callback is interned by slot (registerScriptPeriodic)
	// so a mid-game save of a repeating timer round-trips (resolves the #450
	// class). It returns the backing Trigger handle (Valid() works; Timer_Stop,
	// overridden below, stops it) and passes that handle to the callback so a
	// script can stop itself — the same shape the old api.Timer presented.
	L.SetGlobal("Game_Every", L.NewFunction(func(L *lua.LState) int {
		secs := float64(L.CheckNumber(1))
		fn := L.CheckFunction(2)
		s := getScheduler(L)
		if s == nil {
			// No scheduler (no save layer): a live-only periodic, fn captured
			// directly. Cannot round-trip, but neither could the old timer.
			t := g.NewTrigger()
			t.Do(func(api.Event) { callPeriodicFn(L, nil, fn, t) })
			t.Every(time.Duration(secs * float64(time.Second)))
			L.Push(pushHandle(L, t))
			return 1
		}
		t := registerScriptPeriodic(L, g, s, secs, fn)
		L.Push(pushHandle(L, t))
		return 1
	}))

	// Timer_Stop is overridden (over the generated Timer.Stop binding) to also
	// accept a Trigger, so a Game_Every handle — now a periodic Trigger (#464) —
	// stops via the same verb scripts already use. A real api.Timer (Game_After)
	// still Stops; anything else is a loud arg error (fail-closed).
	L.SetGlobal("Timer_Stop", L.NewFunction(func(L *lua.LState) int {
		switch h := handleArg(L, 1).(type) {
		case api.Trigger:
			h.Destroy()
		case api.Timer:
			h.Stop()
		default:
			L.ArgError(1, "Timer_Stop expects a Timer or Trigger handle")
		}
		return 0
	}))
	L.SetGlobal("Cancel", L.NewFunction(func(L *lua.LState) int {
		sub, ok := handleArg(L, 1).(api.Subscription)
		if !ok {
			L.ArgError(1, "expected Subscription userdata")
		}
		sub.Cancel()
		return 0
	}))

	// OnDamage bridges the typed pre-apply damage-modifier sink (#406). Unlike
	// OnEvent (which observes a landed hit), an OnDamage handler runs DURING
	// combat resolution and receives a *DamageEvent it may read (Amount/Source/
	// Unit) and mutate (DamageEvent_SetAmount) to change the damage that lands.
	// The handle is live only inside the callback (DamageEvent.Valid), matching
	// the userdata lifetime here.
	L.SetGlobal("OnDamage", L.NewFunction(func(L *lua.LState) int {
		fn := L.CheckFunction(1)
		g.OnDamage(func(de *api.DamageEvent) {
			ud := L.NewUserData()
			ud.Value = de
			if err := L.CallByParam(lua.P{Fn: fn, NRet: 0, Protect: true}, ud); err != nil {
				if s := getScheduler(L); s != nil {
					s.reportError(err)
				}
			}
		})
		return 0
	}))
	// DamageEvent payload readers (Amount/Source/Unit are LitD new-capabilities,
	// so hand-written here; the mapped DamageEvent.SetAmount mutator is generated
	// via argDamageEvent).
	L.SetGlobal("DamageEvent_Amount", L.NewFunction(func(L *lua.LState) int {
		L.Push(lua.LNumber(argDamageEvent(L, 1).Amount()))
		return 1
	}))
	L.SetGlobal("DamageEvent_Unit", L.NewFunction(func(L *lua.LState) int {
		L.Push(pushHandle(L, argDamageEvent(L, 1).Unit()))
		return 1
	}))
	L.SetGlobal("DamageEvent_Source", L.NewFunction(func(L *lua.LState) int {
		L.Push(pushHandle(L, argDamageEvent(L, 1).Source()))
		return 1
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

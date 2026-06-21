package luabind

// trigger.go bridges the WC3 trigger editor surface to Lua (#463, ADR
// #451), on top of the public Trigger noun (#461) which itself sits on the
// sim ECA substrate (#455–#460). It installs the deduped trigger globals:
//
//	t = CreateTrigger()
//	TriggerRegisterUnitEvent(t, unit, kind)        fire on a unit's event
//	TriggerRegisterPlayerUnitEvent(t, player, kind) fire on any of a player's units
//	TriggerRegisterEnterRegion(t, region)          fire when a unit enters a region
//	TriggerRegisterTimerEvent(t, seconds)          fire every N game-seconds
//	TriggerAddCondition(t, fn)   fn(event)->bool — all conditions must pass
//	TriggerAddAction(t, fn)      fn(event)        — run when fired + conditions pass
//	EnableTrigger(t) / DisableTrigger(t)
//	TriggerExecute(t)   run actions now, bypassing events+conditions
//	TriggerEvaluate(t)  run conditions only, return their AND
//	DestroyTrigger(t)
//
// Conditions and actions are NAMED Lua functions (the script defines
// `function ownerIsP1(e) ... end` and passes it), not anonymous closures:
// each is wrapped in a Go adapter registered into the sim handler-identity
// registry (#455) under a stable per-game name, so the trigger graph is
// serializable data. A condition receives the full Event (so it can read
// the triggering unit/player via Event_* — the GetTriggerUnit idiom); it
// runs protected and a throwing condition fails closed (does not fire the
// actions). Save/load round-trip of the script-side handler table is the
// follow-up (#464); these bindings establish the live surface.

import (
	"time"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	lua "github.com/yuin/gopher-lua"
)

// triggerEventKindByName maps the deduped event names a script may name in
// TriggerRegisterUnitEvent / ...PlayerUnitEvent to public EventKinds. A
// script may also pass the integer EventKind constant directly.
var triggerEventKindByName = map[string]api.EventKind{
	"death":           api.EventUnitDeath,
	"damaged":         api.EventUnitDamaged,
	"trained":         api.EventUnitTrained,
	"orderIssued":     api.EventOrderIssued,
	"orderDone":       api.EventOrderDone,
	"heroLevel":       api.EventHeroLevel,
	"itemPickedUp":    api.EventItemPickedUp,
	"constructFinish": api.EventConstructFinished,
	"regionEnter":     api.EventRegionEnter,
	"regionLeave":     api.EventRegionLeave,
}

// argTrigger reads argument i as a Trigger handle, raising on a mismatch.
func argTrigger(L *lua.LState, i int) api.Trigger {
	t, ok := handleArg(L, i).(api.Trigger)
	if !ok {
		L.ArgError(i, "expected Trigger userdata")
	}
	return t
}

// argTriggerEventKind accepts either a string event name (e.g. "death") or
// the integer EventKind constant. An unknown name is a loud arg error
// (fail-closed) rather than a silently-never-firing trigger.
func argTriggerEventKind(L *lua.LState, i int) api.EventKind {
	if s, ok := L.Get(i).(lua.LString); ok {
		if k, ok := triggerEventKindByName[string(s)]; ok {
			return k
		}
		L.ArgError(i, "unknown trigger event name "+string(s))
	}
	return api.EventKind(L.CheckInt(i))
}

// registerScriptTriggers installs the trigger globals on L, bound to g.
// Called by Register when g != nil (alongside registerScriptEvents).
func registerScriptTriggers(L *lua.LState, g *api.Game) {
	L.SetGlobal("CreateTrigger", L.NewFunction(func(L *lua.LState) int {
		L.Push(pushHandle(L, g.NewTrigger()))
		return 1
	}))

	L.SetGlobal("DestroyTrigger", L.NewFunction(func(L *lua.LState) int {
		argTrigger(L, 1).Destroy()
		return 0
	}))

	// TriggerRegisterUnitEvent(t, unit, kind) — fire on that one unit's event
	// (a true index scope on the event source, #458).
	L.SetGlobal("TriggerRegisterUnitEvent", L.NewFunction(func(L *lua.LState) int {
		t := argTrigger(L, 1)
		u := argUnit(L, 2)
		kind := argTriggerEventKind(L, 3)
		t.On(kind, api.ForUnit(u))
		return 0
	}))

	// TriggerRegisterPlayerUnitEvent(t, player, kind) — fire on any unit owned
	// by player (compiles to an owner condition, #461).
	L.SetGlobal("TriggerRegisterPlayerUnitEvent", L.NewFunction(func(L *lua.LState) int {
		t := argTrigger(L, 1)
		p := argPlayer(L, 2)
		kind := argTriggerEventKind(L, 3)
		t.On(kind, api.OwnedBy(p))
		return 0
	}))

	// TriggerRegisterEnterRegion(t, region) — fire when a unit's source is
	// inside region (containment gate, #461).
	L.SetGlobal("TriggerRegisterEnterRegion", L.NewFunction(func(L *lua.LState) int {
		t := argTrigger(L, 1)
		r := argRegion(L, 2)
		t.On(api.EventRegionEnter, api.InRegion(r))
		return 0
	}))

	// TriggerRegisterTimerEvent(t, seconds) — fire the trigger every N game
	// seconds. A timer has no triggering unit, so it runs the trigger as
	// conditions-then-actions over an empty event (Evaluate gate + Execute),
	// matching the WC3 periodic-timer-event semantics. The Go-closure timer
	// is not yet save-serializable (the #270/#464 limit, shared with
	// Game_Every).
	L.SetGlobal("TriggerRegisterTimerEvent", L.NewFunction(func(L *lua.LState) int {
		t := argTrigger(L, 1)
		secs := float64(L.CheckNumber(2))
		g.Every(time.Duration(secs*float64(time.Second)), func(api.Timer) {
			if t.Evaluate() {
				t.Execute()
			}
		})
		return 0
	}))

	// TriggerAddCondition(t, fn) — fn(event)->bool. All conditions AND
	// together. fn is a named Lua function registered by stable identity.
	L.SetGlobal("TriggerAddCondition", L.NewFunction(func(L *lua.LState) int {
		t := argTrigger(L, 1)
		fn := L.CheckFunction(2)
		t.WhenEvent(func(ev api.Event) bool { return callLuaCondition(L, fn, ev) })
		return 0
	}))

	// TriggerAddAction(t, fn) — fn(event). Runs when the trigger fires and its
	// conditions pass; actions run in add order on the cooperative scheduler.
	L.SetGlobal("TriggerAddAction", L.NewFunction(func(L *lua.LState) int {
		t := argTrigger(L, 1)
		fn := L.CheckFunction(2)
		t.Do(func(ev api.Event) { callEventHandler(L, fn, ev) })
		return 0
	}))

	L.SetGlobal("EnableTrigger", L.NewFunction(func(L *lua.LState) int {
		argTrigger(L, 1).Enable()
		return 0
	}))
	L.SetGlobal("DisableTrigger", L.NewFunction(func(L *lua.LState) int {
		argTrigger(L, 1).Disable()
		return 0
	}))

	// TriggerExecute(t) — run the trigger's actions now, bypassing its events
	// and conditions (run-from-another-trigger).
	L.SetGlobal("TriggerExecute", L.NewFunction(func(L *lua.LState) int {
		argTrigger(L, 1).Execute()
		return 0
	}))

	// TriggerEvaluate(t) — run only the conditions and return their AND.
	L.SetGlobal("TriggerEvaluate", L.NewFunction(func(L *lua.LState) int {
		L.Push(lua.LBool(argTrigger(L, 1).Evaluate()))
		return 1
	}))
}

// registerScriptPeriodic builds a serializable periodic-timer Trigger whose
// single action invokes the Lua fn every `secs` of game time (#464). The fn is
// interned at a stable slot in the scheduler's periodicActions table so it
// joins the shared save pool (SaveScripts) and its upvalues round-trip; the
// action reads the fn back by slot at fire time (not by capture), so after a
// load-and-rebind the restored closure — with its restored upvalues — is the
// one that fires. Returns the Trigger the script holds (and can stop).
func registerScriptPeriodic(L *lua.LState, g *api.Game, s *scriptScheduler, secs float64, fn *lua.LFunction) api.Trigger {
	idx := len(s.periodicActions)
	s.periodicActions = append(s.periodicActions, fn)
	t := g.NewTrigger()
	t.Do(func(api.Event) {
		if idx < len(s.periodicActions) {
			callPeriodicFn(L, s, s.periodicActions[idx], t)
		}
	})
	t.Every(time.Duration(secs * float64(time.Second)))
	return t
}

// callPeriodicFn invokes a periodic callback with its own Trigger handle as
// the argument (the script can stop itself), protected — an error routes to
// OnScriptError, never unwinding the sim. A nil fn (a slot momentarily unbound
// during a restore window) is a no-op. s may be nil for the schedulerless
// live-only path.
func callPeriodicFn(L *lua.LState, s *scriptScheduler, fn *lua.LFunction, t api.Trigger) {
	if fn == nil {
		return
	}
	L.Push(fn)
	L.Push(pushHandle(L, t))
	if err := L.PCall(1, 0, nil); err != nil {
		if s != nil {
			s.reportError(err)
		} else if sc := getScheduler(L); sc != nil {
			sc.reportError(err)
		}
	}
}

// callLuaCondition invokes a Lua condition fn with the firing Event as a
// userdata argument and returns its boolean result. It runs on the sim
// goroutine during condition evaluation, protected so a throwing condition
// surfaces through OnScriptError and fails closed (the actions do not run)
// rather than unwinding the sim.
func callLuaCondition(L *lua.LState, fn *lua.LFunction, ev api.Event) bool {
	ud := L.NewUserData()
	ud.Value = ev
	if err := L.CallByParam(lua.P{Fn: fn, NRet: 1, Protect: true}, ud); err != nil {
		if s := getScheduler(L); s != nil {
			s.reportError(err)
		}
		return false // fail-closed: a broken condition never fires the actions
	}
	ret := L.Get(-1)
	L.Pop(1)
	return lua.LVAsBool(ret)
}

package luabind

// scriptthread.go integrates Lua coroutines with the deterministic cooperative
// scheduler (#269). It installs two globals, bound to the game:
//
//	Run(fn)           spawn fn as a cooperative script thread; it runs
//	                  immediately up to its first PolledWait or its return
//	                  (like Game.Run), then resumes on a later tick.
//	PolledWait(secs)  inside a Run body, suspend the thread for secs of GAME
//	                  time (quantized up to whole 50 ms ticks); resumes when
//	                  Game.Advance reaches the wake tick.
//
// The bridge rides the proven Go green-thread machinery (api/thread.go): a Lua
// coroutine is driven inside a Game.Run body. PolledWait yields the coroutine
// carrying the wait seconds; the driver translates that into the Go thread's
// (*Thread).PolledWait, which registers a descriptive wake record on the SAME
// stackless scheduler queue as timers and Go threads. So one script runs at a
// time and resume order is the scheduler's deterministic (wakeTick, seq) order.
//
// Persistence (S-5, thread.go note): a Lua thread parked in a wait is, for now,
// backed by a suspended Go thread, whose live stack is not serializable — so it
// does not survive a sim save yet. Making Lua threads descriptive (the VM owns
// the coroutine, the queue record carries a ContID) is #270; this is the
// scheduler-integration half (#269).

import (
	"time"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	lua "github.com/yuin/gopher-lua"
)

// registerScriptThreads installs Run/PolledWait on L, bound to g. Called by
// Register when g != nil (the threads need a game to schedule on).
func registerScriptThreads(L *lua.LState, g *api.Game) {
	L.SetGlobal("PolledWait", L.NewFunction(bindScriptPolledWait))
	L.SetGlobal("Run", L.NewFunction(func(L *lua.LState) int {
		fn := L.CheckFunction(1)
		spawnLuaThread(L, g, fn)
		return 0
	}))
}

// bindScriptPolledWait is the in-coroutine suspend native: it yields the running
// coroutine, handing the wait seconds back to the driver. It must be called from
// within a Run body (on a coroutine); calling it on the main thread yields from
// the main state, which DoString surfaces as a normal "attempt to yield" error.
func bindScriptPolledWait(L *lua.LState) int {
	secs := float64(L.CheckNumber(1))
	return L.Yield(lua.LNumber(secs))
}

// spawnLuaThread drives the Lua function fn as a cooperative thread. fn runs on
// a fresh coroutine; each PolledWait yields control back here, where the wait is
// translated into the scheduler-backed (*Thread).PolledWait. We save/restore
// G.CurrentThread around the hand-back so the host LState is current again
// whenever control returns to it (the VM does this for opcode-driven resume; a
// Go-driven Resume must do it explicitly).
func spawnLuaThread(L *lua.LState, g *api.Game, fn *lua.LFunction) {
	co, _ := L.NewThread()
	g.Run(func(t *api.Thread) {
		var args []lua.LValue
		for {
			st, _, rets := L.Resume(co, fn, args...)
			args = nil
			if st != lua.ResumeYield {
				// Finished (ResumeOK) or errored (ResumeError): the coroutine is
				// done. Restore host currency and let the Go thread retire.
				L.G.CurrentThread = L.G.MainThread
				return
			}
			// Yielded at a PolledWait: rets[0] is the wait in seconds.
			secs := 0.0
			if len(rets) > 0 {
				secs = float64(lua.LVAsNumber(rets[0]))
			}
			// Hand control back to the host while we are parked, then suspend on
			// the scheduler. PolledWait blocks until Advance reaches the wake tick.
			L.G.CurrentThread = L.G.MainThread
			t.PolledWait(time.Duration(secs * float64(time.Second)))
		}
	})
}

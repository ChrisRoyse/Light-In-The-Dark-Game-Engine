package luabind

// sched.go bridges Lua coroutines onto the deterministic cooperative scheduler
// (#269). A suspended Lua coroutine is a SCHEDULER JOB — a descriptive wake
// record on the same queue as Go timers and threads — never a blocked Go
// goroutine. Everything runs on the sim goroutine; there is no baton, no
// channel, no parked OS thread.
//
// It installs two game-bound globals (Register, when g != nil):
//
//	Run(fn)           start fn as a coroutine; it runs immediately to its first
//	                  PolledWait or its return, then resumes on a later tick.
//	PolledWait(secs)  inside a Run body, suspend for secs of GAME time (quantized
//	                  up to whole 50 ms ticks); PolledWait(0) returns without
//	                  suspending. Resumes when Game.Advance reaches the wake tick.
//
// Mechanism: Run resumes the coroutine on the sim goroutine. PolledWait calls
// L.Yield, handing the wait seconds back to resume(); resume() registers the
// continuation on the scheduler via Game.After — a descriptive record that
// fires, on the sim goroutine in phase 2, at the wake tick — and returns. When
// the record fires it resumes the coroutine again. Resume order therefore
// interleaves with every other scheduler job by the scheduler's deterministic
// (wakeTick, seq) order.
//
// Persistence (#270): the wake record currently holds a Go closure capturing the
// coroutine, which is not yet serializable — same posture as Go-closure timers
// (timer.go). Making the record hold the coroutine ref descriptively (so a
// mid-wait save round-trips via the #264 persister) is #270. This is the
// scheduler-integration half (#269): no blocked goroutine, deterministic order.

import (
	"fmt"
	"os"
	"sync"
	"time"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	lua "github.com/yuin/gopher-lua"
)

// scriptScheduler holds the per-LState script-thread state: the bound game, an
// optional error handler, and a count of coroutines currently parked on a wait
// (for the save guard / introspection). One runs at a time on the sim goroutine,
// so its fields need no locking.
type scriptScheduler struct {
	L       *lua.LState
	g       *api.Game
	errH    func(error)
	pending int
}

// schedulers maps an LState to its scriptScheduler (installed by Register).
var schedulers sync.Map // *lua.LState -> *scriptScheduler

func getScheduler(L *lua.LState) *scriptScheduler {
	if v, ok := schedulers.Load(L); ok {
		return v.(*scriptScheduler)
	}
	return nil
}

// OnScriptError installs h as the handler for errors raised inside Run bodies on
// L (including errors surfacing on a post-wait resume). nil clears it. A
// coroutine resumes asynchronously to the host, so its error cannot propagate up
// a Go return path — it is routed here; with no handler it goes to stderr, never
// swallowed (no fallback hides failure).
func OnScriptError(L *lua.LState, h func(err error)) {
	if s := getScheduler(L); s != nil {
		s.errH = h
	}
}

// PendingScriptWaits reports how many Lua coroutines are currently parked on a
// PolledWait on L (descriptive scheduler records awaiting their wake tick).
func PendingScriptWaits(L *lua.LState) int {
	if s := getScheduler(L); s != nil {
		return s.pending
	}
	return 0
}

func (s *scriptScheduler) reportError(err error) {
	if s.errH != nil {
		s.errH(err)
		return
	}
	fmt.Fprintf(os.Stderr, "luabind: uncaught script-thread error: %v\n", err)
}

// registerScriptThreads installs Run/PolledWait on L, bound to g. Called by
// Register when g != nil (the threads need a game to schedule on).
func registerScriptThreads(L *lua.LState, g *api.Game) {
	s := &scriptScheduler{L: L, g: g}
	schedulers.Store(L, s)
	L.SetGlobal("PolledWait", L.NewFunction(bindScriptPolledWait))
	L.SetGlobal("Run", L.NewFunction(func(L *lua.LState) int {
		fn := L.CheckFunction(1)
		co, _ := L.NewThread()
		s.resume(co, fn)
		return 0
	}))
}

// bindScriptPolledWait is the in-coroutine suspend native. PolledWait(d<=0)
// returns without yielding (same-tick continue, matching the Go thread semantics
// and the JASS `if duration > 0` guard); otherwise it yields the coroutine,
// handing the wait seconds back to resume().
func bindScriptPolledWait(L *lua.LState) int {
	secs := float64(L.CheckNumber(1))
	if secs <= 0 {
		return 0
	}
	return L.Yield(lua.LNumber(secs))
}

// resume drives co one slice forward: it resumes the coroutine (on the sim
// goroutine), and either finishes/errors or, on a PolledWait yield, registers a
// descriptive wake record on the scheduler (Game.After) that will call resume
// again at the wake tick. G.CurrentThread is restored to the host main thread
// after each resume (the VM does this for opcode-driven resume; a Go-driven
// Resume must do it explicitly) so the host is current whenever control returns.
func (s *scriptScheduler) resume(co *lua.LState, fn *lua.LFunction) {
	st, err, rets := s.L.Resume(co, fn)
	s.L.G.CurrentThread = s.L.G.MainThread
	if st != lua.ResumeYield {
		if st == lua.ResumeError && err != nil {
			s.reportError(err)
		}
		return
	}
	secs := 0.0
	if len(rets) > 0 {
		secs = float64(lua.LVAsNumber(rets[0]))
	}
	s.pending++
	s.g.After(time.Duration(secs*float64(time.Second)), func() {
		s.pending--
		s.resume(co, fn)
	})
}

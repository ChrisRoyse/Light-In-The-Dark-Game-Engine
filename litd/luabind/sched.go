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
	"reflect"
	"sync"
	"time"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	lua "github.com/yuin/gopher-lua"
)

// scriptScheduler holds the per-LState script-thread state: the bound game, an
// optional error handler, and a count of coroutines currently parked on a wait
// (for the save guard / introspection). One runs at a time on the sim goroutine,
// so its fields need no locking.
// scriptResumeCont is the script-owned scheduler ContID (low-numbered; the api
// reserves >=1<<30 for its own timers/threads) under which suspended Lua
// coroutines re-wake. Its State carries the coroutine's (slot, gen) by value, so
// the wake record is fully descriptive and serializes for a mid-wait save (#270).
const scriptResumeCont uint32 = 1

// scriptThread is one slot in the per-LState coroutine table. gen is bumped on
// retire so a wake record queued for a finished/recycled coroutine is detected
// as stale (mirrors api/thread.go's Go-thread table).
type scriptThread struct {
	co      *lua.LState
	fn      *lua.LFunction
	gen     uint32
	alive   bool
	waiting bool   // parked on a scheduler record (drives PendingScriptWaits)
	waitKind uint16 // 0 = timer wait (or not parked); >0 = parked on this EventKind (#413)
}

type scriptScheduler struct {
	L       *lua.LState
	g       *api.Game
	errH    func(error)
	pending int
	// threads is the coroutine table; a wake record refers back to a coroutine
	// by slot+gen (value-typed State), never by a Go closure — so it serializes.
	threads    []scriptThread
	threadFree []uint32
	// pendingWaitSecs carries the seconds a coroutine asked to wait, from the
	// PolledWait native to resume(), WITHOUT pushing it through the Lua value
	// stack (#265). Passing it as a Lua value forced an LNumber→interface box
	// every wait (1 alloc/tick/coroutine); a plain field is alloc-free. Single
	// coroutine resumes at a time on the sim goroutine, so one field suffices —
	// resume() resets it to 0 before each Resume and reads it immediately after.
	pendingWaitSecs float64
	// pendingWaitKind carries the public EventKind a coroutine asked to wait on,
	// from the WaitForEvent native to resume(), the event-wait analogue of
	// pendingWaitSecs (#413). 0 means "not an event wait" (timer wait or no wait).
	// resume() resets it to 0 before each Resume and reads it immediately after.
	pendingWaitKind uint16
	// handleCaches caches, per comparable handle type, the userdata wrapping each
	// live handle (#407). The value for type T is a map[T]*lua.LUserData; pushHandle
	// reaches it by reflect.Type key. Reuse makes a per-tick re-marshal of the same
	// handle zero-alloc (R-GC-1) — the only box is the one-time ud.Value assignment
	// on first sight. Single-threaded on the sim goroutine, so no lock.
	handleCaches map[reflect.Type]any
}

// handleCacheCap bounds each per-type sub-cache before a wholesale clear, so
// distinct-handle churn (entity recycle = new generation = new key) over a long
// match cannot leak. Sized above any realistic live working set.
const handleCacheCap = 8192

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
	s := &scriptScheduler{L: L, g: g, handleCaches: make(map[reflect.Type]any)}
	schedulers.Store(L, s)
	// Register the resume continuation once on this game's scheduler. On a save
	// load the scheduler rebuilds its registry from this call (it runs at
	// Register time, before any LoadState), so serialized wake records resolve.
	g.RegisterScriptCont(scriptResumeCont, s.onWake)
	// Register the single event-wake dispatcher at setup (before any LoadState),
	// so a restored event-wait subscription (kind → this handler) resolves (#413).
	// It receives the sim kind, which matches the sim kind stored in waitKind.
	g.RegisterScriptEventDispatcher(func(simKind uint16) { s.dispatchEvent(simKind) })
	// PolledWait(d<=0) returns without yielding (same-tick continue, matching the
	// Go thread semantics and the JASS `if duration > 0` guard); otherwise it
	// stashes the wait seconds on the scheduler and yields with NO value — the
	// host reads s.pendingWaitSecs, so nothing is boxed onto the Lua stack (#265).
	L.SetGlobal("PolledWait", L.NewFunction(func(L *lua.LState) int {
		secs := float64(L.CheckNumber(1))
		if secs <= 0 {
			return 0
		}
		s.pendingWaitSecs = secs
		return L.Yield()
	}))
	L.SetGlobal("Run", L.NewFunction(func(L *lua.LState) int {
		fn := L.CheckFunction(1)
		co, _ := L.NewThread()
		s.resume(s.alloc(co, fn))
		return 0
	}))
	// WaitForEvent(kind), inside a Run body, suspends the coroutine until the next
	// game event of EventKind kind fires, then resumes it (#413). Like PolledWait
	// it stashes its parameter on the scheduler and yields with no Lua value; resume()
	// reads s.pendingWaitKind and parks the slot on the kind instead of on a timer.
	// The coroutine inspects game state on resume to decide whether the event was the
	// one it cared about (a filter) — re-calling WaitForEvent to keep waiting. Resume
	// is driven by a descriptive scheduler wake record (dispatchEvent), so a mid-wait
	// save round-trips and the resume order stays deterministic.
	L.SetGlobal("WaitForEvent", L.NewFunction(func(L *lua.LState) int {
		kind := L.CheckInt(1)
		if kind <= 0 || kind > int(^uint16(0)) {
			L.ArgError(1, "event kind must be a positive EventKind constant")
		}
		s.pendingWaitKind = uint16(kind)
		return L.Yield()
	}))
}

// dispatchEvent wakes every coroutine parked on sim event kind, in ascending
// slot order (#413). It does NOT resume inline: it posts a descriptive wake
// record (scriptResumeCont, {slot, gen}) per waiter via AfterScript(0) — the
// resume then fires one tick later through onWake, in the scheduler's
// deterministic (wakeTick, seq) order, exactly as a PolledWait wake does. This
// keeps a restored run (which re-subscribes the dispatcher at load, in a
// different subscriber order) bit-identical to the unbroken run. A waiter's slot
// is cleared here, before its resume, so a coroutine that re-parks on the same
// kind during its resume waits for the NEXT event, never this one.
func (s *scriptScheduler) dispatchEvent(kind uint16) {
	for slot := range s.threads {
		e := &s.threads[slot]
		if !e.alive || !e.waiting || e.waitKind != kind {
			continue
		}
		// Consume the event-park (waitKind=0) so a second event of this kind before
		// the wake fires cannot re-post this slot. Leave waiting=true and pending
		// untouched: onWake is the single point that clears them and resumes — the
		// posted record converts an event-park into an ordinary 1-tick timer wake,
		// which also makes the in-flight window serialize as a plain timer wait.
		e.waitKind = 0
		s.g.AfterScript(0, scriptResumeCont, int64(slot), int64(e.gen))
	}
}

// alloc reserves a coroutine slot (recycling a retired one when available),
// fresh generations starting at 1 so a zero State never matches a live slot 0.
func (s *scriptScheduler) alloc(co *lua.LState, fn *lua.LFunction) uint32 {
	if n := len(s.threadFree); n > 0 {
		slot := s.threadFree[n-1]
		s.threadFree = s.threadFree[:n-1]
		e := &s.threads[slot]
		e.co, e.fn, e.alive, e.waiting, e.waitKind = co, fn, true, false, 0
		return slot
	}
	s.threads = append(s.threads, scriptThread{co: co, fn: fn, gen: 1, alive: true})
	return uint32(len(s.threads) - 1)
}

// retire frees a slot and bumps its generation so any outstanding wake record
// for the old identity is recognized as stale.
func (s *scriptScheduler) retire(slot uint32) {
	e := &s.threads[slot]
	e.alive, e.waiting, e.co, e.fn, e.waitKind = false, false, nil, nil, 0
	e.gen++
	if e.gen == 0 {
		e.gen = 1
	}
	s.threadFree = append(s.threadFree, slot)
}

// onWake is the scheduler callback when a parked coroutine's wake tick arrives.
// It validates the (slot, gen) against the table — the coroutine may have been
// retired and its slot reused — before resuming.
func (s *scriptScheduler) onWake(a, b int64) {
	slot, gen := uint32(a), uint32(b)
	if int(slot) >= len(s.threads) {
		return
	}
	e := &s.threads[slot]
	if !e.alive || e.gen != gen || !e.waiting {
		return // retired, reused, or superseded — stale record
	}
	e.waiting = false
	s.pending--
	s.resume(slot)
}

// resume drives co one slice forward: it resumes the coroutine (on the sim
// goroutine), and either finishes/errors or, on a PolledWait yield, registers a
// descriptive wake record on the scheduler (Game.After) that will call resume
// again at the wake tick. G.CurrentThread is restored to the host main thread
// after each resume (the VM does this for opcode-driven resume; a Go-driven
// Resume must do it explicitly) so the host is current whenever control returns.
func (s *scriptScheduler) resume(slot uint32) {
	co, fn := s.threads[slot].co, s.threads[slot].fn
	// Reset before resume; PolledWait sets it when the coroutine suspends. A
	// yield by any other means leaves it 0 (immediate re-wake) — matching the
	// prior LVAsNumber(non-number)==0 behavior, now without a boxed value (#265).
	s.pendingWaitSecs = 0
	s.pendingWaitKind = 0
	st, err, _ := s.L.Resume(co, fn)
	s.L.G.CurrentThread = s.L.G.MainThread
	if st != lua.ResumeYield {
		if st == lua.ResumeError && err != nil {
			s.reportError(err)
		}
		s.retire(slot) // done or errored — free the slot, invalidate stale records
		return
	}
	secs := s.pendingWaitSecs
	kind := s.pendingWaitKind
	// Re-fetch AFTER Resume: a nested Run() in the coroutine body may have
	// grown s.threads and reallocated its backing array, so any pointer taken
	// before the resume would now be stale.
	e := &s.threads[slot]
	e.waiting = true
	s.pending++
	if kind != 0 {
		// Event wait (#413): subscribe the dispatcher to this public kind (idempotent;
		// the sim subscription list is the source of truth, so it round-trips a
		// save/load without double-subscribing) and park the slot on the resolved SIM
		// kind. No timer record is posted — dispatchEvent posts the wake when an event
		// of the kind fires. The parked (slot→simKind) state serializes in SaveScripts.
		simKind, ok := s.g.SubscribeScriptEvent(api.EventKind(kind))
		if !ok {
			// Unknown event kind: fail closed — finish the coroutine with a loud error
			// rather than parking it forever (no fallback that hides the bad kind).
			s.pending--
			e.waiting = false
			s.reportError(fmt.Errorf("luabind: WaitForEvent(%d): unknown event kind", kind))
			s.retire(slot)
			return
		}
		e.waitKind = simKind
		return
	}
	e.waitKind = 0
	// Descriptive wake record: (scriptResumeCont, {slot, gen}) on the shared
	// stackless queue — same wake tick a Game.After(secs) timer would land on
	// (AfterScript quantizes identically), but serializable, not a Go closure.
	s.g.AfterScript(time.Duration(secs*float64(time.Second)), scriptResumeCont, int64(slot), int64(e.gen))
}

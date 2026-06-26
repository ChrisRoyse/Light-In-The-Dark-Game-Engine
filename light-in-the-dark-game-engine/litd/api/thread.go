package litd

// Cooperative script threads (#377; execution-model.md §2, R-EXEC-1/2/5).
//
// JASS "threads" are cooperative coroutines: a script runs until it
// yields at PolledWait/TriggerSleepAction, exactly one runs at a time,
// and it resumes on a later tick. LitD keeps this model. The decided
// scheduler (execution-model.md §2.3, D-2026-06-11-28) stores suspensions
// as descriptive (wakeTick, seq, ContID, State) records — never Go
// stacks — so its core has no notion of a parked imperative sequence.
//
// This surface layers an ergonomic green thread over that stackless
// scheduler using the goroutine-baton mechanism R-EXEC-1 explicitly
// permits (§2.2): "strict one-at-a-time handoff and deterministic resume
// order." A Thread owns one goroutine; the sim goroutine and the thread
// pass a single conceptual baton over unbuffered channels:
//
//	resume  (sim → thread): you hold the baton, run.
//	yielded (thread → sim): baton back — I am done, or waiting N ticks.
//
// While a thread runs, the sim goroutine blocks on <-yielded, so exactly
// one goroutine in the sim domain is runnable at any instant (S-1): no
// OS-scheduling freedom can reorder anything observable. A wait does not
// park on a channel timer — it yields the wake delay back to the sim
// goroutine, which registers a descriptive After record on the SAME
// stackless scheduler queue as timers and event waiters, so resume order
// is the scheduler's (wakeTick, seq) total order (S-2), shared with every
// other suspension.
//
// Serialization (S-5): a parked Go thread's live stack is opaque to us —
// it cannot be written into a save file. This is the SAME posture as
// Go-closure timers (timer.go): the descriptive record on the queue
// serializes, but the Go stack behind it does not, so a Go thread parked
// mid-wait does not survive a sim save/load. SaveState fails closed if
// any Go thread is suspended (see SuspendedThreadCount). Script (Lua)
// threads, when that layer lands (#269/#270), are descriptive by nature
// — the VM owns their coroutine state — and persist through their own
// registered continuations.

import (
	"sync/atomic"
	"time"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim/sched"
)

// contThreadResume is the single scheduler continuation that resumes
// waiting Go threads. Reserved just above contGoTimer (1<<30) so it can
// never collide with a script-assigned (low-numbered) ContID.
const contThreadResume sched.ContID = 1<<30 + 1

// ambientThread is the script thread currently executing, set by the sim
// goroutine immediately before it hands a thread the baton and read by
// the running thread's goroutine (the channel handoff establishes the
// happens-before, so there is no data race within one sim domain). It is
// the backing for the argument-free helpers.PolledWait, which the D4 keep
// table specifies takes only a duration: the cooperative model runs one
// thread at a time (S-1), so "the current thread" is unambiguous. Tests
// and explicit code use (*Thread).PolledWait, which needs no ambient.
var ambientThread atomic.Pointer[Thread]

// CurrentThread returns the script thread currently executing on this
// goroutine's sim domain, or nil if no thread is running. It backs the
// ambient helpers.PolledWait; gameplay code should prefer the explicit
// (*Thread).PolledWait.
func CurrentThread() *Thread { return ambientThread.Load() }

// yieldKind is what a thread hands back over the baton: either it is
// done, or it wants to wait delayTicks ticks (delayTicks >= 1).
type yieldKind struct {
	done  bool
	delay uint32
}

// Thread is a running cooperative script thread. It is a stable handle
// (a pointer kept in the Game's thread table) so a wait record can refer
// back to it by slot+generation across ticks.
type Thread struct {
	g    *Game
	slot uint32
	gen  uint32

	resume  chan struct{}
	yielded chan yieldKind
}

// threadEntry is one slot in the Game's thread table. gen is bumped on
// retire so a wait record queued for a finished/recycled thread is
// recognized as stale rather than resuming the wrong goroutine.
type threadEntry struct {
	t       *Thread
	gen     uint32
	alive   bool
	waiting bool // true while parked on a scheduler record (for the save guard)
}

// Run starts fn as a cooperative script thread. fn executes immediately,
// on the calling (sim) goroutine's behalf, up to its first wait or its
// return; control then comes back here. If fn waits, the thread resumes
// at the scheduled tick during phase 2. A nil fn or nil game is a no-op
// returning the zero Thread.
//
// Inside fn, suspend with t.PolledWait(d) (or the ambient
// helpers.PolledWait(d)); everything may have changed across a wait, so
// re-check handles with Valid() (execution-model.md §8).
func (g *Game) Run(fn func(t *Thread)) *Thread {
	if g == nil || fn == nil {
		return nil
	}
	if !g.threadContReg {
		// Lazy one-time registration of the resume continuation on this
		// game's scheduler, so a game that never spawns a thread carries
		// no scheduler entry.
		g.w.Sched.Register(contThreadResume, func(_ *sched.Scheduler, st sched.State) {
			g.resumeThread(st)
		})
		g.threadContReg = true
	}
	slot := g.allocThreadSlot()
	e := &g.threads[slot]
	t := &Thread{
		g:       g,
		slot:    slot,
		gen:     e.gen,
		resume:  make(chan struct{}),
		yielded: make(chan yieldKind),
	}
	e.t = t
	e.alive = true
	e.waiting = false

	go func() {
		<-t.resume // wait for the first baton
		fn(t)
		t.yielded <- yieldKind{done: true}
	}()

	g.driveThread(t)
	return t
}

// driveThread hands t the baton and blocks until it yields back, then
// either retires it (done) or registers its wake record. Runs only on
// the sim goroutine. It sets/restores the ambient thread around the
// handoff so the argument-free PolledWait resolves to t, and nests
// correctly when a thread spawns another thread.
func (g *Game) driveThread(t *Thread) {
	prev := ambientThread.Load()
	ambientThread.Store(t)
	t.resume <- struct{}{}
	k := <-t.yielded
	ambientThread.Store(prev)

	if int(t.slot) >= len(g.threads) {
		return
	}
	e := &g.threads[t.slot]
	if !e.alive || e.gen != t.gen {
		return // retired during the slice (cannot normally happen)
	}
	if k.done {
		g.retireThread(t.slot)
		return
	}
	// Suspend: register a descriptive wake record on the shared stackless
	// queue. delay >= 1 (PolledWait quantizes), so it cannot fire on the
	// tick that created it — the phase-2 drain loop always terminates.
	e.waiting = true
	g.suspendedThreads++
	g.w.Sched.After(k.delay, contThreadResume, sched.State{int64(t.slot), int64(t.gen)})
}

// resumeThread is the scheduler callback when a thread's wake tick
// arrives. It validates the record against the table (the thread may
// have been retired and its slot reused) before re-handing the baton.
func (g *Game) resumeThread(st sched.State) {
	slot := uint32(st[0])
	gen := uint32(st[1])
	if int(slot) >= len(g.threads) {
		return
	}
	e := &g.threads[slot]
	if !e.alive || e.gen != gen || !e.waiting {
		return // retired, reused, or superseded — stale record
	}
	e.waiting = false
	g.suspendedThreads--
	g.driveThread(e.t)
}

// allocThreadSlot returns a recycled retired slot or grows the table.
// Fresh entries start at generation 1 so a zero-value handle never
// matches a live slot 0.
func (g *Game) allocThreadSlot() uint32 {
	if n := len(g.threadFree); n > 0 {
		slot := g.threadFree[n-1]
		g.threadFree = g.threadFree[:n-1]
		return slot
	}
	g.threads = append(g.threads, threadEntry{gen: 1})
	return uint32(len(g.threads) - 1)
}

// retireThread frees a slot and bumps its generation so any outstanding
// handle or queued record for the old identity is invalid.
func (g *Game) retireThread(slot uint32) {
	e := &g.threads[slot]
	e.alive = false
	e.waiting = false
	e.t = nil
	e.gen++
	if e.gen == 0 {
		e.gen = 1
	}
	g.threadFree = append(g.threadFree, slot)
}

// SuspendedThreadCount returns how many Go script threads are currently
// parked on a wait. The sim save path consults this to fail closed: a
// parked Go thread's stack is not serializable (see the package note and
// timer.go), so a save attempted with Go threads suspended must error
// rather than silently drop them. Zero in the common case (threads that
// only wait during one-shot setup have already resumed and finished).
func (g *Game) SuspendedThreadCount() int {
	if g == nil {
		return 0
	}
	return g.suspendedThreads
}

// PolledWait suspends the thread for d of game time, resuming on a later
// tick (R-EXEC-5: durations quantize UP to whole 50 ms ticks, one-tick
// floor). d <= 0 returns immediately without suspending — no record is
// created — matching JASS's `if duration > 0` guard in PolledWait. Must
// be called from within the thread's own fn (on its goroutine); calling
// it on a Thread other than the running one would deadlock and is a bug.
func (t *Thread) PolledWait(d time.Duration) {
	if t == nil {
		return
	}
	ticks := waitTicks(d)
	if ticks == 0 {
		return // d <= 0: same-tick continue, no suspension record
	}
	// Hand the baton back with the wake delay; the sim goroutine registers
	// the record and resumes us when the tick arrives.
	t.yielded <- yieldKind{delay: ticks}
	<-t.resume
}

// Valid reports whether the thread is still live (not yet finished).
func (t *Thread) Valid() bool {
	if t == nil || t.g == nil || int(t.slot) >= len(t.g.threads) {
		return false
	}
	e := &t.g.threads[t.slot]
	return e.alive && e.gen == t.gen
}

// waitTicks quantizes a wait duration to whole ticks: 0 for d <= 0
// (immediate, JASS guard), else ceiling with a one-tick floor (§3). This
// differs from durationToTicks (timers), which floors a zero/negative
// duration to one tick — a timer always fires, a wait of 0 does not
// suspend.
func waitTicks(d time.Duration) uint32 {
	if d <= 0 {
		return 0
	}
	return durationToTicks(d) // d > 0 ⇒ >= 1
}

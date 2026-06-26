package ai

// AI thread/timing natives (#275; jass-mapping/ai-natives.md sleep/wait family;
// execution-model.md §3 R-EXEC-5, §6). The common.ai Sleep / StartThread /
// wait-for-signal family, implemented canonically on the AI domain scheduler —
// every wait is tick-quantized exactly like a map-script wait, so the WC3 "AI
// thread sleeps real seconds" replay-divergence hazard is closed by construction.
//
// Threading model. The AI domain scheduler is stackless: a thread is a
// continuation that re-arms itself. A thread therefore expresses a wait not by
// blocking mid-function (there is no stack to park) but by computing the
// quantized delay and re-arming for that many ticks; when its body returns
// without re-arming, the thread has ended and its slot is freed. The eventual
// Lua surface (§3.5) layers true coroutines on top of exactly these primitives.
//
// Thread cap (documented decision). WC3 capped AI at 6 threads per player; the
// manifest (ai-natives.md) says the port "honors that isolation natively." We
// KEEP the cap: a bounded thread count is good hygiene under the determinism +
// per-tick-slice-budget regime (#272), and lifting it would diverge from the
// documented R-EXEC-3 model for no product reason. The 7th concurrent spawn is
// refused deterministically with a loud diagnostic (never a silent drop, never
// unbounded growth).

import (
	"fmt"
	"io"
	"math"
	"os"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim/sched"
)

// TickMS is the sim tick in milliseconds (R-EXEC-5). Mirrors litd/data.TickMS
// and litd/sim's tick rate; a drift-guard test asserts it stays equal so this
// local copy can never silently diverge from the canonical value.
const TickMS = 50

// MaxThreadsPerPlayer is the WC3 cap, kept (see the package decision above).
const MaxThreadsPerPlayer = 6

// QuantizeSleep converts a real-seconds AI Sleep into whole AI-domain ticks,
// ceiling onto the 50 ms grid: a positive sub-tick duration becomes 1 tick
// (never 0, so a wait is never shorter than written), and a duration <= 0 (or
// NaN) becomes 0 — the WC3 "Sleep(0)" yield, which produces no suspension record
// because the thread simply continues in-line rather than parking. This is the
// only place AI wait durations meet the tick grid; once quantized, everything
// downstream is integer ticks and bit-identical on every machine.
func QuantizeSleep(seconds float64) uint32 {
	if math.IsNaN(seconds) || seconds <= 0 {
		return 0
	}
	ms := int64(math.Round(seconds * 1000))
	if ms <= 0 {
		return 0
	}
	t := (ms + TickMS - 1) / TickMS
	if t < 1 {
		t = 1
	}
	return uint32(t)
}

// ThreadID identifies one AI thread within a player's ThreadSet.
type ThreadID uint32

// thread is one AI thread's slot. cont is its dedicated scheduler continuation;
// live is false once the thread ends (or is killed); armed records whether the
// current resume re-armed the thread (re-armed ⇒ still live).
type thread struct {
	id    ThreadID
	cont  sched.ContID
	live  bool
	armed bool
}

// ThreadSet manages one AI player's threads on its context scheduler: spawn
// (cap-enforced), re-arm (Sleep/WaitSignal), and liveness accounting. Thread
// continuations are registered at base + ThreadID, a stable mapping so a
// suspended thread serializes and restores like any other suspension.
type ThreadSet struct {
	ctx     *Context
	base    sched.ContID
	threads []*thread // dense, in spawn order
	nextID  ThreadID
	active  int // live thread count
	refused int // spawns refused past the cap
	diag    io.Writer
}

// NewThreadSet binds a ThreadSet to ctx. baseCont is the first continuation id
// it owns; thread i uses baseCont+i, so baseCont must be reserved clear of any
// other continuation the controller registers on this context.
func NewThreadSet(ctx *Context, baseCont sched.ContID) *ThreadSet {
	return &ThreadSet{ctx: ctx, base: baseCont, diag: os.Stderr}
}

// SetDiagnostics redirects the cap-refusal diagnostic (default os.Stderr); nil
// silences it. The refused counter is maintained regardless.
func (ts *ThreadSet) SetDiagnostics(w io.Writer) { ts.diag = w }

// Active returns the number of live threads.
func (ts *ThreadSet) Active() int { return ts.active }

// Refused returns how many spawns were refused for exceeding the cap.
func (ts *ThreadSet) Refused() int { return ts.refused }

func (ts *ThreadSet) byID(id ThreadID) *thread {
	for _, t := range ts.threads {
		if t.id == id {
			return t
		}
	}
	return nil
}

// wrap builds the per-thread scheduler continuation: it runs the body, then
// frees the slot if the body returned without re-arming (Sleep>0 / WaitSignal),
// and ignores resumes that arrive for an already-ended thread (a stale record
// left by Kill is a no-op, never a resurrection).
func (ts *ThreadSet) wrap(th *thread, body sched.Func) sched.Func {
	return func(s *sched.Scheduler, st sched.State) {
		if !th.live {
			return
		}
		th.armed = false
		body(s, st)
		if !th.armed && th.live {
			th.live = false
			ts.active--
		}
	}
}

// Spawn starts a new AI thread running body, kicked off on the next tick. It is
// the StartThread / SetStartingThread analogue. Returns the new ThreadID and
// true, or (0, false) if the player already has MaxThreadsPerPlayer live
// threads — in which case the spawn is refused deterministically and a loud
// diagnostic is emitted (the 7th-thread rule). The body re-arms via this
// ThreadSet's Sleep/Continue/WaitSignal; returning without re-arming ends it.
func (ts *ThreadSet) Spawn(body sched.Func, st sched.State) (ThreadID, bool) {
	if ts.active >= MaxThreadsPerPlayer {
		ts.refused++
		if ts.diag != nil {
			fmt.Fprintf(ts.diag,
				"ai: THREAD CAP player=%d already has %d live threads (max %d) — "+
					"refused StartThread (total refused=%d)\n",
				ts.ctx.Player(), ts.active, MaxThreadsPerPlayer, ts.refused)
		}
		return 0, false
	}
	id := ts.nextID
	ts.nextID++
	th := &thread{id: id, cont: ts.base + sched.ContID(id), live: true}
	ts.threads = append(ts.threads, th)
	ts.active++
	ts.ctx.Register(th.cont, ts.wrap(th, body))
	ts.ctx.After(1, th.cont, st) // first resume next tick
	return id, true
}

// Continue re-arms thread id to resume after delayTicks ticks (the scheduler
// floors 0 to 1, so this never wakes on the arming tick). Use it after a
// positive Sleep; a body that does not Continue/WaitSignal before returning
// ends the thread.
func (ts *ThreadSet) Continue(id ThreadID, delayTicks uint32, st sched.State) {
	th := ts.byID(id)
	if th == nil || !th.live {
		return
	}
	th.armed = true
	ts.ctx.After(delayTicks, th.cont, st)
}

// Sleep is the AI Sleep verb: it returns the quantized tick delay for seconds
// and, when that delay is positive, re-arms thread id to wake after it. A
// delay of 0 (seconds <= 0) creates no suspension record and does not re-arm —
// the thread continues in-line, exactly the WC3 Sleep(0) yield. Returns the
// delay so the body can branch on it.
func (ts *ThreadSet) Sleep(id ThreadID, seconds float64, st sched.State) uint32 {
	d := QuantizeSleep(seconds)
	if d > 0 {
		ts.Continue(id, d, st)
	}
	return d
}

// WaitSignal parks thread id until ev next fires on this context (Signal). It
// is the wait-for-signal verb; the thread re-arms by waiting, so it stays live.
func (ts *ThreadSet) WaitSignal(id ThreadID, ev sched.EventID, st sched.State) {
	th := ts.byID(id)
	if th == nil || !th.live {
		return
	}
	th.armed = true
	ts.ctx.WaitEvent(ev, th.cont, st)
}

// Signal fires ev, resuming every thread parked on it (FIFO by arm order).
func (ts *ThreadSet) Signal(ev sched.EventID) { ts.ctx.FireEvent(ev) }

// Kill ends thread id immediately and frees its slot. A suspension already in
// the queue for it becomes a no-op resume (wrap guards on live), so Kill is
// safe even mid-wait. No-op on an unknown or already-dead thread.
func (ts *ThreadSet) Kill(id ThreadID) {
	th := ts.byID(id)
	if th == nil || !th.live {
		return
	}
	th.live = false
	ts.active--
}

// Reinstall re-registers a thread's continuation after a save/restore rebuild,
// WITHOUT kicking off a fresh run: the suspended record restored by Domain.Load
// references base+id and must resolve to body. id must match the saved thread's
// id and body must be the same logic. Marks the thread live and counts it
// active. Call once per saved thread, in the original spawn order, before
// Domain.Load. Panics if id was already installed.
func (ts *ThreadSet) Reinstall(id ThreadID, body sched.Func) {
	if ts.byID(id) != nil {
		panic(fmt.Sprintf("ai: Reinstall of already-installed thread %d", id))
	}
	th := &thread{id: id, cont: ts.base + sched.ContID(id), live: true}
	ts.threads = append(ts.threads, th)
	ts.active++
	if id >= ts.nextID {
		ts.nextID = id + 1
	}
	ts.ctx.Register(th.cont, ts.wrap(th, body))
}

// Package sched is the deterministic cooperative script scheduler
// (tick-and-scheduler.md §3.1, THE design per D-28; R-EXEC-1/2/5,
// R-SIM-6 readiness).
//
// A suspended script is data, not a stack: a record
// (wakeTick, seq, continuation ID, value-typed state) in the sleep
// queue. Continuations are registered, stably-identified functions —
// never bare Go closures — so every record is serializable by
// construction. Timers are entries in the same queue. Exactly one
// suspension runs at a time, on the caller's goroutine: no goroutines,
// no channels, nothing the Go runtime can reorder (R-EXEC-1).
package sched

// ContID stably identifies a registered continuation. IDs are assigned
// by the script host at registration and must be identical across
// runs and builds — they go into the save format (R-SIM-6).
type ContID uint32

// EventID identifies an event channel waiters can suspend on.
type EventID uint32

// State is the value-typed continuation payload carried by a
// suspension record. Fixed size, no pointers: it serializes directly
// and never anchors heap garbage.
type State [4]int64

// Func is a registered continuation. It runs synchronously on the
// scheduler's thread; to suspend again it calls After or WaitEvent,
// otherwise the script is done. It must never retain s past return.
type Func func(s *Scheduler, st State)

// record is one suspension in the sleep queue, keyed (wakeTick, seq).
type record struct {
	wakeTick uint32
	seq      uint32
	cont     ContID
	state    State
}

// waiter is one suspension parked on an event, FIFO by seq.
type waiter struct {
	seq   uint32
	cont  ContID
	state State
}

// eventWaiters is the waiter list for one event. The scheduler keeps
// these in a slice sorted by ev — not a map — so hashing and
// serialization walk them in deterministic order with no map
// iteration anywhere (R-SIM-2 discipline, #97 encoder constraint).
type eventWaiters struct {
	ev   EventID
	list []waiter
}

// Scheduler owns all suspended script state. Not safe for concurrent
// use — by design it must only ever be touched from the sim goroutine.
type Scheduler struct {
	now     uint32
	nextSeq uint32
	conts   map[ContID]Func // lookup only, never iterated, never serialized
	sleep   []record        // binary min-heap on (wakeTick, seq)
	waiters []eventWaiters  // sorted ascending by ev

	// listPool recycles waiter-slice backing arrays between FireEvent
	// dispatches (R-GC-2): at steady state no dispatch allocates. Pure
	// memory reuse — never serialized, never observable in resume order.
	listPool [][]waiter
}

// New returns an empty scheduler at tick 0.
func New() *Scheduler {
	return &Scheduler{
		conts: make(map[ContID]Func),
	}
}

// waiterIdx binary-searches s.waiters for ev. Returns the index and
// whether it exists; if not, the index is the sorted insertion point.
func (s *Scheduler) waiterIdx(ev EventID) (int, bool) {
	lo, hi := 0, len(s.waiters)
	for lo < hi {
		mid := (lo + hi) / 2
		if s.waiters[mid].ev < ev {
			lo = mid + 1
		} else {
			hi = mid
		}
	}
	return lo, lo < len(s.waiters) && s.waiters[lo].ev == ev
}

// Register binds id to fn. Call only during sim construction, in
// deterministic order. Panics on duplicate or nil — a half-registered
// continuation table must fail loudly, not resume the wrong code.
func (s *Scheduler) Register(id ContID, fn Func) {
	if fn == nil {
		panic("sched: Register with nil func")
	}
	if _, dup := s.conts[id]; dup {
		panic("sched: duplicate ContID registration")
	}
	s.conts[id] = fn
}

// Now returns the current tick.
func (s *Scheduler) Now() uint32 { return s.now }

// After suspends: cont resumes with st after delayTicks ticks.
// Durations quantize up to whole ticks before reaching here; delay 0
// means "resume next tick" (R-EXEC-5) — a record can never wake on the
// tick that created it, so the drain loop cannot self-feed.
// Panics on an unregistered ContID (fail-closed: a dangling reference
// would otherwise sit in the queue until resume time, possibly past a
// save).
func (s *Scheduler) After(delayTicks uint32, cont ContID, st State) {
	if _, ok := s.conts[cont]; !ok {
		panic("sched: After with unregistered ContID")
	}
	if delayTicks == 0 {
		delayTicks = 1
	}
	s.nextSeq++
	s.push(record{
		wakeTick: s.now + delayTicks,
		seq:      s.nextSeq,
		cont:     cont,
		state:    st,
	})
}

// WaitEvent suspends: cont resumes with st when ev next fires. Waiters
// are FIFO by seq within an event. Panics on an unregistered ContID.
func (s *Scheduler) WaitEvent(ev EventID, cont ContID, st State) {
	if _, ok := s.conts[cont]; !ok {
		panic("sched: WaitEvent with unregistered ContID")
	}
	s.nextSeq++
	i, ok := s.waiterIdx(ev)
	if !ok {
		s.waiters = append(s.waiters, eventWaiters{})
		copy(s.waiters[i+1:], s.waiters[i:])
		s.waiters[i] = eventWaiters{ev: ev}
	}
	s.waiters[i].list = append(s.waiters[i].list, waiter{seq: s.nextSeq, cont: cont, state: st})
}

// FireEvent resumes every waiter currently parked on ev, in FIFO (seq)
// order. The waiter list is snapshotted first: a handler that re-waits
// mid-dispatch lands in a fresh list with a new seq, behind everyone
// in this dispatch, and cannot disturb the remaining order (R-EXEC-2).
func (s *Scheduler) FireEvent(ev EventID) {
	i, ok := s.waiterIdx(ev)
	if !ok || len(s.waiters[i].list) == 0 {
		return
	}
	fired := s.waiters[i].list
	s.waiters[i].list = s.grabList() // re-waits land here, behind this dispatch
	for j := range fired {
		s.run(fired[j].cont, fired[j].state)
	}
	s.releaseList(fired)
}

// grabList returns an empty waiter slice, reusing pooled capacity.
func (s *Scheduler) grabList() []waiter {
	if n := len(s.listPool); n > 0 {
		l := s.listPool[n-1]
		s.listPool = s.listPool[:n-1]
		return l
	}
	return nil
}

// releaseList recycles a dispatched waiter list. Contents are zeroed
// first — a recycled slot must never leak a previous waiter's seq,
// ContID, or state into anything (pool-reset discipline, R-GC-2).
func (s *Scheduler) releaseList(l []waiter) {
	for i := range l {
		l[i] = waiter{}
	}
	s.listPool = append(s.listPool, l[:0])
}

// Step advances one tick and resumes every suspension whose wakeTick
// has arrived, strictly ascending by (wakeTick, seq). Suspensions
// pushed during the drain wake next tick at the earliest (After
// quantizes up), so the loop always terminates.
func (s *Scheduler) Step() {
	s.now++
	for len(s.sleep) > 0 && s.sleep[0].wakeTick <= s.now {
		r := s.pop()
		s.run(r.cont, r.state)
	}
}

func (s *Scheduler) run(cont ContID, st State) {
	fn := s.conts[cont]
	if fn == nil {
		// Unregistered ID inside the queue means corrupted or
		// version-skewed state — resuming arbitrary other code would
		// silently desync. Fail closed.
		panic("sched: suspension references unregistered ContID")
	}
	fn(s, st)
}

// PendingSleepers returns how many suspensions sit in the sleep queue.
func (s *Scheduler) PendingSleepers() int { return len(s.sleep) }

// PendingWaiters returns how many waiters are parked on ev.
func (s *Scheduler) PendingWaiters(ev EventID) int {
	if i, ok := s.waiterIdx(ev); ok {
		return len(s.waiters[i].list)
	}
	return 0
}

// --- sleep queue: binary min-heap on (wakeTick, seq) ----------------------

func recordLess(a, b record) bool {
	if a.wakeTick != b.wakeTick {
		return a.wakeTick < b.wakeTick
	}
	return a.seq < b.seq
}

func (s *Scheduler) push(r record) {
	s.sleep = append(s.sleep, r)
	i := len(s.sleep) - 1
	for i > 0 {
		p := (i - 1) / 2
		if !recordLess(s.sleep[i], s.sleep[p]) {
			break
		}
		s.sleep[i], s.sleep[p] = s.sleep[p], s.sleep[i]
		i = p
	}
}

func (s *Scheduler) pop() record {
	top := s.sleep[0]
	last := len(s.sleep) - 1
	s.sleep[0] = s.sleep[last]
	s.sleep = s.sleep[:last]
	i := 0
	for {
		l, r := 2*i+1, 2*i+2
		min := i
		if l < len(s.sleep) && recordLess(s.sleep[l], s.sleep[min]) {
			min = l
		}
		if r < len(s.sleep) && recordLess(s.sleep[r], s.sleep[min]) {
			min = r
		}
		if min == i {
			break
		}
		s.sleep[i], s.sleep[min] = s.sleep[min], s.sleep[i]
		i = min
	}
	return top
}

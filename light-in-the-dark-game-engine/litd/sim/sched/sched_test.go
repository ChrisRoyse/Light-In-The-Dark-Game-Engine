package sched

import (
	"fmt"
	"strings"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/statehash"
)

// Test continuations are registered named funcs operating on a trace
// env — the registry pattern production scripts use. State[0] carries
// the script's identity so traces show who ran when.

type env struct {
	trace []string
}

func (e *env) log(format string, a ...any) {
	e.trace = append(e.trace, fmt.Sprintf(format, a...))
}

const (
	evPing EventID = 1
)

const (
	contNote     ContID = iota + 1 // logs and stops
	contChain                      // logs, waits 2 ticks, then contNote
	contRewait                     // logs, immediately re-waits on evPing
	contWaitZero                   // logs, After(0) -> contNote
)

func newTestSched(e *env) *Scheduler {
	s := New()
	s.Register(contNote, func(s *Scheduler, st State) {
		e.log("t%02d note id=%d", s.Now(), st[0])
	})
	s.Register(contChain, func(s *Scheduler, st State) {
		e.log("t%02d chain id=%d", s.Now(), st[0])
		s.After(2, contNote, st)
	})
	s.Register(contRewait, func(s *Scheduler, st State) {
		e.log("t%02d rewait id=%d", s.Now(), st[0])
		s.WaitEvent(evPing, contRewait, st)
	})
	s.Register(contWaitZero, func(s *Scheduler, st State) {
		e.log("t%02d waitzero id=%d", s.Now(), st[0])
		s.After(0, contNote, st)
	})
	return s
}

func dumpTrace(t *testing.T, name string, trace []string) {
	t.Logf("--- trace %s ---", name)
	for _, line := range trace {
		t.Logf("  %s", line)
	}
}

// Edge: two scripts waking the same tick resume in seq (insertion)
// order, even when pushed with different delays from different ticks.
func TestSameTickResumesInSeqOrder(t *testing.T) {
	e := &env{}
	s := newTestSched(e)
	s.After(3, contNote, State{1}) // seq 1, wakes tick 3
	s.After(3, contNote, State{2}) // seq 2, wakes tick 3
	s.Step()                       // tick 1
	s.After(2, contNote, State{3}) // seq 3, wakes tick 3 — pushed later, same tick
	for i := 0; i < 3; i++ {
		s.Step()
	}
	dumpTrace(t, "same-tick seq order", e.trace)
	want := []string{"t03 note id=1", "t03 note id=2", "t03 note id=3"}
	if fmt.Sprint(e.trace) != fmt.Sprint(want) {
		t.Fatalf("resume order wrong:\ngot  %v\nwant %v", e.trace, want)
	}
}

// Edge: a resuming script pushing a wait with delay 0 resumes NEXT
// tick, never the current one — R-EXEC-5 "Wait(0) means resume next
// tick"; the drain loop must not self-feed.
func TestWaitZeroResumesNextTick(t *testing.T) {
	e := &env{}
	s := newTestSched(e)
	s.After(1, contWaitZero, State{7})
	s.Step() // tick 1: waitzero runs, schedules After(0)
	if got := len(e.trace); got != 1 {
		t.Fatalf("after tick 1 expected 1 trace entry, got %d: %v", got, e.trace)
	}
	s.Step() // tick 2: the After(0) continuation runs
	dumpTrace(t, "Wait(0) next-tick", e.trace)
	want := []string{"t01 waitzero id=7", "t02 note id=7"}
	if fmt.Sprint(e.trace) != fmt.Sprint(want) {
		t.Fatalf("Wait(0) semantics wrong:\ngot  %v\nwant %v", e.trace, want)
	}
}

// Edge: event with 3 waiters where #2 re-waits mid-dispatch. Dispatch
// must still reach #3 in FIFO order; the re-wait lands behind with a
// new seq and only fires on the NEXT FireEvent.
func TestEventRewaitMidDispatch(t *testing.T) {
	e := &env{}
	s := newTestSched(e)
	s.WaitEvent(evPing, contNote, State{1})   // waiter 1
	s.WaitEvent(evPing, contRewait, State{2}) // waiter 2: re-waits immediately
	s.WaitEvent(evPing, contNote, State{3})   // waiter 3
	s.FireEvent(evPing)
	mid := len(e.trace)
	if got := s.PendingWaiters(evPing); got != 1 {
		t.Fatalf("after first fire: %d waiters parked, want 1 (the re-wait)", got)
	}
	s.FireEvent(evPing) // only the re-armed #2 fires
	dumpTrace(t, "re-wait mid-dispatch", e.trace)
	want := []string{
		"t00 note id=1", "t00 rewait id=2", "t00 note id=3", // first dispatch, FIFO
		"t00 rewait id=2", // second dispatch: re-armed waiter alone
	}
	if fmt.Sprint(e.trace) != fmt.Sprint(want) {
		t.Fatalf("dispatch order wrong:\ngot  %v\nwant %v", e.trace, want)
	}
	if mid != 3 {
		t.Fatalf("first dispatch ran %d handlers, want 3", mid)
	}
	// the re-armed waiter re-waited again during the second fire
	if got := s.PendingWaiters(evPing); got != 1 {
		t.Fatalf("after second fire: %d waiters, want 1", got)
	}
}

// Timers and script waits share one queue and interleave by seq.
func TestTimersShareQueueWithWaits(t *testing.T) {
	e := &env{}
	s := newTestSched(e)
	s.After(2, contNote, State{10})  // "timer" seq 1
	s.After(2, contChain, State{20}) // script wait seq 2, chains +2
	s.After(2, contNote, State{30})  // "timer" seq 3
	for i := 0; i < 4; i++ {
		s.Step()
	}
	dumpTrace(t, "timers + waits one queue", e.trace)
	want := []string{
		"t02 note id=10", "t02 chain id=20", "t02 note id=30",
		"t04 note id=20",
	}
	if fmt.Sprint(e.trace) != fmt.Sprint(want) {
		t.Fatalf("interleave wrong:\ngot  %v\nwant %v", e.trace, want)
	}
}

// Heap pops strictly ascending (wakeTick, seq) under adversarial
// insertion order: delays pushed long-first.
func TestSleepQueueStrictAscending(t *testing.T) {
	e := &env{}
	s := newTestSched(e)
	delays := []uint32{9, 1, 5, 3, 9, 1, 7, 5, 3, 7}
	for i, d := range delays {
		s.After(d, contNote, State{int64(i)})
	}
	for i := 0; i < 10; i++ {
		s.Step()
	}
	dumpTrace(t, "heap ascending", e.trace)
	if len(e.trace) != len(delays) {
		t.Fatalf("ran %d, want %d", len(e.trace), len(delays))
	}
	// reconstruct (wakeTick, seq) per trace line and assert ascending
	prevKey := [2]int{-1, -1}
	for _, line := range e.trace {
		var tick, id int
		fmt.Sscanf(line, "t%02d note id=%d", &tick, &id)
		key := [2]int{tick, id + 1} // seq == insertion index + 1
		if delays[id] != uint32(tick) {
			t.Fatalf("%s: woke at %d, scheduled delay %d", line, tick, delays[id])
		}
		if key[0] < prevKey[0] || (key[0] == prevKey[0] && key[1] <= prevKey[1]) {
			t.Fatalf("not strictly ascending: %v after %v", key, prevKey)
		}
		prevKey = key
	}
}

// Edge: two full runs from identical setup produce byte-identical
// traces — hashed with litd/statehash and printed.
func TestByteIdenticalTraces(t *testing.T) {
	run := func() []string {
		e := &env{}
		s := newTestSched(e)
		for i := 0; i < 6; i++ {
			s.After(uint32(i%3)+1, contChain, State{int64(i)})
			s.WaitEvent(evPing, contNote, State{int64(100 + i)})
		}
		for tick := 0; tick < 20; tick++ {
			s.Step()
			if s.Now()%7 == 0 {
				s.FireEvent(evPing)
			}
		}
		return e.trace
	}
	a, b := run(), run()

	hash := func(tr []string) uint64 {
		h := statehash.New()
		for _, line := range tr {
			h.WriteBytes([]byte(line))
			h.WriteU8('\n')
		}
		return h.Sum64()
	}
	ha, hb := hash(a), hash(b)
	dumpTrace(t, "run A", a)
	t.Logf("run A: %d entries, trace hash 0x%016X", len(a), ha)
	t.Logf("run B: %d entries, trace hash 0x%016X", len(b), hb)
	if strings.Join(a, "\n") != strings.Join(b, "\n") {
		t.Fatal("traces differ between identical runs")
	}
	if ha != hb {
		t.Fatal("trace hashes differ")
	}
}

// Fail-closed: scheduling against an unregistered continuation panics
// at suspension time, not resume time.
func TestUnregisteredContIDPanics(t *testing.T) {
	for name, f := range map[string]func(*Scheduler){
		"After":     func(s *Scheduler) { s.After(1, ContID(999), State{}) },
		"WaitEvent": func(s *Scheduler) { s.WaitEvent(evPing, ContID(999), State{}) },
	} {
		func() {
			defer func() {
				if recover() == nil {
					t.Fatalf("%s with unregistered ContID did not panic", name)
				}
			}()
			f(New())
		}()
	}
}

func TestDuplicateRegisterPanics(t *testing.T) {
	defer func() {
		if recover() == nil {
			t.Fatal("duplicate Register did not panic")
		}
	}()
	s := New()
	s.Register(contNote, func(*Scheduler, State) {})
	s.Register(contNote, func(*Scheduler, State) {})
}

// #557 — a transient cont's records run live but are omitted from Save,
// so a blob never references a save-unsafe continuation. SoT = the byte
// length (SaveSize matches Save) and that Load succeeds without the
// transient cont registered.
func TestTransientContSkippedBySave(t *testing.T) {
	s := New()
	const persistent ContID = 1
	const transient ContID = 2
	s.Register(persistent, func(*Scheduler, State) {})
	s.Register(transient, func(*Scheduler, State) {})
	s.MarkTransient(transient)

	s.After(5, persistent, State{10})
	s.After(5, transient, State{20})
	s.After(7, transient, State{30})

	if got := s.PendingTransient(); got != 2 {
		t.Fatalf("PendingTransient = %d, want 2", got)
	}

	blob := s.Save(make([]byte, 0, s.SaveSize()))
	if len(blob) != s.SaveSize() {
		t.Fatalf("Save wrote %d bytes, SaveSize said %d (out of sync)", len(blob), s.SaveSize())
	}

	// Load into a scheduler that only knows the persistent cont — the
	// transient records must be absent, or Load would reject them.
	s2 := New()
	s2.Register(persistent, func(*Scheduler, State) {})
	if err := s2.Load(blob); err != nil {
		t.Fatalf("Load rejected blob (transient not skipped?): %v", err)
	}
	if s2.PendingSleepers() != 1 {
		t.Fatalf("loaded sleepers = %d, want 1 (only the persistent record)", s2.PendingSleepers())
	}
}

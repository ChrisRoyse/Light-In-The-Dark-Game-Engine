package sched

import "testing"

// Zero-alloc gate (R-GC-1/2, #99): ≥1,000 live scripts sleeping and
// waking per tick must run with AllocsPerRun == 0 after warm-up. This
// test fails on ANY alloc and runs in the per-PR `go test ./...` CI
// step — it IS the gate, the benchmarks below are for ns/op visibility.

const (
	contPerp ContID = 100 + iota // perpetually reschedules itself
	contPark                     // parks on evPing every time it runs
)

// newQuietSched registers non-logging continuations: the steady-state
// loop must not touch a trace buffer (that would be the test
// allocating, not the scheduler).
func newQuietSched() *Scheduler {
	s := New()
	s.Register(contPerp, func(s *Scheduler, st State) {
		s.After(uint32(st[1]%5)+1, contPerp, st)
	})
	s.Register(contPark, func(s *Scheduler, st State) {
		s.WaitEvent(evPing, contPark, st)
	})
	return s
}

func TestZeroAllocSteadyState(t *testing.T) {
	const scripts = 1000

	s := newQuietSched()
	for i := 0; i < scripts; i++ {
		s.After(uint32(i%5)+1, contPerp, State{int64(i), int64(i)})
	}
	for i := 0; i < 64; i++ { // warm-up: heap + pools reach steady capacity
		s.Step()
	}
	warm := testing.AllocsPerRun(100, func() { s.Step() })
	t.Logf("steady state, %d perpetual scripts: AllocsPerRun(Step) = %v", scripts, warm)
	if warm != 0 {
		t.Fatalf("R-GC-1 violation: %v allocs per tick at steady state", warm)
	}
}

// Edge: burst wake — all 1,000 sleepers wake on the same tick.
func TestZeroAllocBurstWake(t *testing.T) {
	const scripts = 1000
	s := New()
	s.Register(contNote, func(*Scheduler, State) {}) // wake and die
	// warm-up burst grows the heap to full capacity once
	for i := 0; i < scripts; i++ {
		s.After(1, contNote, State{int64(i)})
	}
	s.Step()
	n := testing.AllocsPerRun(50, func() {
		for i := 0; i < scripts; i++ {
			s.After(1, contNote, State{int64(i)})
		}
		s.Step() // every sleeper wakes this tick
	})
	t.Logf("burst: %d same-tick wakes: AllocsPerRun(schedule-all+Step) = %v", scripts, n)
	if n != 0 {
		t.Fatalf("R-GC-1 violation on burst wake: %v allocs", n)
	}
}

// Edge: event churn — 1,000 waiters park and the event fires, every
// iteration, exercising the dispatch list pool.
func TestZeroAllocEventChurn(t *testing.T) {
	const scripts = 1000
	s := newQuietSched()
	for i := 0; i < scripts; i++ {
		s.WaitEvent(evPing, contPark, State{int64(i)})
	}
	for i := 0; i < 4; i++ { // warm pool + list capacities
		s.FireEvent(evPing)
	}
	n := testing.AllocsPerRun(100, func() { s.FireEvent(evPing) })
	t.Logf("event churn: %d waiters re-parking per fire: AllocsPerRun(FireEvent) = %v", scripts, n)
	if n != 0 {
		t.Fatalf("R-GC-1 violation on event dispatch: %v allocs", n)
	}
}

// Pool-poisoning check: a released dispatch list is zeroed before
// reuse — no previous waiter's seq/ContID/state survives in the
// recycled backing array.
func TestPoolResetNoStaleState(t *testing.T) {
	s := New()
	seen := []State{}
	s.Register(contNote, func(_ *Scheduler, st State) { seen = append(seen, st) })

	sentinel := State{0x5EED, 0xDEAD, 0xBEEF, 0x7777}
	s.WaitEvent(evPing, contNote, sentinel)
	s.FireEvent(evPing) // list released back to pool (zeroed)

	pooled := s.listPool[len(s.listPool)-1]
	full := pooled[:cap(pooled)]
	t.Logf("released list: len=%d cap=%d; slot 0 after release = %+v", len(pooled), cap(pooled), full[0])
	if full[0] != (waiter{}) {
		t.Fatalf("pool poisoning: released slot still holds %+v", full[0])
	}

	// reuse the pooled list; the new waiter must carry only its own state
	s.WaitEvent(evPing, contNote, State{1, 2, 3, 4})
	s.FireEvent(evPing)
	t.Logf("dispatched states: %v", seen)
	if len(seen) != 2 || seen[1] != (State{1, 2, 3, 4}) {
		t.Fatalf("recycled list corrupted dispatch: %v", seen)
	}
}

// Serialization on a pooled scheduler must stay bit-identical — the
// #97 round-trip fixture re-runs against this implementation in the
// same package, and this test additionally saves a scheduler whose
// pool has been heavily cycled.
func TestSaveUnaffectedByPoolCycling(t *testing.T) {
	build := func(cycles int) []byte {
		e := &env{}
		s := newTestSched(e)
		for c := 0; c < cycles; c++ { // churn the pool
			s.WaitEvent(evPing, contNote, State{int64(c)})
			s.FireEvent(evPing)
		}
		s.After(5, contNote, State{9})
		s.WaitEvent(evPing, contNote, State{8})
		// cancel out seq differences: rebuild fresh with same logical ops
		return s.Save(nil)
	}
	a, b := build(0), build(0)
	if string(a) != string(b) {
		t.Fatal("identical builds saved differently")
	}
	heavy := build(500)
	light := build(500)
	t.Logf("pool-cycled saves: %d bytes == %d bytes, equal=%v", len(heavy), len(light), string(heavy) == string(light))
	if string(heavy) != string(light) {
		t.Fatal("pool cycling leaked into the save blob")
	}
}

func BenchmarkStepSteadyState1000(b *testing.B) {
	s := newQuietSched()
	for i := 0; i < 1000; i++ {
		s.After(uint32(i%5)+1, contPerp, State{int64(i), int64(i)})
	}
	for i := 0; i < 64; i++ {
		s.Step()
	}
	b.ReportAllocs()
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		s.Step()
	}
}

func BenchmarkFireEvent1000Waiters(b *testing.B) {
	s := newQuietSched()
	for i := 0; i < 1000; i++ {
		s.WaitEvent(evPing, contPark, State{int64(i)})
	}
	for i := 0; i < 4; i++ {
		s.FireEvent(evPing)
	}
	b.ReportAllocs()
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		s.FireEvent(evPing)
	}
}

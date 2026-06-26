package sim

import "testing"

// #551 — TimerStore pool foundation. These tests exercise the
// allocator invariants directly against the store's columns (the
// Source of Truth), not just return values: handle validity,
// generation staleness, LIFO reuse, capacity exhaustion, and the
// count/nextSeq/Dropped bookkeeping.

func TestTimerStoreAllocResolveFree(t *testing.T) {
	s := NewTimerStore(8)
	if s.Cap() != 8 {
		t.Fatalf("Cap = %d, want 8", s.Cap())
	}
	if s.Count() != 0 {
		t.Fatalf("fresh Count = %d, want 0", s.Count())
	}

	id, row, ok := s.alloc()
	if !ok {
		t.Fatal("alloc failed on empty pool")
	}
	if id == 0 {
		t.Fatal("alloc returned the invalid sentinel TimerID(0)")
	}
	if row == 0 {
		t.Fatal("alloc handed out reserved slot 0")
	}
	// SoT: the slot must be live and resolvable to the same row.
	if !s.live[row] {
		t.Fatalf("slot %d not marked live after alloc", row)
	}
	if got, ok := s.resolve(id); !ok || got != row {
		t.Fatalf("resolve(id) = (%d,%v), want (%d,true)", got, ok, row)
	}
	if s.Count() != 1 {
		t.Fatalf("Count after one alloc = %d, want 1", s.Count())
	}

	// Free, then the handle must go stale (generation bump).
	if !s.free_(id) {
		t.Fatal("free_ of live timer returned false")
	}
	if s.live[row] {
		t.Fatalf("slot %d still live after free", row)
	}
	if _, ok := s.resolve(id); ok {
		t.Fatal("stale handle resolved after free — generation guard failed")
	}
	if s.Count() != 0 {
		t.Fatalf("Count after free = %d, want 0", s.Count())
	}
	// Double free is an idempotent no-op.
	if s.free_(id) {
		t.Fatal("double free returned true; want no-op false")
	}
}

func TestTimerStoreGenerationReuse(t *testing.T) {
	s := NewTimerStore(4)
	id1, row1, _ := s.alloc()
	g1 := id1.Generation()
	s.free_(id1)
	id2, row2, ok := s.alloc()
	if !ok {
		t.Fatal("realloc after free failed")
	}
	// LIFO: the just-freed slot is reused.
	if row2 != row1 {
		t.Fatalf("reuse row = %d, want %d (LIFO)", row2, row1)
	}
	// But the generation advanced, so the old handle is dead and the
	// new one is live — they must differ.
	if id2 == id1 {
		t.Fatal("reused slot produced an identical handle — generation did not advance")
	}
	if id2.Generation() == g1 {
		t.Fatalf("generation %d unchanged on reuse", g1)
	}
	if _, ok := s.resolve(id1); ok {
		t.Fatal("old handle still resolves after slot reuse")
	}
	if _, ok := s.resolve(id2); !ok {
		t.Fatal("new handle does not resolve after reuse")
	}
}

func TestTimerStoreSeqMonotonic(t *testing.T) {
	s := NewTimerStore(8)
	var last uint32
	for i := 0; i < 5; i++ {
		id, row, ok := s.alloc()
		if !ok {
			t.Fatalf("alloc %d failed", i)
		}
		seq := s.Seq[row]
		if i > 0 && seq <= last {
			t.Fatalf("Seq not monotonic: got %d after %d", seq, last)
		}
		last = seq
		_ = id
	}
	// Freeing and reallocating must NOT reset nextSeq (spec §3:
	// nextSeq never resets within a match).
	before := s.nextSeq
	id, _, _ := s.alloc()
	s.free_(id)
	id2, _, _ := s.alloc()
	if s.Seq[id2.Index()] < before {
		t.Fatalf("Seq reused after free: %d < %d", s.Seq[id2.Index()], before)
	}
}

func TestTimerStoreExhaustion(t *testing.T) {
	const cap = 3
	s := NewTimerStore(cap)
	var ids []TimerID
	for i := 0; i < cap; i++ {
		id, _, ok := s.alloc()
		if !ok {
			t.Fatalf("alloc %d failed below capacity", i)
		}
		ids = append(ids, id)
	}
	if s.Count() != cap {
		t.Fatalf("Count at capacity = %d, want %d", s.Count(), cap)
	}
	// Over capacity: invalid sentinel + Dropped increments, no panic.
	id, _, ok := s.alloc()
	if ok || id != 0 {
		t.Fatalf("alloc past capacity returned (%d,%v), want (0,false)", id, ok)
	}
	if s.Dropped != 1 {
		t.Fatalf("Dropped = %d, want 1", s.Dropped)
	}
	// Free one and we can allocate again.
	s.free_(ids[0])
	if _, _, ok := s.alloc(); !ok {
		t.Fatal("alloc failed after freeing a slot at capacity")
	}
}

func TestTimerStoreStaleAndMalformedHandles(t *testing.T) {
	s := NewTimerStore(4)
	// Invalid sentinel.
	if _, ok := s.resolve(0); ok {
		t.Fatal("TimerID(0) resolved")
	}
	// Out-of-range index.
	if _, ok := s.resolve(makeTimerID(9999, 0)); ok {
		t.Fatal("out-of-range handle resolved")
	}
	// Never-allocated slot (live=false).
	if _, ok := s.resolve(makeTimerID(2, 0)); ok {
		t.Fatal("handle to never-allocated slot resolved")
	}
	if s.Alive(makeTimerID(2, 0)) {
		t.Fatal("Alive true for never-allocated slot")
	}
}

func TestTimerStoreZeroAlloc(t *testing.T) {
	s := NewTimerStore(64)
	// Steady-state churn must not allocate: alloc/free only slice the
	// preallocated free list (R-GC-1).
	avg := testing.AllocsPerRun(1000, func() {
		id, _, ok := s.alloc()
		if !ok {
			t.Fatal("alloc failed mid-churn")
		}
		s.free_(id)
	})
	if avg != 0 {
		t.Fatalf("alloc/free churn allocated %.2f objs/op, want 0", avg)
	}
}

func TestNewWorldWiresTimerStore(t *testing.T) {
	w := NewWorld(Caps{})
	if w.Timers == nil {
		t.Fatal("NewWorld did not construct w.Timers")
	}
	if w.Timers.Cap() != EngineCaps.Timers {
		t.Fatalf("default timer cap = %d, want %d", w.Timers.Cap(), EngineCaps.Timers)
	}
	// A lowered request is honored; a request above the ceiling clamps.
	w2 := NewWorld(Caps{Timers: 16})
	if w2.Timers.Cap() != 16 {
		t.Fatalf("requested cap 16 -> %d", w2.Timers.Cap())
	}
	w3 := NewWorld(Caps{Timers: 1 << 30})
	if w3.Timers.Cap() != EngineCaps.Timers {
		t.Fatalf("over-ceiling cap -> %d, want clamp to %d", w3.Timers.Cap(), EngineCaps.Timers)
	}
}

// #552 — modes + reschedule + cancel lifecycle. Asserts against the
// store columns (SoT), driving onFired transitions directly.

func TestTimerCreateColumns(t *testing.T) {
	s := NewTimerStore(8)
	st := [4]int64{11, 22, 33, 44}
	id := s.Create(100, TimerCount, 5, 3, 7, st, EntityID(0x2000005))
	row, ok := s.resolve(id)
	if !ok {
		t.Fatal("Create returned a handle that does not resolve")
	}
	if TimerMode(s.Mode[row]) != TimerCount {
		t.Fatalf("Mode = %d", s.Mode[row])
	}
	if s.Interval[row] != 5 {
		t.Fatalf("Interval = %d, want 5", s.Interval[row])
	}
	if s.WakeTick[row] != 105 {
		t.Fatalf("WakeTick = %d, want 105 (now100+iv5)", s.WakeTick[row])
	}
	if s.Remaining[row] != 3 {
		t.Fatalf("Remaining = %d, want 3", s.Remaining[row])
	}
	if s.Cont[row] != 7 {
		t.Fatalf("Cont = %d, want 7", s.Cont[row])
	}
	if s.State[row] != st {
		t.Fatalf("State = %v, want %v", s.State[row], st)
	}
	if s.Owner[row] != EntityID(0x2000005) {
		t.Fatalf("Owner = %#x", s.Owner[row])
	}
}

func TestTimerCreateQuantizesInterval(t *testing.T) {
	s := NewTimerStore(4)
	id := s.Create(0, TimerSingle, 0, 0, 1, [4]int64{}, 0)
	row, _ := s.resolve(id)
	if s.Interval[row] != 1 {
		t.Fatalf("zero interval not quantized up: Interval=%d", s.Interval[row])
	}
	if s.WakeTick[row] != 1 {
		t.Fatalf("WakeTick = %d, want 1 (now0 + floor1)", s.WakeTick[row])
	}
}

func TestTimerOnFiredSingle(t *testing.T) {
	s := NewTimerStore(4)
	id := s.Create(0, TimerSingle, 3, 0, 1, [4]int64{}, 0)
	row, _ := s.resolve(id)
	wake, live := s.onFired(row)
	if live || wake != 0 {
		t.Fatalf("single onFired = (%d,%v), want (0,false)", wake, live)
	}
	if s.Alive(id) {
		t.Fatal("single timer still alive after fire")
	}
}

func TestTimerOnFiredLoop(t *testing.T) {
	s := NewTimerStore(4)
	id := s.Create(10, TimerLoop, 4, 0, 1, [4]int64{}, 0)
	row, _ := s.resolve(id)
	// fires at 14, 18, 22 ...
	if s.WakeTick[row] != 14 {
		t.Fatalf("first wake = %d, want 14", s.WakeTick[row])
	}
	wake, live := s.onFired(row)
	if !live || wake != 18 {
		t.Fatalf("loop onFired = (%d,%v), want (18,true)", wake, live)
	}
	wake, live = s.onFired(row)
	if !live || wake != 22 {
		t.Fatalf("loop onFired#2 = (%d,%v), want (22,true)", wake, live)
	}
	if !s.Alive(id) {
		t.Fatal("loop timer freed unexpectedly")
	}
}

func TestTimerOnFiredCount(t *testing.T) {
	s := NewTimerStore(4)
	id := s.Create(0, TimerCount, 2, 3, 1, [4]int64{}, 0)
	row, _ := s.resolve(id)
	// 3 fires expected: remaining 3->2->1->0(free)
	if _, live := s.onFired(row); !live || s.Remaining[row] != 2 {
		t.Fatalf("count fire 1: remaining=%d live check failed", s.Remaining[row])
	}
	if _, live := s.onFired(row); !live || s.Remaining[row] != 1 {
		t.Fatalf("count fire 2: remaining=%d", s.Remaining[row])
	}
	wake, live := s.onFired(row)
	if live || wake != 0 {
		t.Fatalf("count fire 3 = (%d,%v), want (0,false) free", wake, live)
	}
	if s.Alive(id) {
		t.Fatal("count timer alive after exhausting fires")
	}
}

func TestTimerCancelIdempotent(t *testing.T) {
	s := NewTimerStore(4)
	id := s.Create(0, TimerLoop, 5, 0, 1, [4]int64{9}, EntityID(7))
	if !s.Cancel(id) {
		t.Fatal("Cancel of live timer returned false")
	}
	if s.Alive(id) {
		t.Fatal("timer alive after Cancel")
	}
	row := int32(id.Index())
	if s.Owner[row] != 0 || s.State[row] != ([4]int64{}) {
		t.Fatalf("Cancel did not clear owner/state: owner=%#x state=%v", s.Owner[row], s.State[row])
	}
	if s.Cancel(id) {
		t.Fatal("second Cancel returned true; want idempotent no-op")
	}
}

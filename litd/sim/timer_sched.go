package sim

import "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim/sched"

// Timer schedule index + advance drain (#553, spec §3–§5).
//
// The index is a binary min-heap keyed on (WakeTick, Seq) — the same
// total order the scheduler uses for sleepers (sched.go recordLess).
// Seq is the timer's monotonic allocation sequence, so two timers
// waking on the same tick fire in a fully determined order that is
// stable across machines and save/load (R-TMR-3).
//
// The heap is a DERIVED index (spec §4): it is rebuilt on load from the
// live timer columns and is neither hashed nor serialized, so it can be
// retuned without touching the fingerprint. A binary heap (not a timing
// wheel) is the right call at Caps.Timers = 4,096: the contiguous
// backing prefetches into L1 and beats a wheel below ~10K timers
// (perf-budget §3); cheap cancellation comes from the heapPos back-index
// rather than from wheel buckets.

// less reports whether heap entry i sorts before j by (wake, seq).
func (s *TimerStore) less(i, j int32) bool {
	if s.hWake[i] != s.hWake[j] {
		return s.hWake[i] < s.hWake[j]
	}
	return s.hSeq[i] < s.hSeq[j]
}

// swap exchanges heap entries i and j and keeps heapPos in sync.
func (s *TimerStore) swap(i, j int32) {
	s.hWake[i], s.hWake[j] = s.hWake[j], s.hWake[i]
	s.hSeq[i], s.hSeq[j] = s.hSeq[j], s.hSeq[i]
	s.hID[i], s.hID[j] = s.hID[j], s.hID[i]
	s.heapPos[s.hID[i].Index()] = i
	s.heapPos[s.hID[j].Index()] = j
}

func (s *TimerStore) siftUp(i int32) {
	for i > 0 {
		p := (i - 1) / 2
		if !s.less(i, p) {
			break
		}
		s.swap(i, p)
		i = p
	}
}

func (s *TimerStore) siftDown(i int32) {
	for {
		l, r := 2*i+1, 2*i+2
		min := i
		if l < s.hLen && s.less(l, min) {
			min = l
		}
		if r < s.hLen && s.less(r, min) {
			min = r
		}
		if min == i {
			break
		}
		s.swap(i, min)
		i = min
	}
}

// heapPush inserts a live row's (WakeTick, Seq) into the index. The row
// must not already be scheduled (heapPos == -1). Zero alloc — the heap
// backing is preallocated to capacity and an eagerly-maintained heap
// never holds more than `count` entries.
func (s *TimerStore) heapPush(row int32) {
	i := s.hLen
	s.hWake[i] = s.WakeTick[row]
	s.hSeq[i] = s.Seq[row]
	s.hID[i] = makeTimerID(uint32(row), s.Gen[row])
	s.heapPos[row] = i
	s.hLen++
	s.siftUp(i)
}

// heapRemove drops the entry at heap index pos and returns its handle,
// restoring the heap invariant. The last entry fills the hole and is
// sifted both ways (it may belong above or below pos).
func (s *TimerStore) heapRemove(pos int32) TimerID {
	id := s.hID[pos]
	s.heapPos[id.Index()] = -1
	last := s.hLen - 1
	if pos != last {
		s.hWake[pos] = s.hWake[last]
		s.hSeq[pos] = s.hSeq[last]
		s.hID[pos] = s.hID[last]
		s.heapPos[s.hID[pos].Index()] = pos
	}
	s.hLen--
	if pos < s.hLen {
		s.siftDown(pos)
		s.siftUp(pos)
	}
	return id
}

// HeapLen is the number of scheduled timers (testing/inspection).
func (s *TimerStore) HeapLen() int32 { return s.hLen }

// advance fires every timer due at `now` (WakeTick <= now) in
// (WakeTick, Seq) order, running each continuation through the shared
// scheduler so timer fires and script resumes obey one wake-ordering
// authority and one continuation registry (spec §5).
//
// A continuation may create timers (scheduled strictly later, so they
// cannot fire in this same drain — Create's 1-tick floor guarantees
// WakeTick > now), cancel other timers (their heap entries are removed
// eagerly), or cancel the very timer being fired (self-cancel): the
// handle is re-resolved after the continuation runs, and a stale/dead
// handle skips the reschedule. The drain therefore always terminates.
func (s *TimerStore) advance(now uint32, sc *sched.Scheduler) {
	for s.hLen > 0 && s.hWake[0] <= now {
		id := s.heapRemove(0)
		row, ok := s.resolve(id)
		if !ok {
			continue // cancelled/reused since scheduling (defensive)
		}
		cont := sched.ContID(s.Cont[row])
		st := sched.State(s.State[row])
		sc.Invoke(cont, st)
		// Re-resolve: the continuation may have freed (and possibly
		// reused) this slot. Only a still-live, same-generation handle
		// gets its post-fire transition + reschedule.
		if row2, ok := s.resolve(id); ok {
			if _, live := s.onFired(row2); live {
				s.heapPush(row2)
			}
		}
	}
}

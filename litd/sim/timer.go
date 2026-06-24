package sim

// Serializable timer wheel — PRD2 01-timer-wheel. This file lands the
// pool foundation only (#551): the SoA store, the packed TimerID
// handle, the generation-checked free-list allocator, and the
// capacity/exhaustion posture. Timer *modes* and the reschedule/cancel
// lifecycle (#552), the (WakeTick,Seq) schedule index + advance()
// (#553), owner auto-cancel (#554), and the hash/save sections (#555)
// build on top of this store without changing its layout.
//
// Why a self-pooled store rather than an entity-keyed one: a timer is
// not an entity. It has its own lifetime, its own handle space, and is
// allocated/freed far more often than units. So it mirrors the entity
// *allocator* pattern (entity.go) — a packed handle with a generation
// guard and a LIFO free list — rather than the entity-keyed component
// stores (store_missile.go). Slot 0 is reserved so TimerID(0) is an
// unambiguous "no timer" sentinel, exactly as EntityID(0) is for
// entities.

// TimerID is the packed 32-bit timer handle, identical packing to
// EntityID (architecture-principles.md §2):
//
//	[ generation:8 | index:24 ]
//
// A stale handle — one whose generation no longer matches its slot —
// resolves to a safe no-op (read: zero value; mutate: ignored), the
// WC3 dead-handle semantics (R-API-5). TimerID(0) is the invalid
// sentinel returned on exhaustion.
type TimerID uint32

// Index addresses the slot in the timer table (low 24 bits).
func (t TimerID) Index() uint32 { return uint32(t) & 0x00FFFFFF }

// Generation is the slot-reuse counter carried by the handle.
func (t TimerID) Generation() uint8 { return uint8(t >> 24) }

func makeTimerID(index uint32, gen uint8) TimerID {
	return TimerID(uint32(gen)<<24 | index&0x00FFFFFF)
}

// TimerMode selects the firing discipline (lifecycle in #552).
type TimerMode uint8

const (
	TimerSingle TimerMode = iota // fire once, then free
	TimerLoop                    // fire every Interval until cancelled
	TimerCount                   // fire every Interval exactly Remaining times, then free
)

// TimerStore is the SoA pool for all live timers in a match
// (architecture-principles.md §1, spec §1). Columns are indexed by
// slot, NOT by a dense [0,count) range: a timer keeps a stable slot
// for its whole life so its TimerID stays valid, and the schedule
// index (#553) references slots directly. Liveness is tracked by the
// `live` column; the `free` LIFO list hands out reusable slots.
//
// All columns are sized once at construction (R-GC-2). There is no
// append-growth: exhaustion returns TimerID(0) and bumps Dropped.
type TimerStore struct {
	// --- columns, indexed by slot (1..cap; slot 0 reserved) ---
	Mode      []uint8    // TimerMode
	Interval  []uint32   // ticks between fires (>=1; 0 quantized to 1 by #552)
	WakeTick  []uint32   // absolute sim tick of next fire
	Remaining []uint32   // TimerCount: fires left; else unused
	Seq       []uint32   // monotonic allocation sequence (fire tie-break)
	Cont      []uint16   // ContID — stable continuation, not a closure (R-TMR-2)
	State     [][4]int64 // value payload passed to the continuation
	Owner     []EntityID // optional; 0 = unowned (auto-cancel on death, #554)
	Gen       []uint8    // generation for the slot (handle validation)
	live      []bool     // slot occupied?

	free    []int32 // free-list (LIFO); serialized for slot-stable reload (#555)
	count   int32   // live timer count
	nextSeq uint32  // monotonic alloc sequence; never reset within a match

	// Dropped counts creation attempts that failed because the pool was
	// exhausted. Part of hashed state (#555) so a divergent capacity
	// surfaces in the fingerprint — fail-closed, not silently lossy.
	Dropped uint32

	// DebugAssert, when non-nil, is called on contract violations
	// (stale free, double free). The operation still degrades to the
	// safe no-op; determinism is never sacrificed to report (§8).
	DebugAssert func(msg string, id TimerID)
}

// NewTimerStore returns a pool with exactly `capacity` usable slots,
// allocated once. Slot 0 is reserved and never handed out so TimerID(0)
// stays an unambiguous invalid sentinel. capacity must fit the 24-bit
// index space.
func NewTimerStore(capacity int) *TimerStore {
	if capacity <= 0 || capacity >= 1<<24 {
		panic("sim: timer capacity must be in (0, 2^24)")
	}
	n := capacity + 1 // slot 0 reserved
	s := &TimerStore{
		Mode:      make([]uint8, n),
		Interval:  make([]uint32, n),
		WakeTick:  make([]uint32, n),
		Remaining: make([]uint32, n),
		Seq:       make([]uint32, n),
		Cont:      make([]uint16, n),
		State:     make([][4]int64, n),
		Owner:     make([]EntityID, n),
		Gen:       make([]uint8, n),
		live:      make([]bool, n),
		free:      make([]int32, 0, capacity),
	}
	// Seed the free list. Push high indices first so the first alloc
	// hands out slot 1 (LIFO pop), giving stable, low-first slot
	// assignment in the common case — easier to reason about in tests
	// and in the serialized free-list order (#555).
	for i := capacity; i >= 1; i-- {
		s.free = append(s.free, int32(i))
	}
	return s
}

// Cap is the number of usable timer slots (excludes the reserved slot 0).
func (s *TimerStore) Cap() int { return len(s.live) - 1 }

// Count is the number of live timers.
func (s *TimerStore) Count() int32 { return s.count }

// alloc reserves a slot and returns its handle and row index. ok is
// false (and id is TimerID(0)) when the pool is exhausted — the caller
// turns that into the gameplay-level "timer not created" outcome and
// the exhaustion is recorded in Dropped. Zero alloc: the free list is
// preallocated and only sliced.
//
// alloc assigns Gen and Seq and marks the slot live; it does NOT set
// the behavior columns (Mode/Interval/WakeTick/…). Higher layers
// (#552) fill those immediately after a successful alloc.
func (s *TimerStore) alloc() (id TimerID, row int32, ok bool) {
	n := len(s.free)
	if n == 0 {
		s.Dropped++
		return 0, 0, false
	}
	row = s.free[n-1]
	s.free = s.free[:n-1]
	s.live[row] = true
	s.Seq[row] = s.nextSeq
	s.nextSeq++
	s.count++
	return makeTimerID(uint32(row), s.Gen[row]), row, true
}

// free releases a slot, bumping its generation so every outstanding
// handle to it goes stale. Idempotent: freeing an already-free or
// stale slot is a no-op (returns false), matching cancel-of-cancelled.
func (s *TimerStore) free_(id TimerID) bool {
	row, ok := s.resolve(id)
	if !ok {
		s.assert("free of stale/absent timer", id)
		return false
	}
	s.freeRow(row)
	return true
}

// freeRow releases a live slot by row index (no handle to validate —
// the caller already holds a resolved row, e.g. the reschedule path).
func (s *TimerStore) freeRow(row int32) {
	s.live[row] = false
	s.Gen[row]++ // wraps at 256; stale handles fail the generation check
	// Clear references so a freed slot cannot pin an entity or leak a
	// continuation payload across reuse (pool-reset discipline, R-GC-2).
	s.Owner[row] = 0
	s.State[row] = [4]int64{}
	s.free = append(s.free, row)
	s.count--
}

// resolve maps a handle to its live slot, validating the generation.
// ok is false for the invalid sentinel, an out-of-range index, a dead
// slot, or a generation mismatch (stale handle ⇒ no-op).
func (s *TimerStore) resolve(id TimerID) (row int32, ok bool) {
	idx := id.Index()
	if idx == 0 || idx >= uint32(len(s.live)) {
		return 0, false
	}
	r := int32(idx)
	if !s.live[r] || s.Gen[r] != id.Generation() {
		return 0, false
	}
	return r, true
}

// Alive reports whether a handle refers to a live timer.
func (s *TimerStore) Alive(id TimerID) bool {
	_, ok := s.resolve(id)
	return ok
}

// ---------------------------------------------------------------------
// Modes & lifecycle (#552). Create fills the behavior columns after a
// successful alloc; onFired applies the per-mode reschedule/free
// transition; Cancel is the idempotent generation-checked teardown.
// The schedule index that actually drives firing (the (WakeTick,Seq)
// wheel) and the advance() drain land in #553 — these methods are its
// state-transition primitives.
// ---------------------------------------------------------------------

// Create allocates and arms a timer that next fires at
// now + max(1, interval). For TimerCount, remaining is the total number
// of fires (clamped to >=1); it is ignored for Single/Loop. cont is the
// scheduler continuation to run on fire; st is its value payload; owner
// (0 = unowned) enables auto-cancel on death (#554).
//
// Returns TimerID(0) on pool exhaustion (Dropped already incremented by
// alloc) — the caller treats that as "timer not created". Zero alloc.
func (s *TimerStore) Create(now uint32, mode TimerMode, interval, remaining uint32, cont uint16, st [4]int64, owner EntityID) TimerID {
	iv := interval
	if iv == 0 {
		iv = 1 // sub-tick / zero quantizes UP to the 1-tick floor (R-EXEC-5)
	}
	id, row, ok := s.alloc()
	if !ok {
		return 0
	}
	s.Mode[row] = uint8(mode)
	s.Interval[row] = iv
	s.WakeTick[row] = now + iv
	if mode == TimerCount {
		if remaining == 0 {
			remaining = 1
		}
		s.Remaining[row] = remaining
	} else {
		s.Remaining[row] = 0
	}
	s.Cont[row] = cont
	s.State[row] = st
	s.Owner[row] = owner
	return id
}

// onFired applies the post-fire transition for a row that just ran its
// continuation. It returns the next absolute wake tick and whether the
// timer remains live:
//   - Single: freed, ok=false.
//   - Loop:   WakeTick += Interval, ok=true.
//   - Count:  Remaining--; if it hits 0 the timer is freed (ok=false),
//     else WakeTick += Interval (ok=true).
//
// Rescheduling always pushes to a strictly later tick (Interval>=1), so
// a same-tick drain cannot loop forever (spec §2.3). The schedule index
// re-insert is the caller's job (#553); onFired only mutates columns.
func (s *TimerStore) onFired(row int32) (wake uint32, live bool) {
	switch TimerMode(s.Mode[row]) {
	case TimerLoop:
		s.WakeTick[row] += s.Interval[row]
		return s.WakeTick[row], true
	case TimerCount:
		s.Remaining[row]--
		if s.Remaining[row] == 0 {
			s.freeRow(row)
			return 0, false
		}
		s.WakeTick[row] += s.Interval[row]
		return s.WakeTick[row], true
	default: // TimerSingle
		s.freeRow(row)
		return 0, false
	}
}

// Cancel tears down a timer by handle: generation-checked, frees the
// slot, bumps the generation so the handle goes stale. Idempotent — a
// stale or already-cancelled handle is a safe no-op returning false
// (R-API-5). The schedule-index removal is lazy (#553 skips dead slots
// on drain), so there is nothing to unlink here.
func (s *TimerStore) Cancel(id TimerID) bool { return s.free_(id) }

func (s *TimerStore) assert(msg string, id TimerID) {
	if s.DebugAssert != nil {
		s.DebugAssert(msg, id)
	}
}

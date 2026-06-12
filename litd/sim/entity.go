package sim

// EntityID is the packed 32-bit entity handle (ecs-architecture.md §3):
//
//	[ generation:8 | index:24 ]
//
// Scripts and the public API hold EntityIDs, never store rows. A stale
// handle — one whose generation no longer matches its slot — resolves
// to "dead entity" and every operation on it becomes a zero-value
// no-op (R-API-5, the WC3 invalid-handle semantics).
type EntityID uint32

// Index addresses the slot in the entity table.
func (e EntityID) Index() uint32 { return uint32(e) & 0x00FFFFFF }

// Generation is the slot-reuse counter carried by the handle.
func (e EntityID) Generation() uint8 { return uint8(e >> 24) }

func makeEntityID(index uint32, gen uint8) EntityID {
	return EntityID(uint32(gen)<<24 | index&0x00FFFFFF)
}

// entitySlot is one entry of the table. The free list is intrusive:
// next lives in the dead slot itself — no separate allocation.
type entitySlot struct {
	gen   uint8
	alive bool
	next  int32 // when dead: next free slot index, -1 = end of list
}

// Entities is the fixed-capacity entity allocator. Capacity is set
// once at map load and never grows (R-GC-2); exhaustion makes Create
// fail — a gameplay outcome, exactly as WC3 refuses past its handle
// limits. Creation/destruction order is deterministic (LIFO free
// list) and is part of hashed simulation state.
type Entities struct {
	slots    []entitySlot
	freeHead int32
	count    int32

	// DebugStaleHandle, when non-nil, is called on every resolve of a
	// stale or malformed handle (debug-mode assert per R-API-5). The
	// operation still proceeds as a zero-value no-op.
	DebugStaleHandle func(EntityID)
}

// NewEntities returns an allocator with exactly capacity slots,
// allocated once. capacity must fit in the 24-bit index space.
func NewEntities(capacity int) *Entities {
	if capacity <= 0 || capacity > 1<<24 {
		panic("sim: entity capacity must be in (0, 2^24]")
	}
	e := &Entities{
		slots:    make([]entitySlot, capacity),
		freeHead: 0,
	}
	for i := range e.slots {
		e.slots[i].next = int32(i) + 1
	}
	e.slots[capacity-1].next = -1
	return e
}

// Create allocates an entity. ok is false when the pool is exhausted —
// the caller turns that into the gameplay-level creation failure.
func (e *Entities) Create() (EntityID, bool) {
	if e.freeHead < 0 {
		return 0, false
	}
	idx := e.freeHead
	s := &e.slots[idx]
	e.freeHead = s.next
	s.alive = true
	e.count++
	return makeEntityID(uint32(idx), s.gen), true
}

// Destroy kills the entity. The slot's generation increments
// immediately (wrapping at 256, accepted per ecs §3) so every
// outstanding handle goes stale, and the slot pushes onto the LIFO
// free list. Destroying via a stale/dead handle is a no-op (false).
func (e *Entities) Destroy(id EntityID) bool {
	idx, ok := e.resolve(id)
	if !ok {
		return false
	}
	s := &e.slots[idx]
	s.gen++ // uint8 wrap is the documented 256-reuse behavior
	s.alive = false
	s.next = e.freeHead
	e.freeHead = idx
	e.count--
	return true
}

// Alive reports whether id refers to a live entity (generation match).
func (e *Entities) Alive(id EntityID) bool {
	_, ok := e.resolve(id)
	return ok
}

// resolve maps a handle to its slot index, fail-closed: out-of-range,
// dead, or generation-mismatched handles return ok=false (zero-value
// no-op for the caller) and trip the debug assert hook.
func (e *Entities) resolve(id EntityID) (int32, bool) {
	idx := id.Index()
	if idx >= uint32(len(e.slots)) {
		e.staleAssert(id)
		return 0, false
	}
	s := &e.slots[idx]
	if !s.alive || s.gen != id.Generation() {
		e.staleAssert(id)
		return 0, false
	}
	return int32(idx), true
}

func (e *Entities) staleAssert(id EntityID) {
	if e.DebugStaleHandle != nil {
		e.DebugStaleHandle(id)
	}
}

// Count returns the number of live entities.
func (e *Entities) Count() int { return int(e.count) }

// Cap returns the fixed capacity.
func (e *Entities) Cap() int { return len(e.slots) }

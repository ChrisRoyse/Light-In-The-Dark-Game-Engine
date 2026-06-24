package sim

// Persistent unit-group store — PRD2 02-unit-groups. This file lands the
// pool + shared-arena foundation (#560): the SoA store, the packed
// GroupID handle, the generation-checked free-list allocator over group
// slots, and the shared Members arena. Membership ops (add/remove/clear,
// #561), set algebra (#562), query fills (#563), dead-member prune
// (#564), and hash/save (#565) build on this layout without changing it.
//
// v1 allocator (spec §1.2, sanctioned by #560): every group gets a fixed
// contiguous span of MembersPerGroup slots in the arena — group g owns
// Members[g*perCap : g*perCap + Len[g]]. The Start/Len/Cap columns and
// the serialized member bytes are IDENTICAL to the eventual best-fit
// span-allocator version, so swapping the allocator later (varied group
// sizes, follow-up issue) changes no hash or save format — only how
// Start/Cap are assigned. Until then a group caps at MembersPerGroup
// members; an add past that fails with a dropped-member counter.

// GroupID is the packed 32-bit group handle, identical packing to
// EntityID/TimerID (architecture-principles.md §2):
//
//	[ generation:8 | index:24 ]
//
// A stale handle resolves to a safe no-op (R-API-5). GroupID(0) is the
// invalid sentinel returned on exhaustion; slot 0 is reserved.
type GroupID uint32

func (g GroupID) Index() uint32      { return uint32(g) & 0x00FFFFFF }
func (g GroupID) Generation() uint8  { return uint8(g >> 24) }
func makeGroupID(index uint32, gen uint8) GroupID {
	return GroupID(uint32(gen)<<24 | index&0x00FFFFFF)
}

// GroupStore is the SoA pool of persistent unit groups over a shared
// Members arena (architecture-principles.md §1, spec §1). Columns are
// indexed by group slot (1..cap; slot 0 reserved). All arrays are sized
// once at construction (R-GC-2); there is no append-growth.
type GroupStore struct {
	// --- per-group columns, indexed by slot ---
	Start []int32 // index into Members where this group's span begins
	Len   []int32 // live member count (0 <= Len <= Cap)
	Cap   []int32 // reserved span capacity (v1: constant MembersPerGroup)
	Gen   []uint8 // generation for handle validation
	live  []bool  // slot occupied?

	// Members is the shared membership arena. Group at slot s owns
	// Members[Start[s] : Start[s]+Len[s]]. Insertion-ordered (R-UGR-2).
	Members []EntityID

	free    []int32 // free group slots (LIFO); serialized for slot-stable reload
	count   int32   // live group count
	perCap  int32   // v1 fixed members-per-group span size

	// Dropped counts add attempts that failed because a group's span was
	// full (#561) or CreateGroup attempts that failed on pool exhaustion.
	// Part of hashed state (#565) so a capacity divergence fails closed.
	DroppedGroups  uint32
	DroppedMembers uint32

	DebugAssert func(msg string, id GroupID)
}

// NewGroupStore returns a pool of `groupCap` group slots over a Members
// arena of `memberCap` slots. The v1 per-group span size is
// memberCap/groupCap (so the arena is fully partitioned, one fixed span
// per group). Both caps must be positive and fit the 24-bit index space;
// memberCap must be a multiple of groupCap is NOT required — any
// remainder is simply unused arena tail.
func NewGroupStore(groupCap, memberCap int) *GroupStore {
	if groupCap <= 0 || groupCap >= 1<<24 {
		panic("sim: group capacity must be in (0, 2^24)")
	}
	if memberCap <= 0 || memberCap >= 1<<24 {
		panic("sim: group member capacity must be in (0, 2^24)")
	}
	perCap := int32(memberCap / groupCap)
	if perCap < 1 {
		perCap = 1
	}
	n := groupCap + 1 // slot 0 reserved
	s := &GroupStore{
		Start:   make([]int32, n),
		Len:     make([]int32, n),
		Cap:     make([]int32, n),
		Gen:     make([]uint8, n),
		live:    make([]bool, n),
		Members: make([]EntityID, memberCap),
		free:    make([]int32, 0, groupCap),
		perCap:  perCap,
	}
	// Pre-assign each slot's fixed span and seed the free list (low slot
	// first via LIFO, like TimerStore) so slot assignment is stable.
	for i := 1; i <= groupCap; i++ {
		s.Start[i] = int32(i-1) * perCap
		s.Cap[i] = perCap
	}
	for i := groupCap; i >= 1; i-- {
		s.free = append(s.free, int32(i))
	}
	return s
}

// GroupCap is the number of usable group slots (excludes reserved slot 0).
func (s *GroupStore) GroupCap() int { return len(s.live) - 1 }

// MembersPerGroup is the v1 fixed per-group span capacity.
func (s *GroupStore) MembersPerGroup() int32 { return s.perCap }

// Count is the number of live groups.
func (s *GroupStore) Count() int32 { return s.count }

// CreateGroup allocates an empty group and returns its handle. Returns
// GroupID(0) and increments DroppedGroups when the pool is exhausted.
// Zero alloc.
func (s *GroupStore) CreateGroup() GroupID {
	n := len(s.free)
	if n == 0 {
		s.DroppedGroups++
		return 0
	}
	row := s.free[n-1]
	s.free = s.free[:n-1]
	s.live[row] = true
	s.Len[row] = 0
	s.count++
	return makeGroupID(uint32(row), s.Gen[row])
}

// DestroyGroup frees a group's slot, bumping its generation so every
// outstanding handle goes stale. Idempotent: destroying a stale/free
// group is a no-op (returns false). The span returns to the slot's fixed
// region (v1) — nothing to coalesce.
func (s *GroupStore) DestroyGroup(id GroupID) bool {
	row, ok := s.resolve(id)
	if !ok {
		s.assert("DestroyGroup of stale/absent group", id)
		return false
	}
	s.live[row] = false
	s.Gen[row]++
	s.Len[row] = 0
	s.free = append(s.free, row)
	s.count--
	return true
}

// resolve maps a handle to its live slot, validating the generation.
func (s *GroupStore) resolve(id GroupID) (row int32, ok bool) {
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

// Alive reports whether a handle refers to a live group.
func (s *GroupStore) Alive(id GroupID) bool {
	_, ok := s.resolve(id)
	return ok
}

// ---------------------------------------------------------------------
// Membership ops (#561). All operate on a group's span
// Members[Start:Start+Len]. Uniqueness and Contains use a linear span
// scan (O(Cap)=O(64) in the v1 fixed-cap cut, spec §2-sanctioned); the
// O(1) presence bitset is the span-allocator follow-up's concern. All
// zero-alloc; stale handles are safe no-ops.
// ---------------------------------------------------------------------

// memberIndex returns the position of e within group row's span, or -1.
func (s *GroupStore) memberIndex(row int32, target EntityID) int32 {
	start, n := s.Start[row], s.Len[row]
	for i := int32(0); i < n; i++ {
		if s.Members[start+i] == target {
			return i
		}
	}
	return -1
}

// GroupContains reports whether e is a member of g. Stale g ⇒ false.
func (s *GroupStore) GroupContains(id GroupID, e EntityID) bool {
	row, ok := s.resolve(id)
	if !ok {
		return false
	}
	return s.memberIndex(row, e) >= 0
}

// GroupCount returns g's member count, 0 for a stale handle.
func (s *GroupStore) GroupCount(id GroupID) int32 {
	row, ok := s.resolve(id)
	if !ok {
		return 0
	}
	return s.Len[row]
}

// GroupFirst returns g's first (oldest) member in insertion order, or
// EntityID(0) when g is empty or stale — a deterministic pick.
func (s *GroupStore) GroupFirst(id GroupID) EntityID {
	row, ok := s.resolve(id)
	if !ok || s.Len[row] == 0 {
		return 0
	}
	return s.Members[s.Start[row]]
}

// GroupAdd appends e to g if not already present (unique, insertion-
// ordered). Returns true if e is in g after the call (added or already
// present), false on a stale handle or a full span (DroppedMembers++).
func (s *GroupStore) GroupAdd(id GroupID, e EntityID) bool {
	row, ok := s.resolve(id)
	if !ok {
		s.assert("GroupAdd on stale/absent group", id)
		return false
	}
	if s.memberIndex(row, e) >= 0 {
		return true // already a member — unique, no-op
	}
	if s.Len[row] >= s.Cap[row] {
		s.DroppedMembers++
		return false // span full (v1 fixed cap; span allocator lifts this)
	}
	s.Members[s.Start[row]+s.Len[row]] = e
	s.Len[row]++
	return true
}

// GroupRemove removes e from g by swap (O(1), reorders the survivors).
// Returns true if e was present. Stale handle ⇒ false.
func (s *GroupStore) GroupRemove(id GroupID, e EntityID) bool {
	row, ok := s.resolve(id)
	if !ok {
		return false
	}
	i := s.memberIndex(row, e)
	if i < 0 {
		return false
	}
	start, last := s.Start[row], s.Len[row]-1
	s.Members[start+i] = s.Members[start+last]
	s.Len[row]--
	return true
}

// GroupRemoveOrdered removes e from g preserving insertion order (O(n)
// shift). Returns true if e was present. Stale handle ⇒ false.
func (s *GroupStore) GroupRemoveOrdered(id GroupID, e EntityID) bool {
	row, ok := s.resolve(id)
	if !ok {
		return false
	}
	i := s.memberIndex(row, e)
	if i < 0 {
		return false
	}
	start, n := s.Start[row], s.Len[row]
	copy(s.Members[start+i:start+n-1], s.Members[start+i+1:start+n])
	s.Len[row]--
	return true
}

// GroupClear empties g (Len=0) without freeing the slot. Stale ⇒ no-op.
func (s *GroupStore) GroupClear(id GroupID) {
	if row, ok := s.resolve(id); ok {
		s.Len[row] = 0
	}
}

// GroupEach visits g's members in insertion order. The count is snapshotted
// at entry and the index is re-clamped to the live Len each step, so a
// callback that removes members cannot read past the span (safe in-loop
// removal); note a swap-Remove inside the loop may skip the swapped-in
// member — use GroupRemoveOrdered or collect-then-remove if completeness
// matters. Stale handle ⇒ no visits.
func (s *GroupStore) GroupEach(id GroupID, fn func(EntityID)) {
	row, ok := s.resolve(id)
	if !ok {
		return
	}
	n := s.Len[row]
	for i := int32(0); i < n && i < s.Len[row]; i++ {
		fn(s.Members[s.Start[row]+i])
	}
}

func (s *GroupStore) assert(msg string, id GroupID) {
	if s.DebugAssert != nil {
		s.DebugAssert(msg, id)
	}
}

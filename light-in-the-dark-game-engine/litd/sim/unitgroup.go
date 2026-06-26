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

	// Member-arena span allocator (#613). A group's Start/Cap are no longer a
	// fixed partition: CreateGroup reserves nothing, the first add reserves an
	// initialGroupSpan span, and growth past Cap relocates the group to a
	// best-fit free span (double-or-need). freeSpans is the sorted, coalesced
	// list of free arena regions — a DERIVED index, never hashed or serialized;
	// it is rebuilt from the live groups' spans on load. Preallocated so the
	// allocator is zero-alloc.
	freeSpans []span
	memberCap int32

	// Dropped counts add attempts that failed because a group's span was
	// full (#561) or CreateGroup attempts that failed on pool exhaustion.
	// Part of hashed state (#565) so a capacity divergence fails closed.
	DroppedGroups  uint32
	DroppedMembers uint32

	DebugAssert func(msg string, id GroupID)
}

// span is a contiguous region of the member arena: Members[start:start+length].
type span struct{ start, length int32 }

// initialGroupSpan is the member-arena span a group reserves on its first add.
// Growth doubles from here (8→16→…), best-fit, with relocation.
const initialGroupSpan int32 = 8

// NewGroupStore returns a pool of `groupCap` group slots over a shared Members
// arena of `memberCap` slots, managed by a best-fit span allocator (#613): a
// group reserves arena space on demand and grows by relocation, so one large
// group and many small ones share the same arena (R-UGR-7). Both caps must be
// positive and fit the 24-bit index space.
func NewGroupStore(groupCap, memberCap int) *GroupStore {
	if groupCap <= 0 || groupCap >= 1<<24 {
		panic("sim: group capacity must be in (0, 2^24)")
	}
	if memberCap <= 0 || memberCap >= 1<<24 {
		panic("sim: group member capacity must be in (0, 2^24)")
	}
	n := groupCap + 1 // slot 0 reserved
	s := &GroupStore{
		Start:     make([]int32, n),
		Len:       make([]int32, n),
		Cap:       make([]int32, n),
		Gen:       make([]uint8, n),
		live:      make([]bool, n),
		Members:   make([]EntityID, memberCap),
		free:      make([]int32, 0, groupCap),
		freeSpans: make([]span, 0, groupCap+1),
		memberCap: int32(memberCap),
	}
	// Whole arena starts free; group slots seed low-first via LIFO (stable).
	s.freeSpans = append(s.freeSpans, span{start: 0, length: int32(memberCap)})
	for i := groupCap; i >= 1; i-- {
		s.free = append(s.free, int32(i))
	}
	return s
}

// GroupCap is the number of usable group slots (excludes reserved slot 0).
func (s *GroupStore) GroupCap() int { return len(s.live) - 1 }

// MembersPerGroup is the largest a single group can grow to — the whole arena
// (the span allocator lifted the v1 fixed per-group cap, #613).
func (s *GroupStore) MembersPerGroup() int32 { return s.memberCap }

// spanAlloc reserves `need` contiguous arena slots via best fit (smallest free
// span that fits), splitting the chosen span. Returns the start and ok=false on
// arena exhaustion. Zero-alloc (the free-span slice only shrinks/rewrites).
func (s *GroupStore) spanAlloc(need int32) (int32, bool) {
	if need <= 0 {
		return 0, true
	}
	best := -1
	var bestLen int32
	for i := range s.freeSpans {
		l := s.freeSpans[i].length
		if l >= need && (best == -1 || l < bestLen) {
			best, bestLen = i, l
		}
	}
	if best == -1 {
		return 0, false
	}
	start := s.freeSpans[best].start
	if bestLen == need {
		s.freeSpans = append(s.freeSpans[:best], s.freeSpans[best+1:]...) // consume whole span
	} else {
		s.freeSpans[best].start += need // shrink from the front
		s.freeSpans[best].length -= need
	}
	return start, true
}

// spanFree returns [start,start+length) to the free list, inserting in sorted
// order and coalescing with adjacent free spans. Zero-alloc in steady state.
func (s *GroupStore) spanFree(start, length int32) {
	if length <= 0 {
		return
	}
	// Insertion point (sorted by start).
	i := 0
	for i < len(s.freeSpans) && s.freeSpans[i].start < start {
		i++
	}
	s.freeSpans = append(s.freeSpans, span{})
	copy(s.freeSpans[i+1:], s.freeSpans[i:])
	s.freeSpans[i] = span{start: start, length: length}
	// Coalesce with the right neighbour, then the left.
	if i+1 < len(s.freeSpans) && s.freeSpans[i].start+s.freeSpans[i].length == s.freeSpans[i+1].start {
		s.freeSpans[i].length += s.freeSpans[i+1].length
		s.freeSpans = append(s.freeSpans[:i+1], s.freeSpans[i+2:]...)
	}
	if i > 0 && s.freeSpans[i-1].start+s.freeSpans[i-1].length == s.freeSpans[i].start {
		s.freeSpans[i-1].length += s.freeSpans[i].length
		s.freeSpans = append(s.freeSpans[:i], s.freeSpans[i+1:]...)
	}
}

// growGroup relocates group row to a span holding at least `need` members,
// preserving member order. Doubles the current cap (or initialGroupSpan for the
// first span), falling back to an exact `need` fit. Returns false on genuine
// arena exhaustion, leaving the group's existing span intact. Zero-alloc.
func (s *GroupStore) growGroup(row, need int32) bool {
	target := initialGroupSpan
	if s.Cap[row] > 0 {
		target = s.Cap[row] * 2
	}
	if target < need {
		target = need
	}
	// Case 1/2: a disjoint free span fits (preferred target, else exact need).
	// Alloc first, copy, then free the old span — no overlap.
	if start, ok := s.spanAlloc(target); ok {
		s.relocateInto(row, start, target)
		return true
	}
	if start, ok := s.spanAlloc(need); ok {
		s.relocateInto(row, start, need)
		return true
	}
	// Case 3: the only room is this group's own span plus adjacent free. Free
	// it first (coalescing), then alloc — the copy may overlap (copy is
	// memmove-safe). On failure, restore the original span exactly so the
	// group's members are never lost.
	if s.Cap[row] == 0 {
		return false // a fresh group with no span and no free arena
	}
	oldStart, oldCap, oldLen := s.Start[row], s.Cap[row], s.Len[row]
	s.spanFree(oldStart, oldCap)
	want := target
	start, ok := s.spanAlloc(want)
	if !ok {
		want = need
		start, ok = s.spanAlloc(want)
	}
	if !ok {
		s.reclaimExact(oldStart, oldCap) // genuine exhaustion: undo the free
		return false
	}
	copy(s.Members[start:start+oldLen], s.Members[oldStart:oldStart+oldLen])
	s.Start[row], s.Cap[row] = start, want
	return true
}

// relocateInto moves group row's members into an already-allocated [start,cap)
// span (disjoint from the old one) and frees the old span.
func (s *GroupStore) relocateInto(row, start, cap int32) {
	if s.Cap[row] > 0 {
		copy(s.Members[start:start+s.Len[row]], s.Members[s.Start[row]:s.Start[row]+s.Len[row]])
		s.spanFree(s.Start[row], s.Cap[row])
	}
	s.Start[row], s.Cap[row] = start, cap
}

// reclaimExact removes [start,start+length) from the free list — the exact undo
// of a spanFree, used to roll back a failed relocation. The range lies wholly
// within one free span (it was just freed); carve it out, splitting as needed.
func (s *GroupStore) reclaimExact(start, length int32) {
	end := start + length
	for i := range s.freeSpans {
		fs := s.freeSpans[i]
		if start < fs.start || end > fs.start+fs.length {
			continue
		}
		left := span{start: fs.start, length: start - fs.start}
		right := span{start: end, length: fs.start + fs.length - end}
		switch {
		case left.length > 0 && right.length > 0:
			s.freeSpans[i] = left
			s.freeSpans = append(s.freeSpans, span{})
			copy(s.freeSpans[i+2:], s.freeSpans[i+1:])
			s.freeSpans[i+1] = right
		case left.length > 0:
			s.freeSpans[i] = left
		case right.length > 0:
			s.freeSpans[i] = right
		default:
			s.freeSpans = append(s.freeSpans[:i], s.freeSpans[i+1:]...)
		}
		return
	}
}

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
	s.Start[row] = 0 // no span reserved until the first add (lazy)
	s.Cap[row] = 0
	s.count++
	return makeGroupID(uint32(row), s.Gen[row])
}

// DestroyGroup frees a group's slot, bumping its generation so every
// outstanding handle goes stale. Idempotent: destroying a stale/free
// group is a no-op (returns false). The group's member span returns to the
// arena allocator (coalesced) so other groups can reuse it (#613).
func (s *GroupStore) DestroyGroup(id GroupID) bool {
	row, ok := s.resolve(id)
	if !ok {
		s.assert("DestroyGroup of stale/absent group", id)
		return false
	}
	if s.Cap[row] > 0 {
		s.spanFree(s.Start[row], s.Cap[row])
	}
	s.live[row] = false
	s.Gen[row]++
	s.Len[row] = 0
	s.Start[row] = 0
	s.Cap[row] = 0
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
		// Grow into a larger best-fit span (relocating, order-preserving). Only
		// arena exhaustion drops the member now (#613, R-UGR-7).
		if !s.growGroup(row, s.Len[row]+1) {
			s.DroppedMembers++
			return false
		}
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

// ---------------------------------------------------------------------
// Set algebra (#562). Each writes the result into a preallocated dst
// group (cleared first), reading a and b in their member order, so the
// result order is a deterministic function of the inputs. Zero-alloc:
// membership tests reuse the linear span scan, no temporaries. dst MUST
// be distinct from a and b — clearing dst first would otherwise destroy
// a source mid-read; an aliasing call asserts and is a no-op (false).
// Members beyond dst's span cap are dropped (DroppedMembers++), same as
// GroupAdd.
// ---------------------------------------------------------------------

// distinctDst reports whether dst is a live group distinct from a and b
// (the precondition for the algebra ops).
func (s *GroupStore) distinctDst(dst, a, b GroupID) (row int32, ok bool) {
	r, live := s.resolve(dst)
	if !live {
		return 0, false
	}
	if dst == a || dst == b {
		s.assert("group algebra dst aliases a source", dst)
		return 0, false
	}
	return r, true
}

// GroupUnion sets dst = a ∪ b (a's members in order, then b's not already
// present). Returns false on a stale/aliasing dst.
func (s *GroupStore) GroupUnion(dst, a, b GroupID) bool {
	dstRow, ok := s.distinctDst(dst, a, b)
	if !ok {
		return false
	}
	s.Len[dstRow] = 0
	s.appendAll(dst, a)
	s.appendAll(dst, b)
	return true
}

// GroupIntersect sets dst = a ∩ b (a's members, in a's order, that are
// also in b). Returns false on a stale/aliasing dst.
func (s *GroupStore) GroupIntersect(dst, a, b GroupID) bool {
	dstRow, ok := s.distinctDst(dst, a, b)
	if !ok {
		return false
	}
	s.Len[dstRow] = 0
	aRow, aok := s.resolve(a)
	if !aok {
		return true // a empty/stale → empty intersection
	}
	start, n := s.Start[aRow], s.Len[aRow]
	for i := int32(0); i < n; i++ {
		e := s.Members[start+i]
		if s.GroupContains(b, e) {
			s.GroupAdd(dst, e)
		}
	}
	return true
}

// GroupDifference sets dst = a ∖ b (a's members, in a's order, not in b).
// Returns false on a stale/aliasing dst.
func (s *GroupStore) GroupDifference(dst, a, b GroupID) bool {
	dstRow, ok := s.distinctDst(dst, a, b)
	if !ok {
		return false
	}
	s.Len[dstRow] = 0
	aRow, aok := s.resolve(a)
	if !aok {
		return true
	}
	start, n := s.Start[aRow], s.Len[aRow]
	for i := int32(0); i < n; i++ {
		e := s.Members[start+i]
		if !s.GroupContains(b, e) {
			s.GroupAdd(dst, e)
		}
	}
	return true
}

// GroupCopy sets dst = src (src's members in order). dst must differ from
// src. Returns false on a stale/aliasing dst.
func (s *GroupStore) GroupCopy(dst, src GroupID) bool {
	dstRow, ok := s.distinctDst(dst, src, src)
	if !ok {
		return false
	}
	s.Len[dstRow] = 0
	s.appendAll(dst, src)
	return true
}

// appendAll adds every member of src to dst (unique, ordered). src may be
// stale (no-op). Internal helper for the algebra ops.
func (s *GroupStore) appendAll(dst, src GroupID) {
	row, ok := s.resolve(src)
	if !ok {
		return
	}
	start, n := s.Start[row], s.Len[row]
	for i := int32(0); i < n; i++ {
		s.GroupAdd(dst, s.Members[start+i])
	}
}

// PruneEntities removes every entity in dead from every live group,
// preserving insertion order (stable compaction, spec §3) so survivors'
// relative order is unperturbed. One pass per live group; each span is
// scanned in index order and compacted in place. Returns the total
// (group, member) removals. Zero-alloc. Called from the cleanup phase
// with the tick's killed set (#564, R-UGR-6) so a dead unit never lingers
// in a group's count, iteration, or serialized members.
func (s *GroupStore) PruneEntities(dead []EntityID) int {
	if len(dead) == 0 || s.count == 0 {
		return 0
	}
	removed := 0
	seen := int32(0)
	for row := int32(1); row < int32(len(s.live)) && seen < s.count; row++ {
		if !s.live[row] {
			continue
		}
		seen++
		start, n := s.Start[row], s.Len[row]
		wcur := int32(0) // write cursor for in-place compaction
		for r := int32(0); r < n; r++ {
			e := s.Members[start+r]
			drop := false
			for _, d := range dead {
				if d == e {
					drop = true
					break
				}
			}
			if drop {
				removed++
				continue
			}
			if wcur != r {
				s.Members[start+wcur] = e
			}
			wcur++
		}
		s.Len[row] = wcur
	}
	return removed
}

// loadReset clears the store to the empty-but-allocated state before
// applySave writes the decoded rows. The span allocator is reset to a single
// whole-arena free span; applySave re-reserves each live group's span (#613).
// Only the member bytes + Start/Len/Cap are the serialized state — the
// freeSpans index is derived and rebuilt here.
func (s *GroupStore) loadReset() {
	for i := range s.live {
		s.live[i] = false
		s.Len[i] = 0
		s.Start[i] = 0
		s.Cap[i] = 0
	}
	s.free = s.free[:0]
	s.freeSpans = s.freeSpans[:0]
	s.freeSpans = append(s.freeSpans, span{start: 0, length: s.memberCap})
	s.count = 0
	s.DroppedGroups = 0
	s.DroppedMembers = 0
}

func (s *GroupStore) assert(msg string, id GroupID) {
	if s.DebugAssert != nil {
		s.DebugAssert(msg, id)
	}
}

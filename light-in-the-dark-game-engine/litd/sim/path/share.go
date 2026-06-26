package path

// Path sharing for group orders (pathfinding.md §3.2, §5): a group
// ordered to one destination computes ONE representative path per
// collision class present — not one per unit. This kills the
// 1,000-search burst (the D-29 spike measured 1,000 full repaths at
// ≈2.3 s). Members are assigned formation offsets around the
// destination by sorted entity index (deterministic), individually
// steer to their offset point, and collapse toward single-file when
// offsets are unwalkable. No formation state survives the order:
// planning is a pure function, written into caller buffers.
//
// Divergence rule: a member re-paths INDIVIDUALLY only when it
// genuinely cannot follow the shared path — start displaced beyond
// DivergeCells from the representative, or sitting in a different
// connected component. A merely-unwalkable offset goal collapses
// toward the destination instead (§5 single-file rule) and stays on
// the shared path. The divergence threshold is data, not code.

// DefaultDivergeCells: a member starting farther than this (octile
// cells) from its class representative re-paths individually.
const DefaultDivergeCells int32 = 24

// shareMemberCap bounds one group order (WC3 selection is 12; the
// engine allows control-group merges far past that).
const shareMemberCap = 256

// GroupMember is one unit in a group order.
type GroupMember struct {
	ID    uint32 // entity handle upstairs; sort key for determinism
	X, Y  int32  // current cell
	Class uint8  // collision-class index = Sharer queue index
}

// MemberPlan is the planning verdict for one member.
type MemberPlan struct {
	ID           uint32
	Class        uint8
	Slot         int32 // formation slot (position in sorted order)
	OffX, OffY   int32 // assigned formation offset
	GoalX, GoalY int32 // personal goal after validation/collapse
	Collapsed    bool  // offset was unwalkable → single-file toward dest
	OwnPath      bool  // diverged → individual request enqueued
	SharedRep    bool  // this member IS its class representative
}

// Sharer plans group orders over one Queue per collision class.
type Sharer struct {
	queues []*Queue

	// DivergeCells is the individual-re-path displacement threshold
	// in octile cells (data, tunable per balance table).
	DivergeCells int32

	sorted [shareMemberCap]GroupMember // sort scratch
}

// NewSharer wires one queue per collision class (index = class id).
func NewSharer(queues []*Queue) *Sharer {
	if len(queues) == 0 {
		panic("path: NewSharer needs at least one class queue")
	}
	for i, q := range queues {
		if q == nil {
			panic("path: NewSharer nil queue")
		}
		_ = i
	}
	return &Sharer{queues: queues, DivergeCells: DefaultDivergeCells}
}

// ringOffset returns formation slot k's offset around the
// destination: slot 0 sits on it, later slots walk concentric rings
// in the same fixed order as the nearest-reachable scan (top row
// west→east, bottom row west→east, west column, east column).
func ringOffset(k int32) (int32, int32) {
	if k == 0 {
		return 0, 0
	}
	r := int32(1)
	k--
	for k >= 8*r {
		k -= 8 * r
		r++
	}
	top := 2*r + 1
	if k < top {
		return -r + k, r
	}
	k -= top
	if k < top {
		return -r + k, -r
	}
	k -= top
	side := 2*r - 1
	if k < side {
		return -r, -r + 1 + k
	}
	k -= side
	return r, -r + 1 + k
}

// PlanGroupOrder plans one group move to (destX, destY): sorts the
// members by ID, issues one representative path request per class
// present plus one per diverged member, and fills plans (caller
// buffer, len(members) entries). Returns (searches issued, next free
// seq, ok). Fails closed on oversize groups or full plan buffers.
func (s *Sharer) PlanGroupOrder(tick uint32, seq uint16, destX, destY int32,
	members []GroupMember, plans []MemberPlan) (int, uint16, bool) {
	if len(members) == 0 || len(members) > shareMemberCap || len(plans) < len(members) {
		return 0, seq, false
	}
	if !InBounds(destX, destY) {
		return 0, seq, false
	}
	// deterministic order: sorted entity index (insertion sort on a
	// fixed scratch — no allocation, stable enough since IDs unique)
	n := copy(s.sorted[:], members)
	for i := 1; i < n; i++ {
		m := s.sorted[i]
		j := i - 1
		for j >= 0 && s.sorted[j].ID > m.ID {
			s.sorted[j+1] = s.sorted[j]
			j--
		}
		s.sorted[j+1] = m
	}

	searches := 0
	var classSeen [256]bool
	var classRep [256]GroupMember
	for i := 0; i < n; i++ {
		m := s.sorted[i]
		if int(m.Class) >= len(s.queues) {
			return searches, seq, false // unknown class: fail closed
		}
		if !classSeen[m.Class] {
			classSeen[m.Class] = true
			classRep[m.Class] = m
			if !s.queues[m.Class].Enqueue(Request{
				Owner: m.ID, SX: m.X, SY: m.Y, TX: destX, TY: destY,
				Tick: tick, Seq: seq,
			}) {
				return searches, seq, false
			}
			seq++
			searches++
		}
	}

	for i := 0; i < n; i++ {
		m := s.sorted[i]
		h := s.queues[m.Class].h
		offX, offY := ringOffset(int32(i))
		p := MemberPlan{
			ID: m.ID, Class: m.Class, Slot: int32(i),
			OffX: offX, OffY: offY,
			GoalX: destX + offX, GoalY: destY + offY,
		}
		rep := classRep[m.Class]
		p.SharedRep = m.ID == rep.ID

		destLabel := h.Label(destX, destY)
		if !InBounds(p.GoalX, p.GoalY) || h.Label(p.GoalX, p.GoalY) != destLabel {
			// §5: unwalkable offset collapses toward single-file
			p.GoalX, p.GoalY = destX, destY
			p.Collapsed = true
		}
		// divergence: wrong component, or displaced beyond threshold
		memberLabel := h.Label(m.X, m.Y)
		displaced := Octile(m.X, m.Y, rep.X, rep.Y) > s.DivergeCells*CostCardinal
		if !p.SharedRep && (memberLabel != h.Label(rep.X, rep.Y) || displaced) {
			p.OwnPath = true
			if !s.queues[m.Class].Enqueue(Request{
				Owner: m.ID, SX: m.X, SY: m.Y, TX: p.GoalX, TY: p.GoalY,
				Tick: tick, Seq: seq,
			}) {
				return searches, seq, false
			}
			seq++
			searches++
		}
		plans[i] = p
	}
	return searches, seq, true
}

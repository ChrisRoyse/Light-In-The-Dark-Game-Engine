package path

import (
	"strings"
	"testing"
)

// shareFixture: open 96×96 map, nClasses identical ground layers
// (distinct Layer/HPA/Queue instances so per-class accounting is
// real), one Sharer.
func shareFixture(nClasses int) (*Grid, *Sharer) {
	g := NewGrid()
	for y := int32(0); y < 96; y++ {
		for x := int32(0); x < 96; x++ {
			g.OrFlags(x, y, Walkable)
		}
	}
	queues := make([]*Queue, nClasses)
	for c := 0; c < nClasses; c++ {
		d := NewDilatedSet(g, []LayerKey{{Required: Walkable, Blocked: OccupiedStatic}})
		d.RecomputeAll()
		h := NewHPA(g, d.Layer(0), NewSearcher(g))
		queues[c] = NewQueue(h, NewPathStore(64, runWaypointCap))
	}
	return g, NewSharer(queues)
}

func planTable(plans []MemberPlan) string {
	var b strings.Builder
	for _, p := range plans {
		b.WriteString("[id=")
		b.WriteString(itoa(int32(p.ID)))
		b.WriteString(" slot=")
		b.WriteString(itoa(p.Slot))
		b.WriteString(" off=(")
		b.WriteString(itoa(p.OffX))
		b.WriteString(",")
		b.WriteString(itoa(p.OffY))
		b.WriteString(") goal=(")
		b.WriteString(itoa(p.GoalX))
		b.WriteString(",")
		b.WriteString(itoa(p.GoalY))
		b.WriteString(")")
		if p.Collapsed {
			b.WriteString(" COLLAPSED")
		}
		if p.OwnPath {
			b.WriteString(" OWNPATH")
		}
		if p.SharedRep {
			b.WriteString(" REP")
		}
		b.WriteString("] ")
	}
	return b.String()
}

// 50 units, one collision class → exactly ONE search.
func TestPathShareSingleSearchPerClass(t *testing.T) {
	_, s := shareFixture(1)
	members := make([]GroupMember, 50)
	for i := range members {
		members[i] = GroupMember{ID: uint32(100 + i), X: int32(5 + i%10), Y: int32(5 + i/10), Class: 0}
	}
	plans := make([]MemberPlan, 50)
	searches, _, ok := s.PlanGroupOrder(1, 0, 80, 80, members, plans)
	t.Logf("50 units, 1 class -> searches=%d (must be exactly 1)", searches)
	if !ok || searches != 1 {
		t.Fatalf("one class must cost one search: %d", searches)
	}
	spent := s.queues[0].Service(DefaultExpansionBudget)
	completions := 0
	for _, ev := range s.queues[0].Log() {
		if ev.Done && ev.Status == StatusCompleted {
			completions++
		}
	}
	t.Logf("queue service: %d expansions, %d completion(s) — the single shared path", spent, completions)
	if completions != 1 {
		t.Fatalf("queue must show exactly the one shared search: %d", completions)
	}
	reps := 0
	for _, p := range plans {
		if p.SharedRep {
			reps++
			if p.ID != 100 {
				t.Fatalf("representative must be lowest ID (100), got %d", p.ID)
			}
		}
		if p.OwnPath {
			t.Fatalf("tight group must not diverge: %+v", p)
		}
	}
	if reps != 1 {
		t.Fatalf("exactly one representative: %d", reps)
	}
}

// Edge 1: 50 units across 2 collision classes → exactly 2 searches.
func TestPathShareTwoClasses(t *testing.T) {
	_, s := shareFixture(2)
	members := make([]GroupMember, 50)
	for i := range members {
		members[i] = GroupMember{ID: uint32(200 + i), X: int32(5 + i%10), Y: int32(5 + i/10), Class: uint8(i % 2)}
	}
	plans := make([]MemberPlan, 50)
	searches, _, ok := s.PlanGroupOrder(1, 0, 80, 80, members, plans)
	c0 := s.queues[0].Pending()
	c1 := s.queues[1].Pending()
	t.Logf("50 units, 2 classes -> searches=%d (class0 queue=%d, class1 queue=%d; 25 members each)", searches, c0, c1)
	if !ok || searches != 2 || c0 != 1 || c1 != 1 {
		t.Fatalf("two classes must cost exactly two searches: %d (%d/%d)", searches, c0, c1)
	}
}

// Edge 3: identical offset assignment across runs, regardless of
// member arrival order.
func TestPathShareOffsetTableDeterministic(t *testing.T) {
	mk := func(shuffle bool) string {
		_, s := shareFixture(1)
		members := make([]GroupMember, 12)
		for i := range members {
			members[i] = GroupMember{ID: uint32(300 + i), X: int32(10 + i), Y: 10, Class: 0}
		}
		if shuffle { // reversed arrival order: sort must normalize
			for i, j := 0, len(members)-1; i < j; i, j = i+1, j-1 {
				members[i], members[j] = members[j], members[i]
			}
		}
		plans := make([]MemberPlan, 12)
		s.PlanGroupOrder(1, 0, 60, 60, members, plans)
		return planTable(plans)
	}
	a, b := mk(false), mk(true)
	t.Logf("run1 (in order):  %s", a)
	t.Logf("run2 (reversed):  %s", b)
	if a != b {
		t.Fatalf("offset tables must be identical regardless of arrival order")
	}
}

// Edge 2: a member in a different component re-paths individually; a
// member whose OFFSET is merely unwalkable collapses single-file (§5)
// without an extra search.
func TestPathShareDivergenceRepath(t *testing.T) {
	g := NewGrid()
	for y := int32(0); y < 96; y++ {
		for x := int32(0); x < 96; x++ {
			g.OrFlags(x, y, Walkable)
		}
	}
	// island for the diverged member at (90,90), walled off
	for i := int32(86); i <= 94; i++ {
		g.ClearFlags(i, 86, Walkable)
		g.ClearFlags(86, i, Walkable)
	}
	// destination at (60,60); block the ring-1 slot cells north of it
	// so some offsets collapse
	g.ClearFlags(59, 61, Walkable)
	g.ClearFlags(60, 61, Walkable)
	g.ClearFlags(61, 61, Walkable)
	d := NewDilatedSet(g, []LayerKey{{Required: Walkable}})
	d.RecomputeAll()
	h := NewHPA(g, d.Layer(0), NewSearcher(g))
	q := NewQueue(h, NewPathStore(64, runWaypointCap))
	s := NewSharer([]*Queue{q})

	members := []GroupMember{
		{ID: 400, X: 5, Y: 5, Class: 0},   // representative
		{ID: 401, X: 6, Y: 5, Class: 0},   // slot 1 → offset (-1,1) = (59,61) blocked → collapse
		{ID: 402, X: 7, Y: 5, Class: 0},   // slot 2 → (0,1) = (60,61) blocked → collapse
		{ID: 403, X: 90, Y: 90, Class: 0}, // walled island → OWN path
	}
	plans := make([]MemberPlan, len(members))
	searches, _, ok := s.PlanGroupOrder(1, 0, 60, 60, members, plans)
	t.Logf("plan: %s", planTable(plans))
	repaths := []uint32{}
	collapsed := []uint32{}
	for _, p := range plans {
		if p.OwnPath {
			repaths = append(repaths, p.ID)
		}
		if p.Collapsed {
			collapsed = append(collapsed, p.ID)
		}
	}
	t.Logf("searches=%d re-path list=%v collapsed list=%v (collapse costs no search)", searches, repaths, collapsed)
	if !ok || searches != 2 {
		t.Fatalf("1 shared + 1 diverged = 2 searches, got %d", searches)
	}
	if len(repaths) != 1 || repaths[0] != 403 {
		t.Fatalf("only the island member re-paths: %v", repaths)
	}
	if len(collapsed) < 2 {
		t.Fatalf("blocked offsets must collapse single-file: %v", collapsed)
	}
}

// Edge 4: destination inside a 1-wide corridor — every ring offset is
// unwalkable, the whole group collapses to single-file goals.
func TestPathShareCorridorCollapse(t *testing.T) {
	g := NewGrid()
	for x := int32(0); x < 64; x++ {
		for y := int32(0); y < 16; y++ {
			g.OrFlags(x, y, Walkable) // staging room
		}
	}
	for y := int32(16); y < 48; y++ {
		g.OrFlags(30, y, Walkable) // 1-wide vertical corridor
	}
	d := NewDilatedSet(g, []LayerKey{{Required: Walkable}})
	d.RecomputeAll()
	h := NewHPA(g, d.Layer(0), NewSearcher(g))
	q := NewQueue(h, NewPathStore(64, runWaypointCap))
	s := NewSharer([]*Queue{q})

	members := make([]GroupMember, 6)
	for i := range members {
		members[i] = GroupMember{ID: uint32(500 + i), X: int32(10 + i), Y: 5, Class: 0}
	}
	plans := make([]MemberPlan, 6)
	searches, _, ok := s.PlanGroupOrder(1, 0, 30, 40, members, plans)
	t.Logf("corridor dest (30,40), 1 cell wide: %s", planTable(plans))
	if !ok || searches != 1 {
		t.Fatalf("collapse must not add searches: %d", searches)
	}
	// single-file = every goal sits ON the 1-wide corridor column:
	// side offsets collapse to the dest cell, vertical offsets land
	// on corridor cells above/below it — a file, not a blob.
	collapsedCnt := 0
	for i, p := range plans {
		if i == 0 {
			if p.Collapsed || p.OffX != 0 || p.OffY != 0 {
				t.Fatalf("slot 0 sits on the destination: %+v", p)
			}
			continue
		}
		if p.GoalX != 30 {
			t.Fatalf("member %d goal (%d,%d) off the corridor column — not single file", p.ID, p.GoalX, p.GoalY)
		}
		if p.Collapsed {
			collapsedCnt++
		}
	}
	t.Logf("all %d goals on corridor column x=30 (single file); %d side offsets collapsed onto the dest cell", len(members), collapsedCnt)
	if collapsedCnt == 0 {
		t.Fatal("side offsets must have collapsed")
	}
}

// Re-planning after completion is stateless: identical plans again.
func TestPathShareNoPersistentState(t *testing.T) {
	_, s := shareFixture(1)
	members := make([]GroupMember, 8)
	for i := range members {
		members[i] = GroupMember{ID: uint32(600 + i), X: int32(4 + i), Y: 4, Class: 0}
	}
	plans1 := make([]MemberPlan, 8)
	s.PlanGroupOrder(1, 0, 50, 50, members, plans1)
	for s.queues[0].InFlight() > 0 {
		s.queues[0].Service(DefaultExpansionBudget)
	}
	plans2 := make([]MemberPlan, 8)
	searches2, _, _ := s.PlanGroupOrder(2, 0, 50, 50, members, plans2)
	a, b := planTable(plans1), planTable(plans2)
	t.Logf("order 1: %s", a)
	t.Logf("order 2 (after completion): %s", b)
	if a != b || searches2 != 1 {
		t.Fatalf("no formation state may survive an order")
	}
}

func BenchmarkPathSharePlan(b *testing.B) {
	_, s := shareFixture(1)
	members := make([]GroupMember, 50)
	for i := range members {
		members[i] = GroupMember{ID: uint32(i + 1), X: int32(5 + i%10), Y: int32(5 + i/10), Class: 0}
	}
	plans := make([]MemberPlan, 50)
	b.ReportAllocs()
	b.ResetTimer()
	tick := uint32(1)
	for i := 0; i < b.N; i++ {
		s.PlanGroupOrder(tick, 0, 80, 80, members, plans)
		for s.queues[0].InFlight() > 0 {
			s.queues[0].Service(DefaultExpansionBudget)
		}
		for sl := range s.queues[0].ps.slots {
			if s.queues[0].ps.slots[sl].live {
				s.queues[0].ps.Release(makePathID(s.queues[0].ps.slots[sl].gen, int32(sl)))
			}
		}
		tick++
	}
}

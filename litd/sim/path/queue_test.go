package path

import (
	"strings"
	"testing"
)

func newQueueFixture() (*Grid, *DilatedSet, *HPA, *Queue) {
	g, d, h := blockedFixture()
	ps := NewPathStore(RequestCap, runWaypointCap)
	q := NewQueue(h, ps)
	return g, d, h, q
}

func logString(events []ServiceEvent) string {
	var b strings.Builder
	for _, ev := range events {
		b.WriteString("(t")
		b.WriteString(itoa(int32(ev.Tick)))
		b.WriteString(" s")
		b.WriteString(itoa(int32(ev.Seq)))
		b.WriteString(" exp=")
		b.WriteString(itoa(ev.Expansions))
		if ev.Resumed {
			b.WriteString(" resumed")
		}
		if ev.Parked {
			b.WriteString(" PARKED")
		} else {
			b.WriteString(" ")
			b.WriteString(ev.Status.String())
		}
		b.WriteString(") ")
	}
	return b.String()
}

// reachableGoals returns n far-apart reachable goals on the blocked
// fixture, deterministically.
func reachableGoals(h *HPA, n int) [][2]int32 {
	goals := make([][2]int32, 0, n)
	for y := int32(80); y < GridSize && len(goals) < n; y += 36 {
		for x := int32(420); x < GridSize && len(goals) < n; x += 24 {
			if h.Reachable(2, 2, x, y) {
				goals = append(goals, [2]int32{x, y})
			}
		}
	}
	return goals
}

// The §6 worked example, shape-preserved: a 12-request burst whose
// total exceeds one tick's budget. The first requests complete,
// EXACTLY ONE search parks at the boundary, the remainder wait —
// identically on every run. (HPA cut the example's 130k to ~37k
// total, so the budget scales to 30k to keep the boundary inside
// the burst; the semantics under test are unchanged.)
func TestRequestQueueWorkedExample(t *testing.T) {
	const budget = 30_000
	_, _, h, q := newQueueFixture()
	goals := reachableGoals(h, 12)
	if len(goals) < 12 {
		t.Fatalf("fixture yielded only %d reachable goals", len(goals))
	}
	for i, gpt := range goals {
		if !q.Enqueue(Request{Owner: uint32(i), SX: 2, SY: 2, TX: gpt[0], TY: gpt[1], Tick: 1, Seq: uint16(i)}) {
			t.Fatalf("enqueue %d refused", i)
		}
	}
	totalSpent := int32(0)
	tick := 0
	completed, parkEvents := 0, 0
	for q.InFlight() > 0 && tick < 50 {
		spent := q.Service(budget)
		totalSpent += spent
		tick++
		t.Logf("tick %d service (spent %d/%d): %s", tick, spent, budget, logString(q.Log()))
		parksThisTick := 0
		for i, ev := range q.Log() {
			if ev.Parked {
				parkEvents++
				parksThisTick++
				if i != len(q.Log())-1 {
					t.Fatalf("a park must be the tick's final event")
				}
			}
			if ev.Done {
				if ev.Status != StatusCompleted {
					t.Fatalf("request t%d s%d ended %s", ev.Tick, ev.Seq, ev.Status)
				}
				completed++
			}
		}
		if parksThisTick > 1 {
			t.Fatalf("only the head search may park: %d parks in one tick", parksThisTick)
		}
	}
	t.Logf("burst of 12 totaling %d expansions under %d budget: %d completed over %d ticks, %d park event(s)",
		totalSpent, budget, completed, tick, parkEvents)
	if completed != 12 {
		t.Fatalf("all 12 must complete, got %d", completed)
	}
	if tick < 2 || parkEvents == 0 {
		t.Fatalf("burst must exceed one budget and park at the boundary: ticks=%d parks=%d", tick, parkEvents)
	}
}

// Edge 2: two runs of the same burst → identical service schedules.
func TestRequestQueueDeterministicSchedule(t *testing.T) {
	runOnce := func() []string {
		_, _, h, q := newQueueFixture()
		goals := reachableGoals(h, 8)
		for i, gpt := range goals {
			q.Enqueue(Request{Owner: uint32(i), SX: 2, SY: 2, TX: gpt[0], TY: gpt[1], Tick: 1, Seq: uint16(i)})
		}
		var logs []string
		for q.InFlight() > 0 {
			q.Service(DefaultExpansionBudget)
			logs = append(logs, logString(q.Log()))
		}
		return logs
	}
	a, b := runOnce(), runOnce()
	for i := range a {
		t.Logf("run1 tick %d: %s", i+1, a[i])
	}
	for i := range b {
		t.Logf("run2 tick %d: %s", i+1, b[i])
	}
	if len(a) != len(b) {
		t.Fatalf("schedules differ in length: %d vs %d", len(a), len(b))
	}
	for i := range a {
		if a[i] != b[i] {
			t.Fatalf("tick %d schedules differ:\n%s\n%s", i+1, a[i], b[i])
		}
	}
}

// Edge 1: a search parked by a tiny budget produces the IDENTICAL
// path to an unbudgeted run.
func TestRequestQueueParkedPathIdentical(t *testing.T) {
	_, _, h, q := newQueueFixture()
	goals := reachableGoals(h, 1)
	tx, ty := goals[0][0], goals[0][1]
	q.Enqueue(Request{SX: 2, SY: 2, TX: tx, TY: ty, Tick: 1, Seq: 0})
	var parked ServiceEvent
	ticks := 0
	parks := 0
	for q.InFlight() > 0 {
		q.Service(2000) // tiny budget: forces multiple parks
		ticks++
		for _, ev := range q.Log() {
			if ev.Parked {
				parks++
			}
			if ev.Done {
				parked = ev
			}
		}
	}
	if parked.Status != StatusCompleted || parks == 0 {
		t.Fatalf("must park then complete: parks=%d status=%s", parks, parked.Status)
	}
	budgeted := q.ps.Waypoints(parked.Path)

	// unbudgeted reference on a FRESH hierarchy (no shared scratch)
	_, _, h2, q2 := newQueueFixture()
	_ = h2
	q2.Enqueue(Request{SX: 2, SY: 2, TX: tx, TY: ty, Tick: 1, Seq: 0})
	q2.Service(1 << 30)
	ref := q2.Log()[0]
	unbudgeted := q2.ps.Waypoints(ref.Path)
	_ = h
	t.Logf("goal (%d,%d): parked %d times over %d ticks; budgeted len=%d, unbudgeted len=%d",
		tx, ty, parks, ticks, len(budgeted), len(unbudgeted))
	t.Logf("budgeted   first/last: %s ... %s", pathString(budgeted[:3]), pathString(budgeted[len(budgeted)-3:]))
	t.Logf("unbudgeted first/last: %s ... %s", pathString(unbudgeted[:3]), pathString(unbudgeted[len(unbudgeted)-3:]))
	if len(budgeted) != len(unbudgeted) {
		t.Fatalf("path lengths differ: %d vs %d", len(budgeted), len(unbudgeted))
	}
	for i := range budgeted {
		if budgeted[i] != unbudgeted[i] {
			t.Fatalf("waypoint %d differs: %d vs %d", i, budgeted[i], unbudgeted[i])
		}
	}
}

// Edge 4: invalidating a parked search re-enqueues from the current
// position and recycles the run slot.
func TestRequestQueueInvalidateParked(t *testing.T) {
	_, _, h, q := newQueueFixture()
	goals := reachableGoals(h, 1)
	tx, ty := goals[0][0], goals[0][1]
	q.Enqueue(Request{SX: 2, SY: 2, TX: tx, TY: ty, Tick: 1, Seq: 0})
	q.Service(1500) // small budget: parks mid-search
	if !q.run.active {
		t.Fatal("search must be parked after tiny budget")
	}
	curX, curY := q.run.px, q.run.py
	t.Logf("parked: run.active=%v progressed to (%d,%d), pending=%d", q.run.active, curX, curY, q.Pending())
	if !q.InvalidateActive(2, 0) {
		t.Fatal("invalidate refused")
	}
	t.Logf("after invalidate: run.active=%v pending=%d; re-enqueued from (%d,%d) toward (%d,%d)",
		q.run.active, q.Pending(), curX, curY, tx, ty)
	if q.run.active || q.Pending() != 1 {
		t.Fatalf("slot must recycle and request re-enqueue: active=%v pending=%d", q.run.active, q.Pending())
	}
	reEnq := q.ring[q.head]
	if reEnq.SX != curX || reEnq.SY != curY || reEnq.TX != tx || reEnq.TY != ty {
		t.Fatalf("re-enqueued request wrong: %+v", reEnq)
	}
	for q.InFlight() > 0 {
		q.Service(DefaultExpansionBudget)
	}
	last := q.Log()[len(q.Log())-1]
	if !last.Done || last.Status != StatusCompleted {
		t.Fatalf("re-enqueued search must complete: %+v", last)
	}
	t.Logf("re-enqueued search completed: path=%08x len=%d", uint32(last.Path), len(q.ps.Waypoints(last.Path)))
}

// Edge 3a: a path longer than the waypoint scratch terminates with
// the best partial path, flagged — never silently truncated. The
// serpentine forces ~8,000 cells of walking against the 4,096 cap.
func TestRequestQueueScratchCapPartial(t *testing.T) {
	g := NewGrid()
	for y := int32(0); y < 33; y++ {
		for x := int32(0); x < GridSize; x++ {
			g.OrFlags(x, y, Walkable)
		}
	}
	// serpentine: walls every other row, gap alternating ends
	for lane := int32(1); lane < 33; lane += 2 {
		gapLeft := (lane/2)%2 == 1
		for x := int32(0); x < GridSize; x++ {
			if gapLeft && x == 0 {
				continue
			}
			if !gapLeft && x == GridSize-1 {
				continue
			}
			g.ClearFlags(x, lane, Walkable)
		}
	}
	d := NewDilatedSet(g, []LayerKey{{Required: Walkable}})
	d.RecomputeAll()
	h := NewHPA(g, d.Layer(0), NewSearcher(g))
	ps := NewPathStore(8, runWaypointCap)
	q := NewQueue(h, ps)
	if !h.Reachable(2, 0, 256, 32) {
		t.Fatal("serpentine must stay connected")
	}
	q.Enqueue(Request{SX: 2, SY: 0, TX: 256, TY: 32, Tick: 1, Seq: 0})
	var done ServiceEvent
	for q.InFlight() > 0 {
		q.Service(1 << 30)
		for _, ev := range q.Log() {
			if ev.Done {
				done = ev
			}
		}
	}
	if done.Status != StatusPartial {
		t.Fatalf("serpentine longer than scratch must deliver partial, got %s", done.Status)
	}
	wp := q.ps.Waypoints(done.Path)
	x, y := cellXY(wp[len(wp)-1])
	t.Logf("serpentine (~8k-cell walk vs %d scratch): status=%s, %d waypoints delivered, frontier (%d,%d) of goal (256,32) — flagged, not silent",
		runWaypointCap, done.Status, len(wp), x, y)
	if len(wp) == 0 || len(wp) > runWaypointCap {
		t.Fatalf("partial waypoints out of range: %d", len(wp))
	}
}

// Edge 3b (white-box): the per-request node cap itself trips to a
// flagged best-partial delivery.
func TestRequestQueueNodeCapPartial(t *testing.T) {
	_, _, h, q := newQueueFixture()
	goals := reachableGoals(h, 1)
	q.Enqueue(Request{SX: 2, SY: 2, TX: goals[0][0], TY: goals[0][1], Tick: 1, Seq: 0})
	q.Service(500) // start the run (coarse + a segment or two), park it
	if !q.run.active {
		t.Fatal("run must be parked")
	}
	before := q.run.nodeCap
	q.run.nodeCap = 1 // force the cap below already-spent fine work
	q.Service(1 << 30)
	last := q.Log()[len(q.Log())-1]
	t.Logf("node cap forced %d->1 on parked run: delivery status=%s done=%v (resumed=%v)", before, last.Status, last.Done, last.Resumed)
	if !last.Done || last.Status != StatusPartial {
		t.Fatalf("tripped node cap must deliver flagged partial: %+v", last)
	}
}

// 513th in-flight request fails deterministically.
func TestRequestQueueCapRefusal(t *testing.T) {
	_, _, _, q := newQueueFixture()
	accepted := 0
	for i := 0; i < RequestCap+5; i++ {
		if q.Enqueue(Request{SX: 2, SY: 2, TX: 30, TY: 30, Tick: 1, Seq: uint16(i)}) {
			accepted++
		}
	}
	t.Logf("offered %d requests: accepted=%d dropped=%d (cap %d)", RequestCap+5, accepted, q.Dropped(), RequestCap)
	if accepted != RequestCap || q.Dropped() != 5 {
		t.Fatalf("cap must refuse exactly past %d: accepted=%d", RequestCap, accepted)
	}
	// out-of-order enqueue asserts and refuses
	asserts := 0
	q.DebugAssert = func(string) { asserts++ }
	if q.Enqueue(Request{SX: 2, SY: 2, TX: 9, TY: 9, Tick: 0, Seq: 0}) || asserts != 1 {
		t.Fatalf("out-of-order enqueue must be refused with assert")
	}
}

func BenchmarkRequestQueueService(b *testing.B) {
	_, _, h, q := newQueueFixture()
	goals := reachableGoals(h, 8)
	b.ReportAllocs()
	b.ResetTimer()
	tick := uint32(1)
	for i := 0; i < b.N; i++ {
		for j, gpt := range goals {
			q.Enqueue(Request{SX: 2, SY: 2, TX: gpt[0], TY: gpt[1], Tick: tick, Seq: uint16(j)})
		}
		for q.InFlight() > 0 {
			q.Service(DefaultExpansionBudget)
		}
		// release delivered paths so the pool never exhausts
		for s := range q.ps.slots {
			if q.ps.slots[s].live {
				q.ps.Release(makePathID(q.ps.slots[s].gen, int32(s)))
			}
		}
		tick++
	}
}

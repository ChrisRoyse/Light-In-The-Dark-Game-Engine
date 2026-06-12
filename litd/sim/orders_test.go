package sim

import (
	"fmt"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim/path"
)

// orderUnit spawns a unit with Movement + Order head at cell (x, y).
func orderUnit(tb testing.TB, w *World, x, y int32, speed fixed.F64) EntityID {
	tb.Helper()
	id, ok := w.CreateUnit(CellCenter(cellIdx(x, y)), 0)
	if !ok || !w.Movements.Add(w.Ents, w.Transforms, id, speed, 0x4000) || !w.Orders.Add(w.Ents, id) {
		tb.Fatal("order unit setup failed")
	}
	return id
}

// dumpQueue renders a unit's order queue by walking the pooled list —
// the FSV read of the actual intrusive links, not a return value.
func dumpQueue(w *World, id EntityID) string {
	r := w.Orders.Row(id)
	out := fmt.Sprintf("current{kind=%d phase=%d tgt=%d pt=(%d,%d)} queue[",
		w.Orders.Kind[r], w.Orders.Phase[r], w.Orders.Target[r],
		w.Orders.Point[r].X, w.Orders.Point[r].Y)
	for e := w.Orders.QueueHead[r]; e != NoOrderEntry; e = w.orderPool[e].next {
		out += fmt.Sprintf(" #%d{kind=%d pt=(%d,%d)}", e,
			w.orderPool[e].kind, w.orderPool[e].point.X, w.orderPool[e].point.Y)
	}
	return out + " ]"
}

type orderEv struct {
	kind uint16
	arg  int64
	tick uint32
}

// traceOrderEvents subscribes to the three order events for one unit.
func traceOrderEvents(w *World, id EntityID, base HandlerID) *[]orderEv {
	evs := &[]orderEv{}
	w.RegisterHandler(base, func(ww *World, e Event) {
		if e.Src == id {
			*evs = append(*evs, orderEv{e.Kind, e.Arg, ww.Tick()})
		}
	})
	w.Subscribe(EvOrderIssued, base)
	w.Subscribe(EvOrderDone, base)
	w.Subscribe(EvOrderDropped, base)
	return evs
}

// Happy path + edge 4: a 3-order shift-queue chain executes in FIFO
// order, each completion pops the next, and the emptied queue falls
// through to the default stop order. Bit-identical across two runs.
func TestOrderQueueChainAndDefault(t *testing.T) {
	run := func() ([]orderEv, fixed.Vec2, uint8, int32) {
		w := NewWorld(Caps{})
		w.SetGrid(openGrid())
		u := orderUnit(t, w, 10, 10, 16*fixed.One)
		w.OccupyCell(u)
		evs := traceOrderEvents(w, u, 1)
		free0 := w.OrderPoolFree()
		w.IssueOrder(u, Order{Kind: OrderMove, Point: CellCenter(cellIdx(14, 10))}, false)
		w.IssueOrder(u, Order{Kind: OrderMove, Point: CellCenter(cellIdx(18, 10))}, true)
		w.IssueOrder(u, Order{Kind: OrderMove, Point: CellCenter(cellIdx(18, 14))}, true)
		t.Logf("after issue: %s (pool free %d→%d)", dumpQueue(w, u), free0, w.OrderPoolFree())
		if w.QueueDepth(u) != 2 || w.OrderPoolFree() != free0-2 {
			t.Fatalf("queue depth %d (want 2), pool free %d (want %d)",
				w.QueueDepth(u), w.OrderPoolFree(), free0-2)
		}
		for tick := 0; tick < 200; tick++ {
			w.Step()
			r := w.Orders.Row(u)
			if w.Orders.Kind[r] == OrderStop && w.QueueDepth(u) == 0 &&
				w.Movements.State[w.Movements.Row(u)] == MoveIdle && len(*evs) >= 7 {
				break
			}
		}
		r := w.Orders.Row(u)
		pos := w.Transforms.Pos[w.Transforms.Row(u)]
		return *evs, pos, w.Orders.Kind[r], w.OrderPoolFree()
	}
	evs, pos, kind, free := run()
	for _, e := range evs {
		t.Logf("tick %3d: ev kind=%d arg=%d", e.tick, e.kind, e.arg)
	}
	t.Logf("final: pos=(%d,%d) currentOrder=%d poolFree=%d", pos.X, pos.Y, kind, free)
	// exact transition sequence: Issued(move) ×3 with Done(1) between,
	// then Issued(stop) — the default-order fall-through
	want := []struct {
		kind uint16
		arg  int64
	}{
		{EvOrderIssued, int64(OrderMove)},
		{EvOrderDone, 1},
		{EvOrderIssued, int64(OrderMove)},
		{EvOrderDone, 1},
		{EvOrderIssued, int64(OrderMove)},
		{EvOrderDone, 1},
		{EvOrderIssued, int64(OrderStop)},
	}
	if len(evs) != len(want) {
		t.Fatalf("event count %d, want %d: %+v", len(evs), len(want), evs)
	}
	for i := range want {
		if evs[i].kind != want[i].kind || evs[i].arg != want[i].arg {
			t.Fatalf("event %d = {kind %d arg %d}, want {kind %d arg %d}",
				i, evs[i].kind, evs[i].arg, want[i].kind, want[i].arg)
		}
	}
	if pos != CellCenter(cellIdx(18, 14)) {
		t.Fatalf("unit must end at the LAST queued destination")
	}
	if kind != OrderStop {
		t.Fatalf("default order must be stop: %d", kind)
	}
	if int(free) != EngineCaps.OrderQueueEntries {
		t.Fatalf("pool leak: free %d, want %d", free, EngineCaps.OrderQueueEntries)
	}
	// determinism: identical event trace + position on a second run
	evs2, pos2, _, _ := run()
	for i := range evs {
		if evs[i] != evs2[i] {
			t.Fatalf("event trace diverged at %d: %+v vs %+v", i, evs[i], evs2[i])
		}
	}
	if pos != pos2 {
		t.Fatalf("final position diverged")
	}
}

// Edge 1: an unqueued order during a queued chain clears the queue,
// recycles the pool entries, and interrupts the current move.
func TestOrderQueueUnqueuedClears(t *testing.T) {
	w := NewWorld(Caps{})
	w.SetGrid(openGrid())
	u := orderUnit(t, w, 10, 10, 8*fixed.One)
	w.OccupyCell(u)
	free0 := w.OrderPoolFree()
	w.IssueOrder(u, Order{Kind: OrderMove, Point: CellCenter(cellIdx(100, 10))}, false)
	for i := int32(0); i < 5; i++ {
		w.IssueOrder(u, Order{Kind: OrderMove, Point: CellCenter(cellIdx(100, 12+i))}, true)
	}
	for tick := 0; tick < 5; tick++ {
		w.Step() // get the first move running
	}
	mr := w.Movements.Row(u)
	t.Logf("before unqueued issue: %s, pool free=%d (5 queued), moveState=%d",
		dumpQueue(w, u), w.OrderPoolFree(), w.Movements.State[mr])
	if w.QueueDepth(u) != 5 || w.OrderPoolFree() != free0-5 {
		t.Fatalf("setup: depth %d free %d", w.QueueDepth(u), w.OrderPoolFree())
	}
	if w.Movements.State[mr] != MoveFollowing {
		t.Fatalf("setup: first move must be running")
	}

	w.IssueOrder(u, Order{Kind: OrderHold}, false)
	t.Logf("after unqueued hold: %s, pool free=%d, moveState=%d",
		dumpQueue(w, u), w.OrderPoolFree(), w.Movements.State[mr])
	if w.QueueDepth(u) != 0 {
		t.Fatalf("queue must clear: depth %d", w.QueueDepth(u))
	}
	if w.OrderPoolFree() != free0 {
		t.Fatalf("pool entries must recycle: free %d, want %d", w.OrderPoolFree(), free0)
	}
	if w.Movements.State[mr] != MoveIdle {
		t.Fatalf("interrupt edge must halt movement: state %d", w.Movements.State[mr])
	}
	r := w.Orders.Row(u)
	if w.Orders.Kind[r] != OrderHold {
		t.Fatalf("new order must install: kind %d", w.Orders.Kind[r])
	}
	// hold sticks: 20 more ticks, no drift, no events past the issue
	pos := w.Transforms.Pos[w.Transforms.Row(u)]
	for tick := 0; tick < 20; tick++ {
		w.Step()
	}
	if w.Transforms.Pos[w.Transforms.Row(u)] != pos {
		t.Fatalf("hold must not move")
	}
}

// Edge 2: the 17th queued order is deterministically dropped with
// EvOrderDropped; the queue stays at exactly MaxOrderQueue entries.
func TestOrderQueueOverflowDrop(t *testing.T) {
	w := NewWorld(Caps{})
	w.SetGrid(openGrid())
	u := orderUnit(t, w, 10, 10, 8*fixed.One)
	evs := traceOrderEvents(w, u, 1)
	w.IssueOrder(u, Order{Kind: OrderMove, Point: CellCenter(cellIdx(20, 10))}, false)
	for i := int32(0); i < MaxOrderQueue; i++ {
		if !w.IssueOrder(u, Order{Kind: OrderMove, Point: CellCenter(cellIdx(20, 11+i))}, true) {
			t.Fatalf("append %d within cap must succeed", i)
		}
	}
	t.Logf("at cap: depth=%d %s", w.QueueDepth(u), dumpQueue(w, u))
	if w.QueueDepth(u) != MaxOrderQueue {
		t.Fatalf("depth %d, want %d", w.QueueDepth(u), MaxOrderQueue)
	}
	if w.IssueOrder(u, Order{Kind: OrderAttack, Point: CellCenter(cellIdx(20, 40))}, true) {
		t.Fatalf("17th queued order must be dropped")
	}
	w.Step() // flush the drop event
	var drop *orderEv
	for i := range *evs {
		if (*evs)[i].kind == EvOrderDropped {
			drop = &(*evs)[i]
		}
	}
	if drop == nil || drop.arg != int64(OrderAttack) {
		t.Fatalf("EvOrderDropped with the dropped kind must fire: %+v", *evs)
	}
	t.Logf("drop event: kind=%d arg=%d (dropped order kind %d); depth still %d",
		drop.kind, drop.arg, OrderAttack, w.QueueDepth(u))
	if w.QueueDepth(u) != MaxOrderQueue {
		t.Fatalf("overflow must not mutate the queue: depth %d", w.QueueDepth(u))
	}
}

// Edge 2b: pool exhaustion (caps the pool at 1 entry) drops the
// second append even though the depth cap is far away — fail-closed.
func TestOrderQueuePoolExhaustedDrop(t *testing.T) {
	w := NewWorld(Caps{OrderQueueEntries: 1})
	w.SetGrid(openGrid())
	u := orderUnit(t, w, 10, 10, 8*fixed.One)
	evs := traceOrderEvents(w, u, 1)
	w.IssueOrder(u, Order{Kind: OrderMove, Point: CellCenter(cellIdx(20, 10))}, false)
	if !w.IssueOrder(u, Order{Kind: OrderMove, Point: CellCenter(cellIdx(20, 11))}, true) {
		t.Fatalf("first append must take the only pool entry")
	}
	if w.IssueOrder(u, Order{Kind: OrderMove, Point: CellCenter(cellIdx(20, 12))}, true) {
		t.Fatalf("pool-starved append must be dropped")
	}
	w.Step()
	found := false
	for _, e := range *evs {
		if e.kind == EvOrderDropped && e.arg == int64(OrderMove) {
			found = true
		}
	}
	t.Logf("pool free=%d (cap 1, 1 in use), drop event seen=%v", w.OrderPoolFree(), found)
	if !found || w.OrderPoolFree() != 0 {
		t.Fatalf("pool exhaustion must drop with event: free=%d found=%v", w.OrderPoolFree(), found)
	}
}

// Edge 3: an unreachable move stalls out (MoveBlocked) → the order
// FAILS, EvOrderDone(0) fires, and the next queued order pops and
// completes. Transition trace printed tick by tick.
func TestOrderQueueFailurePopsNext(t *testing.T) {
	w := NewWorld(Caps{})
	g := path.NewGrid() // corridor only — same fixture as the stall test
	for x := int32(4); x <= 10; x++ {
		g.SetFlags(x, 8, path.Walkable)
	}
	w.SetGrid(g)
	blocker := orderUnit(t, w, 7, 8, 0) // immobile, Following: not shovable
	w.OccupyCell(blocker)
	w.StartMoveTo(blocker, CellCenter(cellIdx(10, 8)))
	u := orderUnit(t, w, 5, 8, 8*fixed.One)
	w.OccupyCell(u)
	evs := traceOrderEvents(w, u, 7)
	w.IssueOrder(u, Order{Kind: OrderMove, Point: CellCenter(cellIdx(9, 8))}, false) // walled off
	w.IssueOrder(u, Order{Kind: OrderMove, Point: CellCenter(cellIdx(4, 8))}, true)  // reachable retreat
	for tick := 0; tick < 60; tick++ {
		w.Step()
	}
	for _, e := range *evs {
		t.Logf("tick %3d: ev kind=%d arg=%d", e.tick, e.kind, e.arg)
	}
	want := []struct {
		kind uint16
		arg  int64
	}{
		{EvOrderIssued, int64(OrderMove)}, // blocked order installs
		{EvOrderDone, 0},                  // FAILS at the stall threshold
		{EvOrderIssued, int64(OrderMove)}, // queued retreat pops
		{EvOrderDone, 1},                  // succeeds
		{EvOrderIssued, int64(OrderStop)}, // default fall-through
	}
	if len(*evs) != len(want) {
		t.Fatalf("event count %d, want %d: %+v", len(*evs), len(want), *evs)
	}
	for i := range want {
		if (*evs)[i].kind != want[i].kind || (*evs)[i].arg != want[i].arg {
			t.Fatalf("event %d = %+v, want %+v", i, (*evs)[i], want[i])
		}
	}
	pos := w.Transforms.Pos[w.Transforms.Row(u)]
	t.Logf("final: pos=(%d,%d) — at retreat cell center (%d,%d)", pos.X, pos.Y,
		CellCenter(cellIdx(4, 8)).X, CellCenter(cellIdx(4, 8)).Y)
	if pos != CellCenter(cellIdx(4, 8)) {
		t.Fatalf("retreat order must complete after the failure pop")
	}
}

// Zero allocations: 256 units with live order chains stepping.
func TestOrderQueueZeroAlloc(t *testing.T) {
	w := NewWorld(Caps{})
	w.SetGrid(openGrid())
	for u := 0; u < 256; u++ {
		x := int32(20 + (u%16)*3)
		y := int32(20 + (u/16)*3)
		id := orderUnit(t, w, x, y, fixed.One/2)
		w.OccupyCell(id)
		w.IssueOrder(id, Order{Kind: OrderMove, Point: CellCenter(cellIdx(x+100, y))}, false)
		w.IssueOrder(id, Order{Kind: OrderMove, Point: CellCenter(cellIdx(x+100, y+50))}, true)
		w.IssueOrder(id, Order{Kind: OrderMove, Point: CellCenter(cellIdx(x, y))}, true)
	}
	for i := 0; i < allocWarmupTicks; i++ {
		w.Step()
	}
	allocs := testing.AllocsPerRun(100, func() { w.Step() })
	t.Logf("AllocsPerRun(Step with 256 order-driven movers) = %v", allocs)
	if allocs != 0 {
		t.Fatalf("orders allocated: %v", allocs)
	}
}

func BenchmarkOrderIssue(b *testing.B) {
	w := NewWorld(Caps{})
	w.SetGrid(openGrid())
	u := orderUnit(b, w, 10, 10, 8*fixed.One)
	pt := CellCenter(cellIdx(100, 100))
	b.ReportAllocs()
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		w.IssueOrder(u, Order{Kind: OrderMove, Point: pt}, false)
		w.IssueOrder(u, Order{Kind: OrderMove, Point: pt}, true)
	}
}

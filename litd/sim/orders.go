package sim

// Order queue (combat-and-orders.md §2.1/§2.3, input.md §7): every
// unit holds a current order (the OrderStore head) plus a FIFO of
// shift-queued orders — pooled intrusive list entries from the
// 16,000-entry orderPool (ecs §2, R-GC-2).
//
//   issue (unqueued) — clears the queue (entries recycled), interrupts
//     the current order through its interrupt edge, installs the new
//     order as current.
//   issue (queued)   — appends; depth capped at MaxOrderQueue; an
//     overflowing or pool-starved append is DROPPED deterministically
//     with EvOrderDropped (input.md §7) — never silently.
//   completion       — done or failed pops the next entry; an empty
//     queue drops the unit to its default order (stop, the
//     auto-acquire stance of §3.1).
//
// Resolution runs in tick phase 3 over dense order rows. Order
// transitions raise EvOrderIssued/EvOrderDone through the event ring,
// dispatched in the phase-6 flush in deterministic order.

import (
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim/path"
)

// Order-transition events. EvOrderIssued fires when an order BECOMES
// CURRENT (install/pop/default fall-through), not on queue append —
// the append is invisible until it starts executing.
const (
	EvOrderIssued  uint16 = 4 // Src = unit, Dst = order target, Arg = order kind
	EvOrderDone    uint16 = 5 // Src = unit, Arg = 1 done / 0 failed
	EvOrderDropped uint16 = 6 // Src = unit, Arg = kind of the dropped order
)

// MaxOrderQueue is the per-unit shift-queue depth cap (input.md §7).
const MaxOrderQueue = 16

// Order phase values (OrderStore.Phase).
const (
	orderFresh   uint8 = 0 // installed, has not yet started driving its system
	orderRunning uint8 = 1
)

// Order is the value-struct verb (combat-and-orders.md §2.1): order
// kind plus the target variant — none (Target 0, Point zero), point,
// or entity.
type Order struct {
	Kind   uint8
	Target EntityID
	Point  fixed.Vec2
	Data   uint16 // cast: ability ref (defIndex+1)
}

// IssueOrder is the order entry point — Unit.Order/Unit.OrderQueued
// collapse here (PRD §4.3, D3). Returns false when the unit has no
// order head, is dead, or a queued append was dropped (cap/pool).
func (w *World) IssueOrder(id EntityID, o Order, queued bool) bool {
	r := w.Orders.Row(id)
	if r == -1 || !w.Ents.Alive(id) {
		return false
	}
	return w.issueOrderRow(r, id, o, queued)
}

// issueOrderRow is the row-level issue path, shared with the command
// stream (command.go applies validated records as unqueued issues).
func (w *World) issueOrderRow(r int32, id EntityID, o Order, queued bool) bool {
	s := w.Orders
	if !queued {
		w.clearOrderQueue(r)
		w.interruptCurrentOrder(id)
		w.installOrder(r, id, o.Kind, o.Target, o.Point, o.Data)
		return true
	}
	depth := int32(0)
	tail := NoOrderEntry
	for e := s.QueueHead[r]; e != NoOrderEntry; e = w.orderPool[e].next {
		tail = e
		depth++
	}
	if depth >= MaxOrderQueue {
		w.Emit(Event{Kind: EvOrderDropped, Src: id, Arg: int64(o.Kind)})
		return false
	}
	e := w.allocOrderEntry()
	if e == NoOrderEntry { // pool exhausted: deterministic drop, never a hang
		w.Emit(Event{Kind: EvOrderDropped, Src: id, Arg: int64(o.Kind)})
		return false
	}
	w.orderPool[e] = orderEntry{next: NoOrderEntry, kind: o.Kind, target: o.Target, point: o.Point, data: o.Data}
	if tail == NoOrderEntry {
		s.QueueHead[r] = e
	} else {
		w.orderPool[tail].next = e
	}
	return true
}

// QueueDepth returns the number of queued (not current) orders.
func (w *World) QueueDepth(id EntityID) int {
	r := w.Orders.Row(id)
	if r == -1 {
		return 0
	}
	n := 0
	for e := w.Orders.QueueHead[r]; e != NoOrderEntry; e = w.orderPool[e].next {
		n++
	}
	return n
}

// OrderPoolFree returns the free order-queue pool entries — the FSV
// leak probe.
func (w *World) OrderPoolFree() int32 { return w.orderFreeCount }

// installOrder makes an order current and raises EvOrderIssued.
func (w *World) installOrder(r int32, id EntityID, kind uint8, target EntityID, pt fixed.Vec2, data uint16) {
	s := w.Orders
	s.Kind[r] = kind
	s.Phase[r] = orderFresh
	s.Target[r] = target
	s.Point[r] = pt
	s.Data[r] = data
	w.Emit(Event{Kind: EvOrderIssued, Src: id, Dst: target, Arg: int64(kind)})
}

// interruptCurrentOrder is the §5 interrupt edge available today:
// halt movement, release the followed path. Attack-cycle interrupt
// (windup cancel) joins with #150.
func (w *World) interruptCurrentOrder(id EntityID) {
	mr := w.Movements.Row(id)
	if mr == -1 {
		return
	}
	m := w.Movements
	if m.PathHandle[mr] != NoPath {
		pid := path.PathID(m.PathHandle[mr])
		if w.Paths.Valid(pid) {
			w.Paths.Release(pid)
		}
		m.PathHandle[mr] = NoPath
	}
	m.State[mr] = MoveIdle
	m.Stall[mr] = 0
}

// completeOrder pops the next queued order or falls through to the
// default order (stop). done=false marks failure (unreachable, …).
func (w *World) completeOrder(r int32, id EntityID, done bool) {
	arg := int64(0)
	if done {
		arg = 1
	}
	w.Emit(Event{Kind: EvOrderDone, Src: id, Arg: arg})
	s := w.Orders
	if h := s.QueueHead[r]; h != NoOrderEntry {
		e := w.orderPool[h]
		s.QueueHead[r] = e.next
		w.freeOrderEntry(h)
		w.installOrder(r, id, e.kind, e.target, e.point, e.data)
		return
	}
	w.installOrder(r, id, OrderStop, 0, fixed.Vec2{}, 0) // default order fall-through
}

// clearOrderQueue recycles every queued entry of a row.
func (w *World) clearOrderQueue(r int32) {
	s := w.Orders
	for e := s.QueueHead[r]; e != NoOrderEntry; {
		next := w.orderPool[e].next
		w.freeOrderEntry(e)
		e = next
	}
	s.QueueHead[r] = NoOrderEntry
}

func (w *World) allocOrderEntry() int32 {
	h := w.orderFreeHead
	if h == NoOrderEntry {
		return NoOrderEntry
	}
	w.orderFreeHead = w.orderPool[h].next
	w.orderFreeCount--
	return h
}

func (w *World) freeOrderEntry(e int32) {
	w.orderPool[e].next = w.orderFreeHead
	w.orderFreeHead = e
	w.orderFreeCount++
}

// ordersSystem is tick phase 3: drive the current order of every
// order row in dense-row order; pop/fall through on completion.
func (w *World) ordersSystem() {
	s := w.Orders
	for r := int32(0); r < s.count; r++ {
		id := s.Entity[r]
		switch s.Kind[r] {
		case OrderStop, OrderHold:
			// terminal stances — nothing to drive (acquisition joins
			// with #148/#150)
		case OrderMove:
			mr := w.Movements.Row(id)
			if mr == -1 {
				// no Movement component: nothing can drive this order;
				// hold it (issue-time validation is #146's smart table)
				continue
			}
			if s.Phase[r] == orderFresh {
				if !w.StartMoveTo(id, s.Point[r]) {
					w.completeOrder(r, id, false)
					continue
				}
				s.Phase[r] = orderRunning
				continue
			}
			switch w.Movements.State[mr] {
			case MoveIdle: // arrived (movement emitted EvMoveDone)
				w.completeOrder(r, id, true)
			case MoveBlocked: // stalled out: unreachable for now
				w.completeOrder(r, id, false)
			}
		case OrderHarvest:
			w.driveHarvest(r, id) // the #300 cycle state machine
		case OrderAttack:
			// the attack cycle (attack.go) drives the engagement; the
			// order completes when its target is gone
			if !w.Ents.Alive(s.Target[r]) {
				w.completeOrder(r, id, true)
			}
		default:
			// smart execution lands with #146; until then the order
			// holds as current (visible state, no fake done)
		}
	}
}

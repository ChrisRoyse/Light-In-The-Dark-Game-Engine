package sim

// Deterministic event dispatch (tick-and-scheduler.md §3.2, §4 phase
// 6; R-EXEC-2). Events are fixed-size value structs in the
// preallocated per-tick ring — no interface payloads, no allocation.
// Handlers run synchronously at flush, in emission order × per-kind
// registration order. A handler that needs to wait suspends a
// continuation onto the scheduler and dispatch continues immediately
// with the next handler (the JASS thread-per-firing semantics without
// the threads).

// Event is one fixed-size pending event. Kind is the event type;
// Src/Dst are the participating entities; Arg carries the value
// payload (damage amount, order code, …).
type Event struct {
	Kind uint16
	Src  EntityID
	Dst  EntityID
	Arg  int64
}

// EvUnitDeath is the built-in death event: phase 5 kills emit it in
// kill order before the phase-6 flush. Src = the dying entity.
const EvUnitDeath uint16 = 1

// HandlerID stably identifies a registered event handler — handler
// IDs go into the save format with the subscription tables (R-SIM-6),
// so they must be identical across runs and builds.
type HandlerID uint32

// EventHandler runs synchronously at dispatch. To do deferred work it
// suspends a continuation via w.Sched / w.AfterMS — it never blocks.
type EventHandler func(w *World, e Event)

// kindSubs is the ordered handler list of one event kind. Kept in a
// kind-sorted slice (not a map): flush iterates zero maps.
type kindSubs struct {
	kind uint16
	list []HandlerID // registration order — the dispatch order (R-EXEC-2)
}

// RegisterHandler binds id to fn at sim construction. Panics on
// duplicate or nil — same fail-closed contract as the scheduler's
// continuation registry.
func (w *World) RegisterHandler(id HandlerID, fn EventHandler) {
	if fn == nil {
		panic("sim: RegisterHandler with nil func")
	}
	if _, dup := w.handlers[id]; dup {
		panic("sim: duplicate HandlerID registration")
	}
	w.handlers[id] = fn
}

// Subscribe appends handler id to kind's dispatch list. Registration
// order is dispatch order and is deterministic because subscription
// happens in deterministic script/construction order. Panics on an
// unregistered HandlerID (fail-closed).
func (w *World) Subscribe(kind uint16, id HandlerID) {
	if _, ok := w.handlers[id]; !ok {
		panic("sim: Subscribe with unregistered HandlerID")
	}
	i, ok := w.subIdx(kind)
	if !ok {
		w.subs = append(w.subs, kindSubs{})
		copy(w.subs[i+1:], w.subs[i:])
		w.subs[i] = kindSubs{kind: kind}
	}
	w.subs[i].list = append(w.subs[i].list, id)
}

// IsSubscribed reports whether handler id is already in kind's dispatch list.
// The subscription table is restored state (it serializes and hashes, R-SIM-6),
// so this is the source of truth for an idempotent subscribe across a save/load
// — a caller can re-park on a kind without appending a duplicate handler (which
// would both double-fire and diverge the state hash).
func (w *World) IsSubscribed(kind uint16, id HandlerID) bool {
	i, ok := w.subIdx(kind)
	if !ok {
		return false
	}
	for _, h := range w.subs[i].list {
		if h == id {
			return true
		}
	}
	return false
}

func (w *World) subIdx(kind uint16) (int, bool) {
	lo, hi := 0, len(w.subs)
	for lo < hi {
		mid := (lo + hi) / 2
		if w.subs[mid].kind < kind {
			lo = mid + 1
		} else {
			hi = mid
		}
	}
	return lo, lo < len(w.subs) && w.subs[lo].kind == kind
}

// SubsSnapshot returns the subscription tables in canonical order
// (ascending kind, registration-ordered handler IDs) — the form that
// hashes and serializes (R-SIM-6).
func (w *World) SubsSnapshot() []struct {
	Kind     uint16
	Handlers []HandlerID
} {
	out := make([]struct {
		Kind     uint16
		Handlers []HandlerID
	}, len(w.subs))
	for i := range w.subs {
		out[i].Kind = w.subs[i].kind
		out[i].Handlers = append([]HandlerID{}, w.subs[i].list...)
	}
	return out
}

// Emit queues an event for this tick's phase-6 flush. The ring never
// grows: the 4,097th event of a tick is DROPPED deterministically
// (every run drops the same event), the drop counter increments, and
// the debug assert hook fires (ecs §2).
func (w *World) Emit(e Event) bool {
	if w.eventCount == len(w.events) {
		w.eventsDropped++
		if w.OnEventDrop != nil {
			w.OnEventDrop(w.tick, e)
		}
		return false
	}
	w.events[w.eventCount] = e
	w.eventCount++
	return true
}

// EventsDropped returns the total events dropped to ring overflow.
func (w *World) EventsDropped() uint64 { return w.eventsDropped }

// flushEvents dispatches the ring in emission order; per event,
// handlers run in registration order. The loop re-reads eventCount so
// an event emitted BY a handler lands in this same flush, after
// everything already queued — deterministic cascade, no recursion.
func (w *World) flushEvents() {
	w.ensureTriggerIndex() // rebuild the ECA index once per flush if dirty (#458)
	for i := 0; i < w.eventCount; i++ {
		e := w.events[i]
		if w.eventLog != nil {
			w.logEvent(e) // R-FSV-3 structured log, dispatch order (#203)
		}
		// legacy OnEvent subscriptions (kept until #462 folds them into the
		// trigger substrate as sugar).
		if j, ok := w.subIdx(e.Kind); ok {
			list := w.subs[j].list
			for k := 0; k < len(list); k++ {
				w.handlers[list[k]](w, e)
			}
		}
		// first-class ECA trigger dispatch (#459).
		w.dispatchTriggers(e)
	}
	w.eventCount = 0
}

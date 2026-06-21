package sim

// Trigger dispatch — the ECA core (ADR #451, issue #459). When an event
// is flushed (events.go), for each matching enabled trigger in fire
// order (#458): evaluate its condition tree (#457); if it passes, run its
// actions (#455 handler refs). Conditions gate actions — a false
// condition skips them, the WC3 ECA contract.
//
// Actions run on the cooperative scheduler (D-28). An action may suspend
// the trigger "thread" by calling TriggerSleep: the action-runner parks a
// continuation that resumes the *remaining* actions after the delay. The
// continuation is a stably-identified scheduler record whose value-typed
// State packs (triggerID, next-action ordinal, the event) — never a Go
// closure over loop state — so a suspended trigger round-trips through
// save/load like any other sleeper (#464 extends this to the Lua side).

import "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim/sched"

// contTriggerActions is the sim-reserved ContID for the trigger
// action-runner. It sits above the script range (< 1<<30) and the api's
// timer/thread reservations (1<<30 + small) so the three never collide on
// the shared scheduler.
const contTriggerActions sched.ContID = 1 << 31

// registerTriggerDispatch binds the action-runner continuation on the
// world's scheduler. Called once at construction (and again on a fresh
// world before LoadState), so a resumed trigger relinks to the same
// stable ContID over the live world.
func (w *World) registerTriggerDispatch() {
	w.Sched.Register(contTriggerActions, func(s *sched.Scheduler, st sched.State) {
		t, e, ord := unpackTriggerState(st)
		w.runTriggerActions(t, e, ord)
	})
}

// packTriggerState encodes (trigger, next-action ordinal, event) into the
// scheduler's value-typed State. ordinal < maxTriggerActions (<2^16) and
// Kind is uint16, so they share one word; Src/Dst share another.
func packTriggerState(t TriggerID, ordinal int, e Event) sched.State {
	return sched.State{
		int64(uint64(t)),
		int64(uint64(ordinal)<<16 | uint64(e.Kind)),
		int64(uint64(uint32(e.Src))<<32 | uint64(uint32(e.Dst))),
		e.Arg,
	}
}

func unpackTriggerState(st sched.State) (TriggerID, Event, int) {
	t := TriggerID(uint64(st[0]))
	ordinal := int(uint64(st[1]) >> 16)
	kind := uint16(uint64(st[1]))
	src := EntityID(uint64(st[2]) >> 32)
	dst := EntityID(uint64(st[2]))
	e := Event{Kind: kind, Src: src, Dst: dst, Arg: st[3]}
	return t, e, ordinal
}

// eventScopeKey is the scope key an event is matched against in the
// trigger index (#458): the event's source entity. A trigger scoped to a
// specific unit (Scope = that unit's EntityID) fires only when that unit
// is the source; a globally-scoped trigger (Scope 0) fires for any.
func eventScopeKey(e Event) uint32 { return uint32(e.Src) }

// dispatchTriggers fires every trigger registered on e's kind+scope, in
// fire order, honoring enabled state and the condition gate. Called once
// per flushed event (events.go); the index is already ensured for the
// flush.
func (w *World) dispatchTriggers(e Event) {
	list := w.trigIndex.triggersFor(e.Kind, eventScopeKey(e))
	if len(list) == 0 {
		return
	}
	// Copy out of the index's reused scratch: an action may emit a cascade
	// event, and though that event is processed later in this same flush
	// (not re-entrantly), copying keeps this loop independent of any future
	// triggersFor call.
	w.dispatchBuf = append(w.dispatchBuf[:0], list...)
	for _, t := range w.dispatchBuf {
		sl := w.Triggers.slot(t)
		if sl == nil || !sl.enabled {
			continue // destroyed mid-flush or disabled → no eval, no actions
		}
		if !w.EvalExpr(sl.cond, e) {
			continue // condition false → actions skipped (ECA contract)
		}
		w.runTriggerActions(t, e, 0)
	}
}

// runTriggerActions runs a trigger's actions from ordinal `start`. If an
// action calls TriggerSleep, the runner parks a continuation to resume
// the remaining actions after the delay and returns. Unregistered action
// refs are skipped (fail-closed), never run the wrong code.
func (w *World) runTriggerActions(t TriggerID, e Event, start int) {
	sl := w.Triggers.slot(t)
	if sl == nil {
		return // trigger destroyed before/at resume
	}
	// Copy the slice header up front: an action that creates a trigger may
	// grow the slab's slots array and invalidate sl, but the actions
	// backing array is independent and stays valid.
	actions := sl.actions
	for i := start; i < len(actions); i++ {
		fn, ok := w.ResolveHandlerRef(actions[i])
		if ok {
			fn(w, e)
		}
		if w.trigSleepReq != 0 {
			delay := w.trigSleepReq
			w.trigSleepReq = 0
			w.Sched.After(delay, contTriggerActions, packTriggerState(t, i+1, e))
			return
		}
	}
}

// TriggerSleep, called from inside a trigger action, suspends the trigger
// thread: the remaining actions resume after `ticks` ticks (minimum 1 —
// a record never wakes on the tick that created it). The WC3
// TriggerSleepAction primitive.
func (w *World) TriggerSleep(ticks uint32) {
	if ticks == 0 {
		ticks = 1
	}
	w.trigSleepReq = ticks
}

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

// contPeriodicTrigger fires a periodic-timer trigger and re-arms it (#464).
// It sits one above the action-runner in the sim-reserved ContID range. Its
// State packs (triggerID, period) — value-typed, no Go closure — so a parked
// periodic round-trips through save/load and resumes on reload exactly like a
// suspended action, once registerTriggerDispatch re-binds this ContID over the
// restored world. This is what lets Game_Every survive a mid-game save (#450):
// the periodic schedule is data, not a captured closure.
const contPeriodicTrigger sched.ContID = 1<<31 + 1

// EvPeriodic is the synthetic event kind a periodic-timer trigger fires on.
// It is never Emit()ed onto the ring — the periodic continuation builds it
// directly to hand the trigger's condition/actions a well-formed (entity-free)
// Event — so it needs no index bucket and cannot collide with a real event.
const EvPeriodic uint16 = 28

// registerTriggerDispatch binds the trigger continuations on the world's
// scheduler. Called once at construction (and again on a fresh world before
// LoadState), so a resumed action or periodic relinks to the same stable
// ContID over the live world.
func (w *World) registerTriggerDispatch() {
	w.Sched.Register(contTriggerActions, func(s *sched.Scheduler, st sched.State) {
		t, e, ord := unpackTriggerState(st)
		w.runTriggerActions(t, e, ord)
	})
	w.Sched.Register(contPeriodicTrigger, func(s *sched.Scheduler, st sched.State) {
		t, period := unpackPeriodicState(st)
		sl := w.Triggers.slot(t)
		if sl == nil {
			return // trigger destroyed → the periodic stops (no re-arm)
		}
		e := Event{Kind: EvPeriodic}
		if !sl.enabled {
			w.observeTrigger(t, e, TrigSkipDisabled)
		} else if w.EvalExpr(sl.cond, e) {
			w.observeTrigger(t, e, TrigFired)
			w.runTriggerActions(t, e, 0)
		} else {
			w.observeTrigger(t, e, TrigSkipCondition)
		}
		// Re-arm while the trigger is alive: a disabled periodic keeps its
		// schedule and resumes firing when re-enabled (WC3 paused-timer
		// semantics); only Destroy stops it.
		w.Sched.After(period, contPeriodicTrigger, packPeriodicState(t, period))
	})
}

// packPeriodicState / unpackPeriodicState encode a periodic schedule into the
// scheduler's value-typed State: the trigger id and the period in ticks.
func packPeriodicState(t TriggerID, period uint32) sched.State {
	return sched.State{int64(uint64(t)), int64(uint64(period)), 0, 0}
}

func unpackPeriodicState(st sched.State) (TriggerID, uint32) {
	return TriggerID(uint64(st[0])), uint32(uint64(st[1]))
}

// ArmPeriodic schedules trigger t to fire every `period` ticks (minimum 1),
// first firing at now+period (a timer does not fire at arm time — the WC3
// periodic-timer-event semantics). The schedule is a value-typed scheduler
// continuation keyed by a stable ContID, so it serializes and resumes across
// save/load with no Go-closure capture. The fired Event is the entity-free
// EvPeriodic; the trigger's actions run when its condition passes.
func (w *World) ArmPeriodic(t TriggerID, period uint32) {
	if period == 0 {
		period = 1
	}
	w.Sched.After(period, contPeriodicTrigger, packPeriodicState(t, period))
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

// TrigOutcome is the result the dispatcher recorded for one trigger on one
// event — the ECA-layer observability vocabulary (R-FSV-3).
type TrigOutcome uint8

const (
	TrigFired           TrigOutcome = iota // condition passed; actions ran (or parked)
	TrigSkipStale                          // handle destroyed before dispatch reached it
	TrigSkipDisabled                       // trigger disabled
	TrigSkipCondition                      // condition tree returned false
)

// String renders an outcome for logs.
func (o TrigOutcome) String() string {
	switch o {
	case TrigFired:
		return "fired"
	case TrigSkipStale:
		return "skip-stale"
	case TrigSkipDisabled:
		return "skip-disabled"
	case TrigSkipCondition:
		return "skip-condition"
	}
	return "unknown"
}

// TriggerDispatch is one observability record handed to OnTriggerDispatch:
// the trigger, the event that reached it, and the outcome. Value type, no
// allocation — the observer copies what it needs.
type TriggerDispatch struct {
	Trigger TriggerID
	Event   Event
	Outcome TrigOutcome
}

// observeTrigger forwards one dispatch outcome to the hook if installed.
// Inlined-cheap: a nil check on the steady-state path.
func (w *World) observeTrigger(t TriggerID, e Event, o TrigOutcome) {
	if w.OnTriggerDispatch != nil {
		w.OnTriggerDispatch(TriggerDispatch{Trigger: t, Event: e, Outcome: o})
	}
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
		if sl == nil {
			w.observeTrigger(t, e, TrigSkipStale)
			continue // destroyed mid-flush → no eval, no actions
		}
		if !sl.enabled {
			w.observeTrigger(t, e, TrigSkipDisabled)
			continue // disabled → no eval, no actions
		}
		if !w.EvalExpr(sl.cond, e) {
			w.observeTrigger(t, e, TrigSkipCondition)
			continue // condition false → actions skipped (ECA contract)
		}
		w.observeTrigger(t, e, TrigFired)
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

// ExecuteTrigger runs a trigger's actions directly (WC3 TriggerExecute):
// run-from-another-trigger, bypassing its events, its condition, and its
// enabled flag. Actions run on the scheduler exactly as in a normal fire
// — they may TriggerSleep and round-trip. No-op on a stale handle.
func (w *World) ExecuteTrigger(t TriggerID, e Event) {
	if w.Triggers.slot(t) == nil {
		return
	}
	w.runTriggerActions(t, e, 0)
}

// EvaluateTrigger runs only a trigger's condition tree and returns the
// result (WC3 TriggerEvaluate): no actions, no side effects (condition
// leaves are pure). NoExpr / stale handle → true (vacuous), matching the
// dispatch gate.
func (w *World) EvaluateTrigger(t TriggerID, e Event) bool {
	sl := w.Triggers.slot(t)
	if sl == nil {
		return false
	}
	return w.EvalExpr(sl.cond, e)
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

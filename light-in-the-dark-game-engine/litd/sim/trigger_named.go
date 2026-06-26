package sim

// Named-trigger registry (#478). An ability may bind its EFFECT-edge behavior to
// a trigger instead of a static effect-primitive list (ADR #452): the data row
// carries a TriggerName, and setup binds that name to a created TriggerID here.
// At the EFFECT edge the cast machine resolves the name and fires the trigger
// (honoring its enabled flag + condition gate) instead of running ExecuteEffects.
//
// Only NAME → TriggerID pairs live here; the trigger graph itself is the slab
// (#456, already hashed + saved). The bindings hash + serialize (zero when
// empty, golden-stable) so two players — and a save/load — agree on which
// trigger backs each name.

// maxNamedTriggers bounds the per-world name→trigger binding table.
const maxNamedTriggers = 1024

// BindTriggerName binds a name to a trigger so a data ability's TriggerName can
// resolve to it. Setup-only (not inside a tick, not after the first Step), so
// the binding set is fixed for the match. Fail-closed on an empty or duplicate
// name, a stale trigger handle, or a full table.
func (w *World) BindTriggerName(name string, t TriggerID) bool {
	if w.inStep || w.tick > 0 || name == "" ||
		len(w.trigNameKeys) >= cap(w.trigNameKeys) ||
		w.Triggers.slot(t) == nil ||
		w.triggerIDByName(name) != 0 {
		return false
	}
	w.trigNameKeys = append(w.trigNameKeys, name)
	w.trigNameIDs = append(w.trigNameIDs, t)
	return true
}

// TriggerByName resolves a bound name to its TriggerID. ok=false when the name
// was never bound (the cast then runs as a documented no-op).
func (w *World) TriggerByName(name string) (TriggerID, bool) {
	if i := w.triggerNameIndex(name); i >= 0 {
		return w.trigNameIDs[i], true
	}
	return 0, false
}

// NamedTriggerCount is the number of name→trigger bindings.
func (w *World) NamedTriggerCount() int { return len(w.trigNameKeys) }

// triggerIDByName returns the bound TriggerID for a name, or 0 if unbound (a
// real TriggerID is never 0 — New starts at a positive generation/index).
func (w *World) triggerIDByName(name string) TriggerID {
	if i := w.triggerNameIndex(name); i >= 0 {
		return w.trigNameIDs[i]
	}
	return 0
}

// triggerNameIndex is the linear name scan (small N, setup-bound; no map keeps
// registration order the single source of truth and avoids map iteration).
func (w *World) triggerNameIndex(name string) int {
	for i := range w.trigNameKeys {
		if w.trigNameKeys[i] == name {
			return i
		}
	}
	return -1
}

// FireBoundTrigger fires one trigger by id with event e, applying the same
// enabled flag + condition gate as a normal event dispatch (#455). Returns true
// only when the actions actually ran: a stale handle, a disabled trigger, or a
// false condition is a no-op. This is the EFFECT-edge invocation seam for an
// ability bound to a trigger (#478).
func (w *World) FireBoundTrigger(t TriggerID, e Event) bool {
	sl := w.Triggers.slot(t)
	if sl == nil {
		w.observeTrigger(t, e, TrigSkipStale)
		return false
	}
	if !sl.enabled {
		w.observeTrigger(t, e, TrigSkipDisabled)
		return false
	}
	if !w.EvalExpr(sl.cond, e) {
		w.observeTrigger(t, e, TrigSkipCondition)
		return false
	}
	w.observeTrigger(t, e, TrigFired)
	w.runTriggerActions(t, e, 0)
	return true
}

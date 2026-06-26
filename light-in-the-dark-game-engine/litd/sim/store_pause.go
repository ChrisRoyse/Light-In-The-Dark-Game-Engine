package sim

// The per-unit "paused" bit (#217) — WC3's PauseUnit(u, true) / IsUnitPaused.
// A paused unit still exists in the sim (life, position, owner all persist) but
// is frozen: it does not drive orders, move, acquire targets, or attack while
// paused. Unlike the hidden bit, pause HAS sim-system consumers — the orders,
// movement, acquisition and attack systems skip paused entities (step gating).
// The flag is deterministic, gameplay-visible state: scripts read it back via
// IsUnitPaused and it must persist across save/load. Backed by a sparse
// presenceSet (w.Pauses): presence == paused, so the common all-active case
// costs zero rows.

// IsUnitPaused reports whether the unit is currently frozen by PauseUnit.
// False for an invalid/absent unit (a missing unit is not "paused", it simply
// is not there).
func (w *World) IsUnitPaused(id EntityID) bool {
	return w.Pauses.Has(id)
}

// PauseUnit sets the unit's pause bit: PauseUnit(id, true) freezes (adds the
// row), PauseUnit(id, false) resumes (drops it). No-op on a dead unit. Returns
// false on a dead unit or a full store while pausing.
func (w *World) PauseUnit(id EntityID, pause bool) bool {
	if !w.Ents.Alive(id) {
		return false
	}
	if !pause {
		w.Pauses.Remove(id) // already-active Remove is a harmless false
		return true
	}
	return w.Pauses.set(id)
}

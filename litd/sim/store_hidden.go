package sim

// The per-unit "hidden" bit (#217) — WC3's ShowUnit(u, false) / IsUnitHidden.
// A hidden unit still exists in the sim (orders, life, position all persist);
// the flag only suppresses rendering and selection. It has no sim-system
// consumer today (render/enumeration gating lands with those subsystems), but
// scripts read it back via IsUnitHidden and it must persist across save/load,
// so it is deterministic state. Backed by a sparse presenceSet (w.Hiddens):
// presence == hidden, so the common all-visible case costs zero rows.

// IsUnitHidden reports whether the unit is currently suppressed from render and
// selection. False for an invalid/absent unit (the safe default — a missing
// unit is not "hidden", it simply is not there).
func (w *World) IsUnitHidden(id EntityID) bool {
	return w.Hiddens.Has(id)
}

// ShowUnit sets the unit's hidden bit: ShowUnit(id, true) reveals (drops the
// row), ShowUnit(id, false) hides (adds it). No-op on a dead unit. Returns
// false on a dead unit or a full store while hiding.
func (w *World) ShowUnit(id EntityID, show bool) bool {
	if !w.Ents.Alive(id) {
		return false
	}
	if show {
		w.Hiddens.Remove(id) // already-visible Remove is a harmless false
		return true
	}
	return w.Hiddens.set(id)
}

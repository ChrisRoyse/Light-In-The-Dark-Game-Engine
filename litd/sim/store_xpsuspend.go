package sim

// The per-hero "experience suspended" bit (#217) — WC3's SuspendHeroXP /
// IsSuspendedXP. While suspended a hero gains no experience (AddXP early-returns
// for it), but keeps its current level/XP. Backed by a sparse presenceSet
// (w.XPSuspends): presence == suspended, so the common case (no hero suspended)
// costs zero rows. Deterministic, save-persisted state consulted at the single
// XP chokepoint (AddXP).

// IsHeroXPSuspended reports whether the unit's experience gain is currently
// suspended. False for any non-suspended or non-hero unit.
func (w *World) IsHeroXPSuspended(id EntityID) bool {
	return w.XPSuspends.Has(id)
}

// SuspendHeroXP turns experience gain off (flag=true) or back on (flag=false)
// for a hero. No-op on a non-hero. Returns false on a non-hero or a full store
// while suspending.
func (w *World) SuspendHeroXP(id EntityID, flag bool) bool {
	if w.Heroes.Row(id) == -1 {
		return false // only heroes have experience to suspend
	}
	if flag {
		return w.XPSuspends.set(id)
	}
	w.XPSuspends.Remove(id) // already-active Remove is a harmless false
	return true
}

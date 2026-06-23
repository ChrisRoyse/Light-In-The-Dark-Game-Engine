package sim

// Life-regeneration system (#520 root cause): apply each living unit's life
// regeneration once per tick — Life = min(Life + BuffedRegen(regen), MaxLife).
//
// Before this, Healths.Regen was stored (fed by the unit `regen` data field and
// hero strength), hashed, and saved, but NO system ever consumed it: authored
// regen was dead data, a silent fail-open. The fix is this system PLUS the
// life-regen buff stat (#520) it reads through BuffedRegen, so buff/item/upgrade
// life-regen modifiers actually take effect.
//
// It runs after damage application (phaseCombat) so a unit dropped to <= 0 this
// tick is not healed back, and it skips any row that is not alive — it never
// revives a corpse. For a unit with base regen 0 and no life-regen modifier,
// BuffedRegen returns 0 (untouched-cache identity) and the row is skipped with no
// state change, which is why every regen-less determinism golden stays
// bit-identical.
func (w *World) regenSystem() {
	h := w.Healths
	n := h.Count()
	for r := int32(0); r < n; r++ {
		if h.DeathState[r] != DeathAlive {
			continue // dying / decaying: never regen a corpse
		}
		life := h.Life[r]
		max := h.MaxLife[r]
		if life <= 0 || life >= max {
			continue // dead, or already full — nothing to add
		}
		regen := w.BuffedRegen(h.Entity[r], h.Regen[r])
		if regen <= 0 {
			continue
		}
		life = life.Add(regen)
		if life > max {
			life = max
		}
		h.Life[r] = life
	}
}

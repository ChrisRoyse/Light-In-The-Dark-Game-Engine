package litd

// Render-side presentation-cue stream (#313, #449). The sim stages one-shot cues
// (unit death, …) on the NON-HASHING render-event channel; this exposes them to
// render consumers as value types (R-API-6 — no sim/G3N types leak), drained from
// the current published snapshot. Read-only: draining never mutates the sim and
// never affects the state hash, so a consumer (audio sound trigger, death anim)
// cannot perturb determinism.

import (
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
)

// RenderEventKind tags a one-shot presentation cue.
type RenderEventKind uint8

const (
	// RenderUnitDied — a unit died this tick. UnitType and Pos are valid.
	RenderUnitDied RenderEventKind = iota + 1
	// RenderUnitReady — a unit finished training this tick. UnitType valid.
	RenderUnitReady
)

// RenderEvent is a value-type presentation cue for render-side consumers.
type RenderEvent struct {
	Kind     RenderEventKind
	UnitType UnitType // the involved unit's type (zero if not applicable)
	UnitKey  uint32   // stable per-unit key (entity index) for throttling
	Pos      Vec2     // world position (valid when HasPos)
	HasPos   bool
}

// RenderEvents appends this tick's published presentation cues to buf and returns
// it (reuses buf's capacity; pass nil for a fresh slice). Call after Advance. It is
// read-only against the published snapshot — it does not touch the sim state or the
// hash. Nil-receiver safe.
func (g *Game) RenderEvents(buf []RenderEvent) []RenderEvent {
	buf = buf[:0]
	if g == nil || g.w == nil {
		return buf
	}
	snap := g.w.Snaps.Curr()
	for i := range snap.Events {
		e := snap.Events[i]
		var kind RenderEventKind
		switch e.Kind {
		case sim.RenderUnitDeath:
			kind = RenderUnitDied
		case sim.RenderUnitReady:
			kind = RenderUnitReady
		default:
			continue
		}
		re := RenderEvent{
			Kind:     kind,
			UnitType: UnitType{ref: e.Data + 1}, // Data = unit-type id
			UnitKey:  e.Ent.Index(),
		}
		// Position from the same snapshot — the unit appears this tick (a dying
		// unit is published in phase 7 before its removal; a trained unit is live).
		for j := range snap.Entries {
			if snap.Entries[j].ID == e.Ent {
				p := snap.Entries[j].Pos
				re.Pos = Vec2{X: toFloat(p.X), Y: toFloat(p.Y)}
				re.HasPos = true
				break
			}
		}
		buf = append(buf, re)
	}
	return buf
}

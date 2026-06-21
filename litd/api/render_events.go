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
	// RenderUnitAttack — a unit fired an attack this tick. UnitType and Pos valid.
	RenderUnitAttack
	// RenderSpellCue — a script emitted a one-shot spell VFX cue at a unit this
	// tick (#479, via Game.EmitSpellCue). UnitType and Pos valid.
	RenderSpellCue
	// RenderUnitOrderAck — a unit received an explicit order this tick (#313).
	// UnitType + Owner valid; render filters to the local player's units.
	RenderUnitOrderAck
)

// RenderEvent is a value-type presentation cue for render-side consumers.
type RenderEvent struct {
	Kind     RenderEventKind
	UnitType UnitType // the involved unit's type (zero if not applicable)
	UnitKey  uint32   // stable per-unit key (entity index) for throttling
	Owner    int      // owning player of the unit, or -1 if none — for local-player presentation filters
	Pos      Vec2     // world position (valid when HasPos)
	HasPos   bool
}

// EmitSpellCue stages a one-shot spell VFX cue at a unit on the NON-HASHING
// render-event channel (#449/#479) — a trigger Action calls this so render plays
// an impact/cast effect without perturbing the state hash (an audio/vfx-on game
// hashes identically to one without). Returns false on an invalid handle. The
// cue carries the unit's type and position for the render consumer.
func (g *Game) EmitSpellCue(u Unit) bool {
	if g == nil || g.w == nil || !u.Valid() {
		return false
	}
	return g.w.EmitUnitRenderCue(sim.RenderSpellCue, u.id)
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
		case sim.RenderUnitAttack:
			kind = RenderUnitAttack
		case sim.RenderSpellCue:
			kind = RenderSpellCue
		case sim.RenderUnitOrderAck:
			kind = RenderUnitOrderAck
		default:
			continue
		}
		re := RenderEvent{
			Kind:     kind,
			UnitType: UnitType{ref: e.Data + 1}, // Data = unit-type id
			UnitKey:  e.Ent.Index(),
			Owner:    -1,
		}
		if or := g.w.Owners.Row(e.Ent); or >= 0 {
			re.Owner = int(g.w.Owners.Player[or])
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

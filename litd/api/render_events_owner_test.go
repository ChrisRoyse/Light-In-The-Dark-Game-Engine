package litd

// Regression for #666: a render-event must carry the owning player captured at
// EMIT time, not via a live Owners.Row lookup at drain time. A death cue is
// drained AFTER the dying unit is removed, so a live lookup returns -1 — the
// owner is gone. Any consumer that filters death cues by player (a defeat sound
// for your own units, the match-flow UnitsLost counter #665) then sees Owner=-1
// and silently drops the cue. SoT = the drained RenderUnitDied event's Owner.

import (
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
)

func TestRenderEventsDeathOwnerFSV(t *testing.T) {
	const ownerSlot uint8 = 2
	g := lvGame(t, ownerSlot) // barracks owned by player 2

	var barracks sim.EntityID
	for _, id := range g.w.AppendAllUnits(nil) {
		barracks = id
	}
	if barracks == 0 {
		t.Fatal("no barracks to kill")
	}

	// Trigger: kill the owned unit. SoT BEFORE: a live owner lookup still resolves
	// (unit alive); AFTER it is removed it would resolve to -1 — which is exactly
	// why the cue must snapshot the owner at emit.
	if !g.w.KillUnit(barracks) {
		t.Fatal("KillUnit failed")
	}

	var death *RenderEvent
	var buf []RenderEvent
	for i := 0; i < 5 && death == nil; i++ {
		g.Advance(1)
		buf = g.RenderEvents(buf)
		for j := range buf {
			if buf[j].Kind == RenderUnitDied {
				e := buf[j]
				death = &e
			}
		}
	}
	if death == nil {
		t.Fatal("no RenderUnitDied cue drained after KillUnit")
	}
	t.Logf("FSV #666: death cue Owner=%d (want %d), UnitKey=%d", death.Owner, ownerSlot, death.UnitKey)
	if death.Owner != int(ownerSlot) {
		t.Fatalf("death cue Owner = %d, want %d — owner not captured at emit (a since-removed unit's live lookup returned -1, the #666 bug)", death.Owner, ownerSlot)
	}
}

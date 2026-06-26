package render

// FSV for #506: selection acknowledgement is a HUD/input-layer sound, not a sim
// render event. SoT = audio.Manager.Dump() after driving the real input resolver
// and SelectionAckDriver with synthetic selectable rows.

import (
	"testing"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/audio"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/input"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
)

func TestSelectionAckOwnGroupAndCancelPreviousFSV(t *testing.T) {
	trig, mgr := buildTrigger(t, 2, 0)
	drv := NewSelectionAckDriver(trig, 0, map[uint16]string{10: "u0", 11: "u1"})
	items := []input.Selectable{
		selectionAckItem(1, 10, input.SelectUnit, 0, 100, 100),
		selectionAckItem(2, 10, input.SelectUnit, 0, 130, 100),
		selectionAckItem(3, 11, input.SelectUnit, 0, 180, 100),
		selectionAckItem(4, 10, input.SelectUnit, 1, 230, 100),
	}

	r := input.NewResolver(input.DefaultConfig(0))
	group := r.Drag(items, input.Rect{MinX: 80, MinY: 80, MaxX: 150, MaxY: 120}, input.Modifiers{})
	if out := drv.FireSelection(group, items, 10); out != CueRouted {
		t.Fatalf("own group selection ack = %v, want routed", out)
	}
	before := mgr.Dump()
	if len(before.Voices) != 1 || before.Voices[0].Cue != api.CueID("u0_ack") {
		t.Fatalf("group ack voice wrong: %+v", before.Voices)
	}
	if before.Voices[0].Domain != audio.DomainUI || before.Voices[0].HasPos {
		t.Fatalf("group ack should be UI/flat: %+v", before.Voices[0])
	}

	reselect := r.Click(items, 180, 100, input.Modifiers{})
	if out := drv.FireSelection(reselect, items, 11); out != CueRouted {
		t.Fatalf("rapid re-selection ack = %v, want routed", out)
	}
	after := mgr.Dump()
	if len(after.Voices) != 1 {
		t.Fatalf("rapid re-selection stacked ack voices: %+v", after.Voices)
	}
	if after.Voices[0].Cue != api.CueID("u1_ack") {
		t.Fatalf("prior ack was not cancelled/replaced: %+v", after.Voices)
	}
	t.Logf("FSV #506 selection ack BEFORE group voiceCount=%d cue=u0_ack(%d) domain=%d hasPos=%v AFTER rapid reselect voiceCount=%d cue=u1_ack(%d) domain=%d hasPos=%v",
		len(before.Voices), api.CueID("u0_ack"), before.Voices[0].Domain, before.Voices[0].HasPos,
		len(after.Voices), api.CueID("u1_ack"), after.Voices[0].Domain, after.Voices[0].HasPos)
}

func TestSelectionAckFiltersEnemyNeutralAndBuildingsFSV(t *testing.T) {
	trig, mgr := buildTrigger(t, 1, 0)
	drv := NewSelectionAckDriver(trig, 0, map[uint16]string{10: "u0", 20: "u0"})
	items := []input.Selectable{
		selectionAckItem(1, 10, input.SelectUnit, 1, 100, 100),     // enemy
		selectionAckItem(2, 10, input.SelectUnit, 2, 150, 100),     // neutral/non-local
		selectionAckItem(3, 20, input.SelectBuilding, 0, 200, 100), // own building, not a unit ack
	}
	r := input.NewResolver(input.DefaultConfig(0))

	enemy := r.Click(items, 100, 100, input.Modifiers{})
	if out := drv.FireSelection(enemy, items, 20); out != CueFiltered {
		t.Fatalf("enemy selection ack = %v, want filtered", out)
	}
	neutral := r.Click(items, 150, 100, input.Modifiers{})
	if out := drv.FireSelection(neutral, items, 21); out != CueFiltered {
		t.Fatalf("neutral selection ack = %v, want filtered", out)
	}
	building := r.Click(items, 200, 100, input.Modifiers{})
	if out := drv.FireSelection(building, items, 22); out != CueFiltered {
		t.Fatalf("building selection ack = %v, want filtered", out)
	}
	if s := mgr.Dump(); s.VoiceCount != 0 {
		t.Fatalf("filtered selections emitted voices: %+v", s.Voices)
	}
	t.Logf("FSV #506 selection ack filters: enemy=%s neutral=%s building=%s AFTER voiceCount=%d",
		CueFiltered, CueFiltered, CueFiltered, mgr.Dump().VoiceCount)
}

func selectionAckItem(id, typ uint16, class input.SelectClass, owner uint8, cx, cy float32) input.Selectable {
	return input.Selectable{
		ID:          sim.EntityID(id),
		TypeID:      typ,
		Class:       class,
		OwnerPlayer: owner,
		Screen:      input.Rect{MinX: cx - 8, MinY: cy - 8, MaxX: cx + 8, MaxY: cy + 8},
	}
}

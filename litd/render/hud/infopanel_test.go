package hud

// #195 info panel FSV. SoT = the widget's computed model (mode, stat text, grid
// cells, queue, emitted cancel records, dirty flags). Synthetic known selection
// snapshots => known panel state. Headless half of the widget pattern; canvas
// draw is presentation on top.

import (
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
)

func infoLabels() InfoPanelStrings {
	return InfoPanelStrings{Life: "HP", Mana: "MP", Armor: "AR", Attack: "AT", Level: "Lv"}
}

func TestInfoPanelSingleStatsFSV(t *testing.T) {
	var text TextBuffer
	p := NewInfoPanel(&text, infoLabels(), 0)
	u := p.Update(InfoPanelState{
		SelectionVersion: 1,
		Mode:             InfoSingle,
		Stats: InfoUnitStats{
			Name: "knight", Level: 2,
			Life: 100, MaxLife: 120, Mana: 40, MaxMana: 60,
			Armor: 3, AttackMin: 12, AttackMax: 15,
		},
	})
	t.Logf("FSV single: %+v text=%q", u, text.String())
	if !u.Dirty || u.Mode != InfoSingle || !u.Visible {
		t.Fatalf("single mode wrong: %+v", u)
	}
	if text.String() != "HP 100/120  MP 40/60  AR 3  AT 12-15  Lv 2" {
		t.Fatalf("stat line wrong: %q", text.String())
	}

	// Same version => no repaint (steady selection is free).
	u = p.Update(InfoPanelState{SelectionVersion: 1, Mode: InfoSingle, Stats: InfoUnitStats{Life: 100, MaxLife: 120, Mana: 40, MaxMana: 60, Armor: 3, AttackMin: 12, AttackMax: 15, Level: 2}})
	if u.Dirty || u.Repaints != 0 {
		t.Fatalf("unchanged version should not repaint: %+v", u)
	}

	// Damage bumps the version => repaint with the new line.
	u = p.Update(InfoPanelState{SelectionVersion: 2, Mode: InfoSingle, Stats: InfoUnitStats{Life: 70, MaxLife: 120, Mana: 40, MaxMana: 60, Armor: 3, AttackMin: 12, AttackMax: 15, Level: 2}})
	t.Logf("FSV single damaged: text=%q", text.String())
	if !u.Dirty || text.String() != "HP 70/120  MP 40/60  AR 3  AT 12-15  Lv 2" {
		t.Fatalf("damaged stat line wrong: %+v %q", u, text.String())
	}
}

func TestInfoPanelMultiGridFSV(t *testing.T) {
	var text TextBuffer
	p := NewInfoPanel(&text, infoLabels(), 0)
	cells := []InfoSelCell{
		{ID: 11, Icon: "knight", Subgroup: 100},
		{ID: 12, Icon: "knight", Subgroup: 100},
		{ID: 20, Icon: "archer", Subgroup: 200},
	}
	u := p.Update(InfoPanelState{SelectionVersion: 5, Mode: InfoMulti, Cells: cells, ActiveSubgroup: 100})
	t.Logf("FSV multi: %+v cells=%+v", u, p.Cells())
	if u.Mode != InfoMulti || u.Cells != 3 || len(p.Cells()) != 3 {
		t.Fatalf("multi grid wrong: %+v cells=%d", u, len(p.Cells()))
	}

	// Click the archer cell => re-select that unit + its subgroup.
	click := p.ClickCell(2)
	t.Logf("FSV cell click: %+v", click)
	if !click.Accepted || click.ID != 20 || click.Subgroup != 200 {
		t.Fatalf("cell re-select wrong: %+v", click)
	}
	// Out-of-range click is a no-op.
	if got := p.ClickCell(9); got.Accepted {
		t.Fatalf("out-of-range cell click should be no-op: %+v", got)
	}
}

func TestInfoPanelGridCapFSV(t *testing.T) {
	var text TextBuffer
	p := NewInfoPanel(&text, infoLabels(), 0)
	big := make([]InfoSelCell, 30) // over the 12 cap
	for i := range big {
		big[i] = InfoSelCell{ID: sim.EntityID(i + 1), Subgroup: uint16(i)}
	}
	u := p.Update(InfoPanelState{SelectionVersion: 1, Mode: InfoMulti, Cells: big})
	t.Logf("FSV grid cap: fed=%d shown=%d", len(big), u.Cells)
	if u.Cells != InfoGridCap || len(p.Cells()) != InfoGridCap {
		t.Fatalf("grid not capped at %d: shown=%d", InfoGridCap, u.Cells)
	}
}

func TestInfoPanelBuildingQueueCancelFSV(t *testing.T) {
	var text TextBuffer
	p := NewInfoPanel(&text, infoLabels(), 1)
	building := sim.EntityID(77)
	queue := []InfoQueueSlot{
		{Slot: 0, Label: "footman", Progress: 45},
		{Slot: 1, Label: "footman", Progress: 0},
		{Slot: 2, Label: "rifle", Progress: 0},
	}
	u := p.Update(InfoPanelState{QueueVersion: 3, Mode: InfoBuilding, Building: building, Queue: queue})
	t.Logf("FSV building: %+v queue=%+v", u, p.Queue())
	if u.Mode != InfoBuilding || u.Queue != 3 {
		t.Fatalf("building queue wrong: %+v", u)
	}

	// Cancel slot 1 => OpCancel record, building in Units[0], slot in Data.
	rec, ok := p.CancelSlot(1)
	t.Logf("FSV cancel slot 1: ok=%v rec={op:%d units0:%d data:%d player:%d}", ok, rec.Opcode, rec.Units[0], rec.Data, rec.Player)
	if !ok || rec.Opcode != sim.OpCancel || rec.Units[0] != building || rec.Data != 1 || rec.UnitCount != 1 || rec.Player != 1 {
		t.Fatalf("cancel record wrong: ok=%v %+v", ok, rec)
	}
	if len(p.Records()) != 1 {
		t.Fatalf("expected 1 emitted record, got %d", len(p.Records()))
	}
	// Cancel in the wrong mode is rejected.
	p.Update(InfoPanelState{SelectionVersion: 9, Mode: InfoSingle})
	if _, ok := p.CancelSlot(0); ok {
		t.Fatalf("cancel should be rejected outside building mode")
	}
}

func TestInfoPanelModeSwitchAndEmptyFSV(t *testing.T) {
	var text TextBuffer
	p := NewInfoPanel(&text, infoLabels(), 0)

	u := p.Update(InfoPanelState{Mode: InfoEmpty})
	if u.Visible {
		t.Fatalf("empty panel should be hidden: %+v", u)
	}
	// Mode change dirties even at the same (zero) versions.
	u = p.Update(InfoPanelState{Mode: InfoSingle, Stats: InfoUnitStats{Life: 5, MaxLife: 5, AttackMin: 1, AttackMax: 1}})
	t.Logf("FSV mode switch empty->single: %+v text=%q", u, text.String())
	if !u.Dirty || !u.Visible {
		t.Fatalf("mode switch should dirty + show: %+v", u)
	}
}

func TestInfoPanelSteadyZeroAllocFSV(t *testing.T) {
	var text TextBuffer
	p := NewInfoPanel(&text, infoLabels(), 0)
	st := InfoPanelState{SelectionVersion: 1, Mode: InfoSingle, Stats: InfoUnitStats{Life: 100, MaxLife: 120, Mana: 40, MaxMana: 60, Armor: 3, AttackMin: 12, AttackMax: 15, Level: 2}}
	p.Update(st) // warm
	got := testing.AllocsPerRun(1000, func() { p.Update(st) })
	t.Logf("FSV infopanel steady Update allocs/op=%.2f", got)
	if got != 0 {
		t.Fatalf("steady InfoPanel.Update allocated %.2f, want 0", got)
	}
}

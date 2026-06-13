package hud

import (
	"os"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/asset/locale"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
)

func TestCommandCardLoadRealTableFSV(t *testing.T) {
	table := loadCommandCardTable(t)
	keys := table.LocaleKeys()
	t.Logf("FSV command-card table path=%s groups=%d localeKeys=%d hotkeys=%v", table.Path, len(table.Groups), len(keys), table.GridHotkeys)
	if len(table.Groups) != 2 {
		t.Fatalf("groups=%d want 2", len(table.Groups))
	}
	footman, ok := table.Group("footman")
	if !ok {
		t.Fatal("footman group missing")
	}
	move := footman.Slots[0]
	if move.ID != "move" || move.Opcode != sim.OpMove || !move.RequiresTarget || move.Icon == "" {
		t.Fatalf("move slot not data-driven as expected: %+v", move)
	}
	if table.GridHotkeys[0] != "Q" || table.GridHotkeys[11] != "V" {
		t.Fatalf("grid hotkeys not QWER/ASDF/ZXCV: %v", table.GridHotkeys)
	}
}

func TestCommandCardRefreshAndSubgroupFSV(t *testing.T) {
	card := newTestCommandCard(t, "en")
	unit := commandCardUnitState()
	stats := card.Refresh(unit)
	t.Logf("FSV unit command-card active=%s summary=%q stats=%+v slots=%+v", card.ActiveSubgroup, card.Summary.String(), stats, visibleSlots(card))
	if !stats.Visible || card.ActiveSubgroup != "footman" || card.Slots[0].Label != "Move" || card.Slots[4].Label != "Attack" {
		t.Fatalf("footman card not populated from data/locale: active=%s slots=%+v", card.ActiveSubgroup, visibleSlots(card))
	}

	mixed := commandCardMixedState()
	before := mixed.ActiveSubgroupIndex
	if !CycleCommandSubgroup(&mixed) || mixed.ActiveSubgroupIndex == before {
		t.Fatalf("Tab should cycle subgroup: before=%d after=%d", before, mixed.ActiveSubgroupIndex)
	}
	stats = card.Refresh(mixed)
	t.Logf("FSV mixed+Tab command-card active=%s summary=%q stats=%+v slots=%+v", card.ActiveSubgroup, card.Summary.String(), stats, visibleSlots(card))
	if card.ActiveSubgroup != "barracks" || card.Slots[0].Label != "Footman" || card.Slots[2].ID != "rally" {
		t.Fatalf("mixed Tab should show barracks slots, got active=%s slots=%+v", card.ActiveSubgroup, visibleSlots(card))
	}
}

func TestCommandCardEmitsInputRecordFSV(t *testing.T) {
	card := newTestCommandCard(t, "en")
	card.Refresh(commandCardUnitState())
	click := card.ClickSlot(0, false)
	if !click.Accepted || !click.PendingTarget {
		t.Fatalf("move click should arm targeting, got %+v", click)
	}
	pt := fixed.Vec2{X: fixed.FromInt(320), Y: fixed.FromInt(480)}
	rec, ok := card.ConfirmTarget(pt, 0, false)
	t.Logf("FSV command-card click Move→ground click=%+v record=%+v records=%+v", click, rec, card.Records())
	if !ok || rec.Version != sim.CommandVersion || rec.Opcode != sim.OpMove || rec.UnitCount != 2 || rec.Units[0] != 101 || rec.Units[1] != 102 || rec.Point != pt {
		t.Fatalf("move target should emit deterministic command record, got ok=%v rec=%+v", ok, rec)
	}
	if len(card.Records()) != 1 {
		t.Fatalf("record ring count=%d want 1", len(card.Records()))
	}
}

func TestCommandCardEdgesFSV(t *testing.T) {
	card := newTestCommandCard(t, "en")
	enemy := commandCardUnitState()
	enemy.OwnSelection = false
	stats := card.Refresh(enemy)
	t.Logf("FSV enemy selection command-card stats=%+v summary=%q slots=%+v", stats, card.Summary.String(), visibleSlots(card))
	if stats.Visible || card.Summary.String() != "" {
		t.Fatalf("enemy selection should have no issuable orders")
	}

	empty := commandCardUnitState()
	empty.UnitCount = 0
	stats = card.Refresh(empty)
	t.Logf("FSV empty selection command-card stats=%+v summary=%q slots=%+v", stats, card.Summary.String(), visibleSlots(card))
	if stats.Visible || card.Summary.String() != "" {
		t.Fatalf("empty selection should show no stale buttons")
	}

	disabled := commandCardBuildingState()
	disabled.Gold = 100
	disabled.Lumber = 0
	disabled.Cooldown[0] = 5
	stats = card.Refresh(disabled)
	before := len(card.Records())
	click := card.ClickSlot(1, false)
	t.Logf("FSV disabled command-card stats=%+v summary=%q trainFootman=%+v trainArcher=%+v click=%+v recordsBefore=%d recordsAfter=%d",
		stats, card.Summary.String(), card.Slots[0], card.Slots[1], click, before, len(card.Records()))
	if card.Slots[0].Enabled || card.Slots[0].DisabledReason != "cooldown" {
		t.Fatalf("cooldown slot should be disabled, got %+v", card.Slots[0])
	}
	if card.Slots[1].Enabled || card.Slots[1].DisabledReason != "unaffordable" || click.Accepted || len(card.Records()) != before {
		t.Fatalf("unaffordable click should emit nothing, slot=%+v click=%+v before=%d after=%d", card.Slots[1], click, before, len(card.Records()))
	}
}

func TestCommandCardRefreshZeroAllocFSV(t *testing.T) {
	card := newTestCommandCard(t, "xx")
	state := commandCardUnitState()
	card.Refresh(state)
	allocs := testing.AllocsPerRun(1000, func() {
		state.Gold++
		_ = card.Refresh(state)
	})
	t.Logf("FSV localized command-card refresh allocs/op=%v active=%s summary=%q slots=%+v", allocs, card.ActiveSubgroup, card.Summary.String(), visibleSlots(card))
	if allocs != 0 {
		t.Fatalf("command-card dirty refresh allocated: %v", allocs)
	}
}

func newTestCommandCard(t *testing.T, tag string) CommandCard {
	t.Helper()
	table := loadCommandCardTable(t)
	strings, err := locale.Load(os.DirFS("../../../data"), tag)
	if err != nil {
		t.Fatal(err)
	}
	return NewCommandCard(table, strings)
}

func loadCommandCardTable(t *testing.T) *CommandCardTable {
	t.Helper()
	table, err := LoadCommandCardTable(os.DirFS("../../../data"))
	if err != nil {
		t.Fatal(err)
	}
	return table
}

func commandCardUnitState() CommandCardState {
	var s CommandCardState
	s.Player = 0
	s.OwnSelection = true
	s.SelectionLabel = "footman"
	s.Subgroups[0] = "footman"
	s.SubgroupCount = 1
	s.UnitCount = 2
	s.Units[0], s.Units[1] = 101, 102
	s.Gold, s.Lumber = 725, 240
	return s
}

func commandCardBuildingState() CommandCardState {
	var s CommandCardState
	s.Player = 0
	s.OwnSelection = true
	s.SelectionLabel = "barracks"
	s.Subgroups[0] = "barracks"
	s.SubgroupCount = 1
	s.UnitCount = 1
	s.Units[0] = 201
	s.Gold, s.Lumber = 725, 240
	return s
}

func commandCardMixedState() CommandCardState {
	s := commandCardUnitState()
	s.SelectionLabel = "mixed"
	s.Subgroups[1] = "barracks"
	s.SubgroupCount = 2
	s.UnitCount = 3
	s.Units[2] = 201
	return s
}

func visibleSlots(card CommandCard) []CommandCardSlotState {
	var out []CommandCardSlotState
	for _, slot := range card.Slots {
		if slot.Visible {
			out = append(out, slot)
		}
	}
	return out
}

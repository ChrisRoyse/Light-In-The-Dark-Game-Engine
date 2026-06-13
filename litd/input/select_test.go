package input

import (
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
)

func TestDragPriorityMixedFSV(t *testing.T) {
	items := []Selectable{
		selectable(1, 10, SelectUnit, 0, false, 110, 110),
		selectable(2, 10, SelectUnit, 0, false, 140, 110),
		selectable(3, 20, SelectBuilding, 0, false, 130, 130),
		selectable(4, 10, SelectUnit, 1, false, 120, 120),
		selectable(5, 20, SelectBuilding, 1, false, 160, 120),
	}
	r := NewResolver(DefaultConfig(0))
	res := r.Drag(items, Rect{MinX: 90, MinY: 90, MaxX: 180, MaxY: 160}, Modifiers{})
	t.Logf("FSV mixed drag selection ids=%v candidates=%d buildings=%d commands=%d",
		idsOf(res.Selection), res.Candidates, res.BuildingsConsidered, res.CommandRecordsEmitted)

	want := []sim.EntityID{id(2), id(1)}
	if !sameIDs(res.Selection, want) {
		t.Fatalf("mixed marquee selected %v, want own units %v", idsOf(res.Selection), want)
	}
	if res.CommandRecordsEmitted != 0 {
		t.Fatalf("selection emitted sim command records: %d", res.CommandRecordsEmitted)
	}
}

func TestDragSelectionCapClosestFSV(t *testing.T) {
	var items [20]Selectable
	for i := range items {
		items[i] = selectable(uint32(i+1), 1, SelectUnit, 0, false, 500+float32(i*10), 500)
	}
	r := NewResolver(DefaultConfig(0))
	res := r.Drag(items[:], Rect{MinX: 450, MinY: 450, MaxX: 740, MaxY: 550}, Modifiers{})
	t.Logf("FSV cap drag selection ids=%v count=%d", idsOf(res.Selection), res.Count)

	if res.Count != DefaultSelectionCap {
		t.Fatalf("selection count %d, want %d", res.Count, DefaultSelectionCap)
	}
	want := []sim.EntityID{id(10), id(11), id(9), id(12), id(8), id(13), id(7), id(14), id(6), id(15), id(5), id(16)}
	for i := 0; i < DefaultSelectionCap; i++ {
		if res.IDs[i] != want[i] {
			t.Fatalf("selection[%d]=%d, want closest id %d; all=%v", i, res.IDs[i], want[i], idsOf(res.Selection))
		}
	}
}

func TestLowPriorityExclusionFSV(t *testing.T) {
	workersAndFootman := []Selectable{
		selectable(1, 1, SelectUnit, 0, true, 100, 100),
		selectable(2, 1, SelectUnit, 0, true, 120, 100),
		selectable(3, 2, SelectUnit, 0, false, 140, 100),
	}
	r := NewResolver(DefaultConfig(0))
	res := r.Drag(workersAndFootman, Rect{MinX: 80, MinY: 80, MaxX: 160, MaxY: 120}, Modifiers{})
	t.Logf("FSV low-priority mixed ids=%v normal=%d", idsOf(res.Selection), res.NormalPriority)
	if !sameIDs(res.Selection, []sim.EntityID{id(3)}) {
		t.Fatalf("low-priority workers should be excluded when normal unit present: %v", idsOf(res.Selection))
	}

	workersOnly := workersAndFootman[:2]
	r = NewResolver(DefaultConfig(0))
	res = r.Drag(workersOnly, Rect{MinX: 80, MinY: 80, MaxX: 140, MaxY: 120}, Modifiers{})
	t.Logf("FSV low-priority workers-only ids=%v normal=%d", idsOf(res.Selection), res.NormalPriority)
	if !sameIDs(res.Selection, []sim.EntityID{id(1), id(2)}) {
		t.Fatalf("workers-only marquee should select workers: %v", idsOf(res.Selection))
	}
}

func TestShiftClickToggleFSV(t *testing.T) {
	items := []Selectable{
		selectable(1, 1, SelectUnit, 0, false, 100, 100),
		selectable(2, 1, SelectUnit, 0, false, 130, 100),
		selectable(3, 1, SelectUnit, 0, false, 160, 100),
	}
	r := NewResolver(DefaultConfig(0))
	r.SetSelection([]sim.EntityID{id(1), id(2)}, items)

	removed := r.Click(items, 100, 100, Modifiers{Shift: true})
	added := r.Click(items, 160, 100, Modifiers{Shift: true})
	t.Logf("FSV shift toggle after-remove=%v after-add=%v", idsOf(removed.Selection), idsOf(added.Selection))

	if !sameIDs(removed.Selection, []sim.EntityID{id(2)}) {
		t.Fatalf("shift-click selected unit should remove it: %v", idsOf(removed.Selection))
	}
	if !sameIDs(added.Selection, []sim.EntityID{id(2), id(3)}) {
		t.Fatalf("shift-click unselected unit should add it: %v", idsOf(added.Selection))
	}
}

func TestEnemyClickViewOnlyFSV(t *testing.T) {
	items := []Selectable{
		selectable(1, 1, SelectUnit, 0, false, 100, 100),
		selectable(2, 1, SelectUnit, 1, false, 130, 100),
	}
	r := NewResolver(DefaultConfig(0))
	res := r.Click(items, 130, 100, Modifiers{})
	t.Logf("FSV enemy click ids=%v hit=%d commands=%d", idsOf(res.Selection), res.Hit, res.CommandRecordsEmitted)
	if !sameIDs(res.Selection, []sim.EntityID{id(2)}) {
		t.Fatalf("enemy click should select exactly enemy view-only: %v", idsOf(res.Selection))
	}
	if res.CommandRecordsEmitted != 0 {
		t.Fatalf("enemy selection emitted command records: %d", res.CommandRecordsEmitted)
	}
}

func TestTypeSelectAndTabFSV(t *testing.T) {
	items := []Selectable{
		selectable(1, 7, SelectUnit, 0, false, 100, 100),
		selectable(2, 8, SelectUnit, 0, false, 130, 100),
		selectable(3, 7, SelectUnit, 0, false, 160, 100),
		selectable(4, 7, SelectUnit, 1, false, 190, 100),
	}
	r := NewResolver(DefaultConfig(0))
	typeSel := r.Click(items, 100, 100, Modifiers{Double: true})
	t.Logf("FSV double-click type-select ids=%v activeType=%d", idsOf(typeSel.Selection), typeSel.ActiveSubgroupTypeID)
	if !sameIDs(typeSel.Selection, []sim.EntityID{id(1), id(3)}) {
		t.Fatalf("double-click should type-select own visible type: %v", idsOf(typeSel.Selection))
	}

	r.SetSelection([]sim.EntityID{id(1), id(2), id(3)}, items)
	tab := r.Tab(items)
	t.Logf("FSV tab subgroup ids=%v subgroup=%d type=%d", idsOf(tab.Selection), tab.ActiveSubgroup, tab.ActiveSubgroupTypeID)
	if tab.ActiveSubgroup != 1 || tab.ActiveSubgroupTypeID != 8 {
		t.Fatalf("tab did not cycle to next subgroup/type: %+v", tab.Selection)
	}
}

func TestSelectionDragZeroAllocFSV(t *testing.T) {
	var items [20]Selectable
	for i := range items {
		items[i] = selectable(uint32(i+1), 1, SelectUnit, 0, i%5 == 0, 500+float32(i*10), 500)
	}
	rect := Rect{MinX: 450, MinY: 450, MaxX: 740, MaxY: 550}
	allocs := testing.AllocsPerRun(1000, func() {
		r := NewResolver(DefaultConfig(0))
		_ = r.Drag(items[:], rect, Modifiers{})
	})
	t.Logf("FSV drag-select allocs/op=%v", allocs)
	if allocs != 0 {
		t.Fatalf("drag-select allocated: %v", allocs)
	}
}

func selectable(idNum uint32, typ uint16, class SelectClass, owner uint8, low bool, cx, cy float32) Selectable {
	return Selectable{
		ID:          id(idNum),
		TypeID:      typ,
		Class:       class,
		OwnerPlayer: owner,
		LowPriority: low,
		Screen:      Rect{MinX: cx - 8, MinY: cy - 8, MaxX: cx + 8, MaxY: cy + 8},
	}
}

func id(n uint32) sim.EntityID { return sim.EntityID(n) }

func idsOf(s Selection) []sim.EntityID {
	return s.IDs[:s.Count]
}

func sameIDs(s Selection, want []sim.EntityID) bool {
	if int(s.Count) != len(want) {
		return false
	}
	for i := range want {
		if s.IDs[i] != want[i] {
			return false
		}
	}
	return true
}

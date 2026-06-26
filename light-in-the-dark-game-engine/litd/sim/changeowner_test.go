package sim

import "testing"

// TestChangeOwnerContract pins the primitive's branch contract (#362): a unit
// with no Econ row still migrates owner cleanly; dead units and units without an
// owner row are rejected (return false, no state change). SoT = the Owners store.
func TestChangeOwnerContract(t *testing.T) {
	w := NewWorld(Caps{Units: 8})

	// A live unit owned by player 1, no Econ row (no food charge).
	id, ok := w.CreateUnit(CellCenter(cellIdx(4, 4)), 0)
	if !ok || !w.Owners.Add(w.Ents, id, 1, 1, 1) {
		t.Fatal("unit setup failed")
	}
	or := w.Owners.Row(id)

	// No Econ row → owner/team/color migrate, ledger untouched, returns true.
	if !w.ChangeOwner(id, 3, 3, true) {
		t.Fatal("ChangeOwner returned false for a live owned unit")
	}
	if w.Owners.Player[or] != 3 || w.Owners.Team[or] != 3 || w.Owners.Color[or] != 3 {
		t.Errorf("owner row = (%d,%d,%d), want (3,3,3)", w.Owners.Player[or], w.Owners.Team[or], w.Owners.Color[or])
	}
	if w.FoodUsed(1) != 0 || w.FoodUsed(3) != 0 {
		t.Errorf("no-Econ unit moved food: used[1]=%d used[3]=%d, want 0,0", w.FoodUsed(1), w.FoodUsed(3))
	}

	// changeColor=false keeps the color while moving the owner.
	if !w.ChangeOwner(id, 4, 4, false) || w.Owners.Player[or] != 4 || w.Owners.Color[or] != 3 {
		t.Errorf("changeColor=false: player=%d color=%d, want player=4 color=3 kept", w.Owners.Player[or], w.Owners.Color[or])
	}

	// A unit with no owner row → false.
	bare, ok := w.CreateUnit(CellCenter(cellIdx(5, 5)), 0)
	if !ok {
		t.Fatal("bare unit spawn failed")
	}
	if w.ChangeOwner(bare, 2, 2, true) {
		t.Error("ChangeOwner returned true for a unit with no owner row")
	}

	// A dead unit → false, no panic.
	w.DestroyUnit(id)
	if w.ChangeOwner(id, 2, 2, true) {
		t.Error("ChangeOwner returned true for a dead unit")
	}
}

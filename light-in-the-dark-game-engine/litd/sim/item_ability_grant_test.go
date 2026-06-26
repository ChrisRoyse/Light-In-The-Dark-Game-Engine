package sim

// #621 — data-authored item ability grants. SoT = the unit's ability slot list
// after a data-driven pickup/drop, and the loader's fail-closed rejection of an
// item that grants an undefined ability. (Unit-type abilities=[...] auto-grant
// at spawn already lands via SpawnFromTable; this covers the item data field.)

import (
	"testing"
	"testing/fstest"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
)

const grantAbilityTOML = `
[[ability]]
id = "shield_bash"
name = "Shield Bash"
mana-cost = 0
cooldown = 1.0
cast-range = 100
`

const grantItemTOML = `
[[item]]
id = "warshield"
name = "War Shield"
class = "permanent"
grants-abilities = ["shield_bash"]
`

func loadGrantTables(t *testing.T, items string) (*data.Tables, error) {
	t.Helper()
	return data.Load(fstest.MapFS{
		"combat/damage-table.toml": &fstest.MapFile{Data: []byte("attack-types = [\"magic\"]\narmor-types = [\"none\"]\n[coefficients]\nmagic = [1000]\n")},
		"abilities/core.toml":      &fstest.MapFile{Data: []byte(grantAbilityTOML)},
		"items/core.toml":          &fstest.MapFile{Data: []byte(items)},
	})
}

// TestItemGrantsAbilityDataDriven: an item authored with grants-abilities grants
// the ability on pickup and revokes it on drop — no host RegisterItemAbilityGrant
// call, purely from the data file.
func TestItemGrantsAbilityDataDriven(t *testing.T) {
	tb, err := loadGrantTables(t, grantItemTOML)
	if err != nil {
		t.Fatalf("load: %v", err)
	}
	// SoT at the data layer: the grant resolved to an ability index.
	var warshield int = -1
	for i := range tb.Items {
		if tb.Items[i].ID == "warshield" {
			warshield = i
		}
	}
	if warshield < 0 || len(tb.Items[warshield].GrantsAbilities) != 1 {
		t.Fatalf("warshield grants-abilities not resolved: %+v", tb.Items)
	}
	t.Logf("data: warshield.GrantsAbilities=%v (ability index)", tb.Items[warshield].GrantsAbilities)

	w := NewWorld(Caps{Units: 16})
	if !w.BindAbilityDefs(tb.Abilities) {
		t.Fatal("BindAbilityDefs failed")
	}
	if !w.BindItemDefs(tb.Items) {
		t.Fatal("BindItemDefs failed")
	}
	unit := atkUnit(t, w, 1, fixed.Vec2{X: 1000 * fixed.One, Y: 1000 * fixed.One}, 0)
	if !w.AddInventory(unit) {
		t.Fatal("AddInventory failed")
	}
	item, ok := w.SpawnItem(uint16(warshield), fixed.Vec2{X: 1000 * fixed.One, Y: 1000 * fixed.One})
	if !ok {
		t.Fatal("SpawnItem failed")
	}

	t.Logf("BEFORE pickup: hasAbility=%v", w.UnitHasAbility(unit, "shield_bash"))
	if w.UnitHasAbility(unit, "shield_bash") {
		t.Fatal("unit has the item ability before pickup")
	}
	if rc := w.AddItemToInventory(unit, item); rc != ItemOK {
		t.Fatalf("pickup rc=%d", rc)
	}
	t.Logf("AFTER pickup: hasAbility=%v", w.UnitHasAbility(unit, "shield_bash"))
	if !w.UnitHasAbility(unit, "shield_bash") {
		t.Fatal("data-authored item did not grant its ability on pickup")
	}

	ir := w.Invents.Row(unit)
	slot := -1
	for s := 0; s < InventorySlots; s++ {
		if w.Invents.Slots[ir][s] == item {
			slot = s
		}
	}
	if rc := w.DropItem(unit, slot); rc != ItemOK {
		t.Fatalf("drop rc=%d", rc)
	}
	t.Logf("AFTER drop: hasAbility=%v", w.UnitHasAbility(unit, "shield_bash"))
	if w.UnitHasAbility(unit, "shield_bash") {
		t.Fatal("item ability not revoked on drop")
	}
}

// TestUnitTypeAbilitiesGrantedOnSpawn: a unit type with abilities=[...] grants
// them when spawned from the table (SpawnFromTable). SoT = the spawned unit's
// ability slot list.
func TestUnitTypeAbilitiesGrantedOnSpawn(t *testing.T) {
	w := NewWorld(Caps{Units: 8})
	if !w.BindAbilityDefs([]data.Ability{{ID: "zap", Name: "Zap", CastRange: 100 * fixed.One}}) {
		t.Fatal("BindAbilityDefs failed")
	}
	if !w.BindUnitDefs([]data.Unit{{ID: "stormcaller", Life: 100, Abilities: []uint16{0}}}) {
		t.Fatal("BindUnitDefs failed")
	}
	id, ok := w.SpawnFromTable(0, 1, 1, fixed.Vec2{X: 500 * fixed.One, Y: 500 * fixed.One})
	if !ok {
		t.Fatal("SpawnFromTable failed")
	}
	t.Logf("spawned stormcaller %d: hasAbility(zap)=%v", id, w.UnitHasAbility(id, "zap"))
	if !w.UnitHasAbility(id, "zap") {
		t.Fatal("unit type abilities=[zap] not granted at spawn")
	}
}

// TestItemGrantsUnknownAbilityRejected: an item granting an undefined ability is
// a fail-closed LOAD error naming the bad id.
func TestItemGrantsUnknownAbilityRejected(t *testing.T) {
	bad := "\n[[item]]\nid = \"warshield\"\nname = \"War Shield\"\nclass = \"permanent\"\ngrants-abilities = [\"no_such_ability\"]\n"
	_, err := loadGrantTables(t, bad)
	if err == nil {
		t.Fatal("load accepted an item granting an undefined ability")
	}
	t.Logf("load rejected: %v", err)
}

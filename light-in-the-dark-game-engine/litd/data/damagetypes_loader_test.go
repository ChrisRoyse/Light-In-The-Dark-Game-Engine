package data

// FSV for #472 at the data layer: a modder adds an attack type by editing the
// table TOML — no engine edit — and the loader sizes the coefficient matrix
// from the named tables. SoT = the loaded Tables.AttackTypes/ArmorTypes/Coeff;
// fail-closed when a declared type has no matrix row.

import (
	"strings"
	"testing"
	"testing/fstest"
)

func damageFS(t *testing.T, damageTable string) fstest.MapFS {
	t.Helper()
	return fstest.MapFS{
		"combat/damage-table.toml": &fstest.MapFile{Data: []byte(damageTable)},
		"abilities/core.toml":      &fstest.MapFile{Data: []byte(testAbilities)},
		"units/test.toml":          &fstest.MapFile{Data: []byte(testUnitTOML)},
	}
}

// TestLoaderNewAttackTypeExtensibleFSV — adding a 3rd attack type "holy" + its
// coefficient row loads cleanly; the matrix grows to 3×2 and "holy" resolves to
// index 2 (append order).
func TestLoaderNewAttackTypeExtensibleFSV(t *testing.T) {
	const dt = `
attack-types = ["normal", "piercing", "holy"]
armor-types = ["light", "heavy"]
[coefficients]
normal = [1000, 700]
piercing = [2000, 350]
holy = [1500, 450]
`
	tables, err := Load(damageFS(t, dt))
	if err != nil {
		t.Fatalf("load: %v", err)
	}
	t.Logf("FSV #472 loader: attackTypes=%v armorTypes=%v matrix=%dx%d",
		tables.AttackTypes, tables.ArmorTypes, len(tables.Coeff), len(tables.Coeff[0]))
	if len(tables.AttackTypes) != 3 || tables.AttackTypes[2] != "holy" {
		t.Fatalf("attackTypes = %v, want [normal piercing holy]", tables.AttackTypes)
	}
	if len(tables.Coeff) != 3 || len(tables.Coeff[0]) != 2 {
		t.Fatalf("matrix dims = %dx%d, want 3x2 (sized from tables)", len(tables.Coeff), len(tables.Coeff[0]))
	}
	if tables.Coeff[2][0] != 1500 || tables.Coeff[2][1] != 450 {
		t.Fatalf("holy row = %v, want [1500 450]", tables.Coeff[2])
	}
}

// TestLoaderNewTypeMissingRowFailsClosedFSV — declaring a type but omitting its
// coefficient row fails the load loudly (dims must match the type counts).
func TestLoaderNewTypeMissingRowFailsClosedFSV(t *testing.T) {
	const dt = `
attack-types = ["normal", "piercing", "holy"]
armor-types = ["light", "heavy"]
[coefficients]
normal = [1000, 700]
piercing = [2000, 350]
`
	_, err := Load(damageFS(t, dt))
	t.Logf("FSV #472 loader missing-row: err=%v", err)
	if err == nil {
		t.Fatal("a declared attack type with no coefficient row must fail the load")
	}
	if !strings.Contains(err.Error(), "holy") && !strings.Contains(err.Error(), "rows") {
		t.Fatalf("error must point at the missing/short type: %v", err)
	}
}

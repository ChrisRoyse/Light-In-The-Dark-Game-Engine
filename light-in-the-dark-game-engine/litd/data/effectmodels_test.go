package data

// #530 effect-model registry FSV. SoT = the converted Tables.EffectModels slice
// (and the Fingerprint it folds into). Known input rows -> known sorted output
// with known Key/Asset; row order must not matter (determinism, R-SIM-2);
// malformed rows fail closed; an absent table folds nothing (existing world
// fingerprints unchanged).

import (
	"testing"
	"testing/fstest"
)

func effModelsFS(models string) fstest.MapFS {
	fs := fstest.MapFS{
		"combat/damage-table.toml": &fstest.MapFile{Data: []byte(testDamageTable)},
		"abilities/core.toml":      &fstest.MapFile{Data: []byte(testAbilities)},
		"units/test.toml":          &fstest.MapFile{Data: []byte(testUnitTOML)},
	}
	if models != "" {
		fs["effects-models/models.toml"] = &fstest.MapFile{Data: []byte(models)}
	}
	return fs
}

func TestLoadEffectModelsFSV(t *testing.T) {
	// Declared out of key order on purpose — the loader must sort by Key so the
	// worldhost id assignment (1..N) is row-order independent.
	const models = `
[[model]]
key = "fx/spark"
asset = "fx/spark.glb"

[[model]]
key = "fx/glow"
asset = "fx/glow.glb"
`
	tables, err := Load(effModelsFS(models))
	if err != nil {
		t.Fatalf("Load: %v", err)
	}
	if len(tables.EffectModels) != 2 {
		t.Fatalf("EffectModels = %d, want 2", len(tables.EffectModels))
	}
	// SoT: sorted by Key — "fx/glow" precedes "fx/spark" regardless of file order.
	want := []EffectModel{
		{Key: "fx/glow", Asset: "fx/glow.glb"},
		{Key: "fx/spark", Asset: "fx/spark.glb"},
	}
	for i, w := range want {
		got := tables.EffectModels[i]
		t.Logf("FSV model[%d] = key=%q asset=%q (id %d at install)", i, got.Key, got.Asset, i+1)
		if got != w {
			t.Fatalf("EffectModels[%d] = %+v, want %+v", i, got, w)
		}
	}
}

func TestLoadEffectModelsRowOrderInvariantFSV(t *testing.T) {
	const a = `
[[model]]
key = "fx/a"
asset = "a.glb"
[[model]]
key = "fx/b"
asset = "b.glb"
`
	const b = `
[[model]]
key = "fx/b"
asset = "b.glb"
[[model]]
key = "fx/a"
asset = "a.glb"
`
	ta, err := Load(effModelsFS(a))
	if err != nil {
		t.Fatalf("Load a: %v", err)
	}
	tb, err := Load(effModelsFS(b))
	if err != nil {
		t.Fatalf("Load b: %v", err)
	}
	// Same content, swapped rows -> identical converted order AND identical
	// fingerprint. If either differed, id assignment / state hash would depend on
	// file row order, breaking determinism.
	if len(ta.EffectModels) != len(tb.EffectModels) {
		t.Fatalf("len mismatch %d vs %d", len(ta.EffectModels), len(tb.EffectModels))
	}
	for i := range ta.EffectModels {
		if ta.EffectModels[i] != tb.EffectModels[i] {
			t.Fatalf("row %d differs: %+v vs %+v", i, ta.EffectModels[i], tb.EffectModels[i])
		}
	}
	t.Logf("FSV row-order invariant: fp(a)=%#016x fp(b)=%#016x", ta.Fingerprint, tb.Fingerprint)
	if ta.Fingerprint != tb.Fingerprint {
		t.Fatalf("fingerprint depends on row order: %#x != %#x", ta.Fingerprint, tb.Fingerprint)
	}
}

func TestLoadEffectModelsRejectsFSV(t *testing.T) {
	cases := []struct {
		name, models string
	}{
		{"empty-key", "[[model]]\nkey = \"\"\nasset = \"a.glb\"\n"},
		{"empty-asset", "[[model]]\nkey = \"fx/a\"\nasset = \"\"\n"},
		{"duplicate-key", "[[model]]\nkey = \"fx/a\"\nasset = \"a.glb\"\n[[model]]\nkey = \"fx/a\"\nasset = \"b.glb\"\n"},
		{"no-rows", "# present but empty\n"},
	}
	for _, c := range cases {
		t.Run(c.name, func(t *testing.T) {
			_, err := Load(effModelsFS(c.models))
			if err == nil {
				t.Fatalf("%s: Load succeeded, want fail-closed error", c.name)
			}
			t.Logf("FSV reject %s -> %v", c.name, err)
		})
	}
}

func TestEffectModelsFingerprintInertWhenAbsentFSV(t *testing.T) {
	// A world WITHOUT an effects-models table folds nothing (hashEffectModels
	// returns early): two absent-table loads are stable, and adding a table
	// changes the fingerprint — proving the fold is present-only, so existing
	// worlds keep their exact prior hash.
	noTable1, err := Load(effModelsFS(""))
	if err != nil {
		t.Fatalf("Load (no table) #1: %v", err)
	}
	noTable2, err := Load(effModelsFS(""))
	if err != nil {
		t.Fatalf("Load (no table) #2: %v", err)
	}
	withTable, err := Load(effModelsFS("[[model]]\nkey = \"fx/a\"\nasset = \"a.glb\"\n"))
	if err != nil {
		t.Fatalf("Load (with table): %v", err)
	}
	t.Logf("FSV inert: absent=%#016x absent2=%#016x present=%#016x",
		noTable1.Fingerprint, noTable2.Fingerprint, withTable.Fingerprint)
	if noTable1.Fingerprint != noTable2.Fingerprint {
		t.Fatalf("absent-table fingerprint unstable: %#x != %#x", noTable1.Fingerprint, noTable2.Fingerprint)
	}
	if noTable1.Fingerprint == withTable.Fingerprint {
		t.Fatal("present table did not change the fingerprint — fold is a no-op")
	}
	if len(noTable1.EffectModels) != 0 {
		t.Fatalf("absent table yielded %d models, want 0", len(noTable1.EffectModels))
	}
}

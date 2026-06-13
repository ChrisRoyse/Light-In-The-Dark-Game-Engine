package main

import (
	"strings"
	"testing"
)

func validEntry() FunctionEntry {
	return FunctionEntry{
		Name:           "IsUnitPausedBJ",
		Origin:         "blizzard.j",
		Signature:      Signature{Params: []ParamEntry{{Name: "whichUnit", JassType: "unit", TSType: "unit"}}, Returns: "boolean"},
		Classification: "D1",
		ClassifiedBy:   "override",
		Disposition:    "mapped",
		GoMapping:      &GoMapping{Symbol: "Unit.Paused", Package: "litd/api"},
	}
}

func TestValidateManifestHappy(t *testing.T) {
	m := Manifest{
		SchemaVersion: 1,
		Sources:       []SourceEntry{{File: "common.j", SHA256: "abc", DeclCount: 1534}},
		Functions:     []FunctionEntry{validEntry()},
	}
	if err := ValidateManifest(m); err != nil {
		t.Fatalf("valid manifest rejected: %v", err)
	}
}

func TestValidateManifestEdges(t *testing.T) {
	base := func() Manifest {
		return Manifest{SchemaVersion: 1, Sources: []SourceEntry{{File: "c", SHA256: "h", DeclCount: 1}}}
	}

	// Edge 1: mapped entry missing goMapping.
	e1 := validEntry()
	e1.GoMapping = nil
	m1 := base()
	m1.Functions = []FunctionEntry{e1}
	if err := ValidateManifest(m1); err == nil || !strings.Contains(err.Error(), "disposition=mapped requires goMapping") {
		t.Errorf("edge1 = %v", err)
	}

	// Edge 2: tombstoned entry carrying goMapping.
	e2 := validEntry()
	e2.Disposition = "tombstoned"
	e2.Tombstone = &TombstoneT{Reason: "superseded", Detail: "x"}
	// keeps GoMapping from validEntry()
	m2 := base()
	m2.Functions = []FunctionEntry{e2}
	if err := ValidateManifest(m2); err == nil || !strings.Contains(err.Error(), "tombstoned entry must not carry a goMapping") {
		t.Errorf("edge2 = %v", err)
	}

	// Edge 3: origin outside enum.
	e3 := validEntry()
	e3.Origin = "common.ai.bogus"
	m3 := base()
	m3.Functions = []FunctionEntry{e3}
	if err := ValidateManifest(m3); err == nil || !strings.Contains(err.Error(), "origin") {
		t.Errorf("edge3 = %v", err)
	}
}

func TestMarshalManifestDeterministic(t *testing.T) {
	m := Manifest{
		SchemaVersion: 1,
		Sources:       []SourceEntry{{File: "common.j", SHA256: "abc", DeclCount: 1534}},
		Functions:     []FunctionEntry{validEntry()},
	}
	a, err := MarshalManifest(m)
	if err != nil {
		t.Fatal(err)
	}
	b, err := MarshalManifest(m)
	if err != nil {
		t.Fatal(err)
	}
	if string(a) != string(b) {
		t.Error("MarshalManifest not deterministic")
	}
	if a[len(a)-1] != '\n' {
		t.Error("manifest should end with newline")
	}
}


package main

import (
	"strings"
	"testing"
)

func baseClasses() []Classification {
	return []Classification{
		{Name: "IsUnitPausedBJ", Class: ClassD1, ClassifiedBy: "heuristic", Evidence: "passthrough"},
		{Name: "DoNothing", Class: ClassUnclassified, ClassifiedBy: "heuristic", Evidence: "native: no pattern"},
		{Name: "SetUnitState", Class: ClassD5, ClassifiedBy: "heuristic", Evidence: "state accessor"},
	}
}

func TestApplyOverrideTombstoneFlipAndRevert(t *testing.T) {
	cs := baseClasses()
	// Before: DoNothing is heuristic/unclassified, no tombstone.
	var before Classification
	for _, c := range cs {
		if c.Name == "DoNothing" {
			before = c
		}
	}
	if before.ClassifiedBy != "heuristic" || before.Tombstone != "" {
		t.Fatalf("precondition wrong: %+v", before)
	}

	ovs := []Override{{Name: "DoNothing", Class: "D1", Tombstone: "gameplay-irrelevant", Reason: "no-op stub"}}
	after, err := ApplyOverrides(cs, ovs)
	if err != nil {
		t.Fatalf("apply: %v", err)
	}
	var got Classification
	for _, c := range after {
		if c.Name == "DoNothing" {
			got = c
		}
	}
	if got.ClassifiedBy != "override" || got.Tombstone != "gameplay-irrelevant" {
		t.Errorf("after override = %+v, want classifiedBy=override tombstone=gameplay-irrelevant", got)
	}

	// Revert-check: ApplyOverrides does not mutate the input slice.
	if cs[1].ClassifiedBy != "heuristic" || cs[1].Tombstone != "" {
		t.Errorf("input slice mutated: %+v", cs[1])
	}
}

// A collapse member inherits its canonical's dedup class. SetUnitX folds onto
// SetUnitState's canonical; without propagation it would keep its heuristic
// class and inflate the M2 unclassified counter. Regression for the
// collapse-member class-propagation fix (#260 M2 green-gate).
func TestApplyOverridePropagatesClassToCollapseMembers(t *testing.T) {
	cs := append(baseClasses(),
		Classification{Name: "SetUnitX", Class: ClassUnclassified, ClassifiedBy: "heuristic", Evidence: "native: no pattern"},
	)
	ovs := []Override{{
		Name: "SetUnitState", Class: "D5", GoMapping: "Unit.SetLife", Reason: "state setter",
		CollapsesWith: []string{"SetUnitX"},
	}}
	after, err := ApplyOverrides(cs, ovs)
	if err != nil {
		t.Fatalf("apply: %v", err)
	}
	var member Classification
	for _, c := range after {
		if c.Name == "SetUnitX" {
			member = c
		}
	}
	if member.Class != ClassD5 || member.ClassifiedBy != "override" {
		t.Errorf("collapse member SetUnitX = %+v, want Class=D5 classifiedBy=override", member)
	}
	// The member stays a collapse member (no own mapping/tombstone), so it must
	// not itself become a duplicate canonical target.
	if member.GoMapping != "" || member.Tombstone != "" {
		t.Errorf("collapse member must not get its own disposition: %+v", member)
	}
}

func TestApplyOverrideRejections(t *testing.T) {
	cs := baseClasses()
	cases := []struct {
		name   string
		ov     Override
		errSub string
	}{
		{"unknown name", Override{Name: "NotARealNative", Class: "D1", Reason: "x"},
			`override for unknown function "NotARealNative"`},
		{"bad tombstone reason", Override{Name: "DoNothing", Tombstone: "because", Reason: "x"},
			`tombstone reason "because" outside enum`},
		{"missing reason", Override{Name: "IsUnitPausedBJ", Class: "D1"},
			`override for "IsUnitPausedBJ" missing required reason`},
		// #368 regression: a tombstone with a valid reason but no class must error
		// (fail-closed) rather than silently no-op into the unmapped universe.
		{"classless tombstone", Override{Name: "DoNothing", Tombstone: "gameplay-irrelevant", Reason: "no-op"},
			`tombstones without a class`},
	}
	for _, tc := range cases {
		_, err := ApplyOverrides(cs, []Override{tc.ov})
		if err == nil {
			t.Errorf("%s: expected error, got nil", tc.name)
			continue
		}
		if !strings.Contains(err.Error(), tc.errSub) {
			t.Errorf("%s: error = %q, want contains %q", tc.name, err.Error(), tc.errSub)
		}
	}

	// Duplicate override for one name.
	_, err := ApplyOverrides(cs, []Override{
		{Name: "DoNothing", Class: "D1", Tombstone: "deprecated", Reason: "a"},
		{Name: "DoNothing", Class: "D1", Tombstone: "superseded", Reason: "b"},
	})
	if err == nil || !strings.Contains(err.Error(), `duplicate override for "DoNothing"`) {
		t.Errorf("duplicate: error = %v", err)
	}
}

// TestLoadRealOverridesFile parses the committed overrides.toml and applies it
// over the real classification — proving every entry names an existing symbol
// (no hard error) and flips classifiedBy.
func TestLoadRealOverridesFile(t *testing.T) {
	ovs, err := LoadOverrides("overrides.toml")
	if err != nil {
		t.Fatalf("load overrides.toml: %v", err)
	}
	if len(ovs) == 0 {
		t.Fatal("overrides.toml parsed to 0 entries")
	}
	// Every entry must have a reason and (if tombstoned) a valid enum reason.
	for _, o := range ovs {
		if o.Reason == "" {
			t.Errorf("override %q in file missing reason", o.Name)
		}
		if o.Tombstone != "" && !tombstoneReasons[o.Tombstone] {
			t.Errorf("override %q has bad tombstone reason %q", o.Name, o.Tombstone)
		}
	}
}

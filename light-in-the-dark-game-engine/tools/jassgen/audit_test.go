package main

import (
	"strings"
	"testing"
)

func clean3() ([]Classification, Manifest) {
	cs := []Classification{
		{Name: "A", Origin: "common", Class: ClassD3, ClassifiedBy: "override", GoMapping: "X.A", Package: "litd/api"},
		{Name: "B", Origin: "blizzard", Class: ClassD2, ClassifiedBy: "override", Tombstone: "superseded", Evidence: "tombstone: gone"},
	}
	m := Manifest{
		SchemaVersion: 1,
		Functions: []FunctionEntry{
			{Name: "A", Origin: "common.j", Signature: Signature{Params: []ParamEntry{}, Returns: "nothing"}, Classification: "D3", ClassifiedBy: "override", Disposition: "mapped", GoMapping: &GoMapping{Symbol: "X.A", Package: "litd/api"}},
			{Name: "B", Origin: "blizzard.j", Signature: Signature{Params: []ParamEntry{}, Returns: "unit"}, Classification: "D2", ClassifiedBy: "override", Disposition: "tombstoned", Tombstone: &TombstoneT{Reason: "superseded", Detail: "gone"}},
		},
	}
	return cs, m
}

func TestAuditCleanGreen(t *testing.T) {
	cs, m := clean3()
	r := ComputeAudit(cs, m)
	if r.Total != 2 || r.Mapped != 1 || r.Tombstoned != 1 {
		t.Errorf("counts wrong: %+v", r)
	}
	if r.Unclassified != 0 || r.Unmapped != 0 {
		t.Errorf("expected 0 unclassified/unmapped, got %d/%d", r.Unclassified, r.Unmapped)
	}
	if len(r.Violations) != 0 {
		t.Errorf("expected green, got violations: %v", r.Violations)
	}
}

// Edge 1: an unclassified entry fires the gate with the counter printed.
func TestAuditEdgeUnclassifiedFires(t *testing.T) {
	cs, m := clean3()
	cs = append(cs, Classification{Name: "C", Origin: "common", Class: ClassUnclassified, ClassifiedBy: "heuristic"})
	r := ComputeAudit(cs, m)
	if r.Unclassified != 1 {
		t.Errorf("unclassified = %d, want 1", r.Unclassified)
	}
	found := false
	for _, v := range r.Violations {
		if strings.Contains(v, "unclassified=1") {
			found = true
		}
	}
	if !found {
		t.Errorf("expected unclassified violation, got %v", r.Violations)
	}
}

// Edge 2: an entry mapped AND tombstoned is caught by the manifest validator
// (uniqueness — mapped XOR tombstone).
func TestAuditEdgeDualDispositionRejected(t *testing.T) {
	dual := FunctionEntry{
		Name: "Dual", Origin: "common.j",
		Signature:      Signature{Params: []ParamEntry{}, Returns: "nothing"},
		Classification: "D1", ClassifiedBy: "override", Disposition: "mapped",
		GoMapping: &GoMapping{Symbol: "X.Dual", Package: "litd/api"},
		Tombstone: &TombstoneT{Reason: "superseded", Detail: "x"},
	}
	m := Manifest{SchemaVersion: 1, Sources: []SourceEntry{{File: "c", SHA256: "h", DeclCount: 1}}, Functions: []FunctionEntry{dual}}
	err := ValidateManifest(m)
	if err == nil || !strings.Contains(err.Error(), "mapped entry must not carry a tombstone") {
		t.Errorf("dual-disposition not rejected: %v", err)
	}
}

// TestAuditCreditsCollapseMembers is the regression for the structural defect
// where D3 collapse members counted as unmapped forever (the canonical entry's
// collapsesWith list them, but they have no mapped/tombstoned record of their
// own). Before the fix: Total=4, Mapped=1, Collapsed absent → Unmapped=3 and the
// M2 gate is RED for every collapsed family. After: the 3 members are credited
// as Collapsed, Unmapped=0, and they do NOT inflate duplicateTargets (only the
// one canonical symbol is a manifest GoMapping target).
func TestAuditCreditsCollapseMembers(t *testing.T) {
	// Source universe: canonical A plus its 3 collapse members, all real symbols.
	cs := []Classification{
		{Name: "A", Origin: "common", Class: ClassD3, ClassifiedBy: "override", GoMapping: "X.A", Package: "litd/api"},
		{Name: "M1", Origin: "common", Class: ClassD3, ClassifiedBy: "heuristic"},
		{Name: "M2", Origin: "common", Class: ClassD3, ClassifiedBy: "heuristic"},
		{Name: "M3", Origin: "blizzard", Class: ClassD3, ClassifiedBy: "heuristic"},
	}
	m := Manifest{
		SchemaVersion: 1,
		Functions: []FunctionEntry{
			{Name: "A", Origin: "common.j", Signature: Signature{Params: []ParamEntry{}, Returns: "nothing"}, Classification: "D3", ClassifiedBy: "override", Disposition: "mapped", GoMapping: &GoMapping{Symbol: "X.A", Package: "litd/api", CollapsesWith: []string{"M1", "M2", "M3"}}},
		},
	}
	r := ComputeAudit(cs, m)
	if r.Total != 4 {
		t.Fatalf("Total = %d, want 4", r.Total)
	}
	if r.Mapped != 1 {
		t.Errorf("Mapped = %d, want 1", r.Mapped)
	}
	if r.Collapsed != 3 {
		t.Errorf("Collapsed = %d, want 3 (M1,M2,M3)", r.Collapsed)
	}
	if r.Unmapped != 0 {
		t.Errorf("Unmapped = %d, want 0 (collapse members credited)", r.Unmapped)
	}
	if len(r.DuplicateTargets) != 0 {
		t.Errorf("DuplicateTargets = %v, want empty (collapse members are not separate targets)", r.DuplicateTargets)
	}
	if len(r.Violations) != 0 {
		t.Errorf("expected green, got violations: %v", r.Violations)
	}
}

// A collapsesWith member that is NOT a real source symbol must not be credited
// (fail-closed: phantom members can't paper over the gate).
func TestAuditDoesNotCreditPhantomCollapseMembers(t *testing.T) {
	cs := []Classification{
		{Name: "A", Origin: "common", Class: ClassD3, ClassifiedBy: "override", GoMapping: "X.A", Package: "litd/api"},
		{Name: "Real", Origin: "common", Class: ClassD3, ClassifiedBy: "heuristic"},
	}
	m := Manifest{
		SchemaVersion: 1,
		Functions: []FunctionEntry{
			{Name: "A", Origin: "common.j", Signature: Signature{Params: []ParamEntry{}, Returns: "nothing"}, Classification: "D3", ClassifiedBy: "override", Disposition: "mapped", GoMapping: &GoMapping{Symbol: "X.A", Package: "litd/api", CollapsesWith: []string{"Real", "Phantom"}}},
		},
	}
	r := ComputeAudit(cs, m)
	if r.Collapsed != 1 {
		t.Errorf("Collapsed = %d, want 1 (only Real; Phantom is not a source symbol)", r.Collapsed)
	}
	if r.Unmapped != 0 {
		t.Errorf("Unmapped = %d, want 0 (A mapped, Real collapsed)", r.Unmapped)
	}
}

// Edge 3 (duplicate canonical target): two mapped entries claiming one symbol.
func TestAuditEdgeDuplicateTarget(t *testing.T) {
	cs := []Classification{
		{Name: "A", Origin: "common", Class: ClassD3, ClassifiedBy: "override", GoMapping: "X.Same", Package: "litd/api"},
		{Name: "B", Origin: "common", Class: ClassD3, ClassifiedBy: "override", GoMapping: "X.Same", Package: "litd/api"},
	}
	m := Manifest{
		SchemaVersion: 1,
		Functions: []FunctionEntry{
			{Name: "A", Origin: "common.j", Signature: Signature{Params: []ParamEntry{}, Returns: "nothing"}, Classification: "D3", ClassifiedBy: "override", Disposition: "mapped", GoMapping: &GoMapping{Symbol: "X.Same", Package: "litd/api"}},
			{Name: "B", Origin: "common.j", Signature: Signature{Params: []ParamEntry{}, Returns: "nothing"}, Classification: "D3", ClassifiedBy: "override", Disposition: "mapped", GoMapping: &GoMapping{Symbol: "X.Same", Package: "litd/api"}},
		},
	}
	r := ComputeAudit(cs, m)
	if len(r.DuplicateTargets) != 1 {
		t.Errorf("duplicateTargets = %v, want 1", r.DuplicateTargets)
	}
	found := false
	for _, v := range r.Violations {
		if strings.Contains(v, "duplicateTargets") {
			found = true
		}
	}
	if !found {
		t.Errorf("expected duplicateTargets violation: %v", r.Violations)
	}
}

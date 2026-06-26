package main

import (
	"strings"
	"testing"
)

// TestBuildProvenanceAggregates is the X+X=Y check on synthetic manifest data:
// three JASS functions collapsing onto one Go symbol must aggregate (name +
// collapsesWith), de-dupe, and sort into a single sources list.
func TestBuildProvenanceAggregates(t *testing.T) {
	m := Manifest{Functions: []FunctionEntry{
		{Name: "SetUnitState", GoMapping: &GoMapping{
			Symbol: "Unit.SetLife", Package: "litd/api",
			CollapsesWith: []string{"SetUnitLifeBJ", "SetUnitLifePercentBJ"},
		}},
		// a second entry mapping to the SAME symbol must union in, not replace
		{Name: "SetWidgetLife", GoMapping: &GoMapping{
			Symbol: "Unit.SetLife", Package: "litd/api",
		}},
		// a different package must be excluded
		{Name: "CreateNUnitsAtLoc", GoMapping: &GoMapping{
			Symbol: "CreateUnits", Package: "litd/api/helpers",
		}},
		// a tombstoned/unmapped function (no goMapping) must be ignored
		{Name: "AbortCinematicFadeBJ"},
	}}
	prov := BuildProvenance(m, "litd/api")
	got := strings.Join(prov["Unit.SetLife"], ", ")
	want := "SetUnitLifeBJ, SetUnitLifePercentBJ, SetUnitState, SetWidgetLife"
	if got != want {
		t.Fatalf("Unit.SetLife sources = %q, want %q", got, want)
	}
	if _, ok := prov["CreateUnits"]; ok {
		t.Fatalf("helpers-package symbol leaked into litd/api provenance")
	}
	if len(prov) != 1 {
		t.Fatalf("expected exactly 1 litd/api symbol, got %d: %v", len(prov), prov)
	}
}

// TestApplyProvenanceAppendAndIdempotent proves the file rewriter (1) appends
// the JASS line as the doc comment's final line, touching nothing else, and
// (2) is idempotent — a second pass produces byte-identical output, and (3)
// rewrites a stale line in place rather than duplicating it.
func TestApplyProvenanceAppendAndIdempotent(t *testing.T) {
	const src = `package litd

// SetLife writes the unit's current life.
func (u Unit) SetLife(v float64) {}

// Untouched has no mapping and must keep no JASS line.
func (u Unit) Untouched() {}
`
	prov := map[string][]string{
		"Unit.SetLife": {"SetUnitLifeBJ", "SetUnitState"},
	}

	out1, changed, err := applyProvenanceToFile("x.go", []byte(src), prov)
	if err != nil {
		t.Fatal(err)
	}
	if !changed {
		t.Fatal("expected the first pass to change the file")
	}
	s1 := string(out1)
	// The JASS line is the last line of SetLife's doc, directly above the func.
	if !strings.Contains(s1, "// SetLife writes the unit's current life.\n// JASS: SetUnitLifeBJ, SetUnitState\nfunc (u Unit) SetLife") {
		t.Fatalf("provenance not appended as final doc line:\n%s", s1)
	}
	if strings.Contains(s1, "Untouched() {}\n// JASS") || strings.Count(s1, "// JASS:") != 1 {
		t.Fatalf("unmapped symbol got a JASS line, or too many lines:\n%s", s1)
	}

	// Idempotency: second pass is a no-op.
	out2, changed2, err := applyProvenanceToFile("x.go", out1, prov)
	if err != nil {
		t.Fatal(err)
	}
	if changed2 || string(out2) != s1 {
		t.Fatalf("second pass was not idempotent (changed=%v)", changed2)
	}

	// Staleness: a manifest change rewrites the existing line in place.
	prov["Unit.SetLife"] = []string{"SetUnitState"}
	out3, changed3, err := applyProvenanceToFile("x.go", out1, prov)
	if err != nil {
		t.Fatal(err)
	}
	if !changed3 {
		t.Fatal("expected a stale line to be rewritten")
	}
	if c := strings.Count(string(out3), "// JASS:"); c != 1 {
		t.Fatalf("stale rewrite duplicated the line (count=%d):\n%s", c, out3)
	}
	if !strings.Contains(string(out3), "// JASS: SetUnitState\nfunc (u Unit) SetLife") {
		t.Fatalf("stale line not rewritten to new sources:\n%s", out3)
	}
}

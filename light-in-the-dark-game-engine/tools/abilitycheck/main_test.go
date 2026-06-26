package main

import (
	"path/filepath"
	"testing"
)

// sev counts findings by severity.
func sev(fs []finding) (errs, warns int) {
	for _, f := range fs {
		if f.Severity == "error" {
			errs++
		} else {
			warns++
		}
	}
	return
}

// TestCleanFixturesPass: the clean fixtures (real templates) produce no errors.
func TestCleanFixturesPass(t *testing.T) {
	files, err := collectTOML([]string{"fixtures/clean"})
	if err != nil {
		t.Fatal(err)
	}
	if len(files) == 0 {
		t.Fatal("no clean fixtures found")
	}
	for _, f := range files {
		fs := checkFile(f, 64)
		if e, _ := sev(fs); e != 0 {
			t.Fatalf("%s: %d errors, want 0: %v", f, e, fs)
		}
	}
}

// TestDirtyBadRef: an undeclared effect-list reference is an error naming the id.
func TestDirtyBadRef(t *testing.T) {
	fs := checkFile(filepath.Join("fixtures/dirty", "bad-ref.toml"), 64)
	e, _ := sev(fs)
	if e == 0 {
		t.Fatal("bad-ref.toml passed; want a reference error")
	}
	found := false
	for _, f := range fs {
		if f.Rule == "reference" && contains(f.Message, "missing_list") {
			found = true
		}
	}
	if !found {
		t.Fatalf("no reference error naming the missing list: %v", fs)
	}
}

// TestDirtyBudgetWarn: a 5000-fan-out nova warns it could exhaust Caps.Movers.
func TestDirtyBudgetWarn(t *testing.T) {
	fs := checkFile(filepath.Join("fixtures/dirty", "mega-nova.toml"), 64)
	warn := false
	for _, f := range fs {
		if f.Rule == "budget" && f.Severity == "warning" {
			warn = true
		}
	}
	if !warn {
		t.Fatalf("mega-nova.toml did not warn on budget: %v", fs)
	}
}

// TestShippedTemplatesClean: the actual shipped template library passes clean —
// abilitycheck is the gate preflight runs over it.
func TestShippedTemplatesClean(t *testing.T) {
	dir := "../../docs/prd2/06-ability-composition/templates/specs"
	files, err := collectTOML([]string{dir})
	if err != nil {
		t.Fatal(err)
	}
	if len(files) != 6 {
		t.Fatalf("expected 6 shipped templates, found %d", len(files))
	}
	for _, f := range files {
		fs := checkFile(f, 64)
		if e, _ := sev(fs); e != 0 {
			t.Fatalf("shipped template %s has errors: %v", f, fs)
		}
	}
}

func contains(s, sub string) bool {
	return len(sub) == 0 || (len(s) >= len(sub) && indexOf(s, sub) >= 0)
}
func indexOf(s, sub string) int {
	for i := 0; i+len(sub) <= len(s); i++ {
		if s[i:i+len(sub)] == sub {
			return i
		}
	}
	return -1
}

package main

import (
	"strings"
	"testing"
)

// Every diagnostic the fixtures package must produce, matched by
// (file substring, message substring). The test fails on missing OR
// unexpected findings — the linter can neither under- nor over-flag.
var wantFindings = []string{
	`import "math" banned`,
	`import "math/rand" banned`,
	`import "crypto/rand" banned`,
	"range over map",
	"time.Now: wall-clock",
	"time.Since: wall-clock",
	"go statement",
	"select statement",
	"float in gameplay declaration (float64)", // struct field
	"float in gameplay declaration (float32)", // var decl
	"raw * on fixed.F64",
	"raw += on fixed.F64",
	"raw + on fixed.F64",
}

func TestFixturesProduceExactFindings(t *testing.T) {
	findings, audited, err := run([]string{"./fixtures"}, true)
	if err != nil {
		t.Fatal(err)
	}
	if audited != 1 {
		t.Fatalf("audited %d packages, want 1", audited)
	}
	for _, f := range findings {
		t.Logf("finding: %s", f)
	}

	remaining := append([]string{}, findings...)
	for _, want := range wantFindings {
		idx := -1
		for i, f := range remaining {
			if strings.Contains(f, want) {
				idx = i
				break
			}
		}
		if idx == -1 {
			t.Errorf("expected finding %q not produced", want)
			continue
		}
		remaining = append(remaining[:idx], remaining[idx+1:]...)
	}
	// the inferred-float64 `g := 1.5` also matches the float64 message;
	// account for it explicitly, then nothing may remain
	extraFloat := 0
	var leftover []string
	for _, f := range remaining {
		if strings.Contains(f, "float in gameplay declaration (float64)") {
			extraFloat++
			continue
		}
		leftover = append(leftover, f)
	}
	if extraFloat != 1 {
		t.Errorf("expected exactly 1 inferred-float64 finding beyond the struct field, got %d", extraFloat)
	}
	for _, f := range leftover {
		t.Errorf("unexpected finding: %s", f)
	}
}

// The real gameplay tree must be clean — and the scope filter must
// actually match packages (a vacuous gate is a broken gate).
func TestScopedTreeClean(t *testing.T) {
	findings, audited, err := run([]string{"./../../litd/..."}, false)
	if err != nil {
		t.Fatal(err)
	}
	for _, f := range findings {
		t.Errorf("scoped tree finding: %s", f)
	}
	if audited < 4 {
		t.Fatalf("audited %d scoped packages, want >= 4 (sim, sim/sched, fixed, prng, statehash)", audited)
	}
	t.Logf("clean: %d scoped packages audited, 0 findings", audited)
}

// fixed.F64 arithmetic INSIDE litd/fixed is allowed — the package
// audit above proves it stays clean while fixtures' identical
// expressions are flagged.
func TestF64ArithAllowedInsideFixedPkg(t *testing.T) {
	findings, audited, err := run([]string{"./../../litd/fixed"}, false)
	if err != nil {
		t.Fatal(err)
	}
	if audited != 1 {
		t.Fatalf("audited %d, want exactly litd/fixed", audited)
	}
	for _, f := range findings {
		t.Errorf("litd/fixed flagged: %s", f)
	}
	t.Log("litd/fixed (full of raw F64 operators by design) produces 0 findings")
}

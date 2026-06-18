package main

// Regression coverage for the G-1/G-5 godoc gate (#259). A lint that cannot
// fail is worthless, so the fixtures prove both directions: a fully-documented
// package yields zero findings, and a package with an undocumented field + a
// doc that leaks an internal reference is caught.

import "testing"

func TestAuditCleanFixtureHasNoFindings(t *testing.T) {
	findings, n, err := audit("testdata/clean")
	if err != nil {
		t.Fatalf("audit clean: %v", err)
	}
	if n == 0 {
		t.Fatal("counted zero exported symbols — fixture or walker broken")
	}
	if len(findings) != 0 {
		t.Fatalf("clean fixture must be finding-free, got %d: %v", len(findings), findings)
	}
	t.Logf("FSV clean: %d exported symbols, 0 findings", n)
}

func TestAuditDirtyFixtureHasTeeth(t *testing.T) {
	findings, _, err := audit("testdata/dirty")
	if err != nil {
		t.Fatalf("audit dirty: %v", err)
	}
	var g1, g5 int
	for _, f := range findings {
		switch f.rule {
		case "G-1":
			g1++
		case "G-5":
			g5++
		}
	}
	t.Logf("FSV dirty: findings=%v (G-1=%d G-5=%d)", findings, g1, g5)
	if g1 < 1 {
		t.Error("G-1 has no teeth: undocumented Gadget.Size not flagged")
	}
	if g5 < 1 {
		t.Error("G-5 has no teeth: litd/sim reference in Leaky's doc not flagged")
	}
}

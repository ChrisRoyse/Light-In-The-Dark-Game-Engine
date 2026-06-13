package main

import (
	"strings"
	"testing"
)

// wantFindings: every diagnostic the fixtures package must produce, matched by
// (location substring, message substring). The test fails on missing OR
// unexpected findings — the linter can neither under- nor over-flag.
var wantFindings = []struct{ where, msg string }{
	{"SixParamMethod", "G2.3"},                          // method, 6 positional
	{"SixPositional", "G2.3"},                           // free func, 6 positional
	{"KillThing", "G2.4"},                               // free func with handle param
	{"Leaky", `exported field "Exposed"`},               // handle with exported field
	{"NoValid", "no Valid() bool method"},               // handle missing Valid()
	{"BadVerb", "returns error"},                        // gameplay verb returns error
	{"Frob", "returns error"},                           // free verb returns error
	{"BadSig", "foreign engine type"},                   // G3N type in signature
	{"Trigger", `exported type "Trigger" is forbidden`}, // forbidden ident
	{"Location", `exported type "Location" is forbidden`},
	{"Surface", `exported field "Node" exposes foreign engine type`}, // G3N field
}

func TestFixturesProduceExactFindings(t *testing.T) {
	findings, audited, err := run([]string{"./fixtures"})
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
	for _, w := range wantFindings {
		idx := -1
		for i, f := range remaining {
			if strings.Contains(f, w.where) && strings.Contains(f, w.msg) {
				idx = i
				break
			}
		}
		if idx == -1 {
			t.Errorf("expected finding for %q containing %q not produced", w.where, w.msg)
			continue
		}
		remaining = append(remaining[:idx], remaining[idx+1:]...)
	}
	for _, f := range remaining {
		t.Errorf("unexpected finding (over-flag): %s", f)
	}
	if len(findings) != len(wantFindings) {
		t.Errorf("got %d findings, want exactly %d", len(findings), len(wantFindings))
	}
}

// The real public surface must be clean — and the audit must actually match a
// package (a vacuous gate is a broken gate).
func TestRealAPISurfaceClean(t *testing.T) {
	findings, audited, err := run([]string{"./../../litd/api"})
	if err != nil {
		t.Fatal(err)
	}
	for _, f := range findings {
		t.Errorf("litd/api finding (surface must be clean): %s", f)
	}
	if audited != 1 {
		t.Fatalf("audited %d packages, want exactly litd/api", audited)
	}
	t.Logf("clean: litd/api audited, 0 findings")
}

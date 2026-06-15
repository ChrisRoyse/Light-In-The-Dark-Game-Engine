package main

import (
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// repoRoot returns the repo root relative to the test CWD (tools/jassgen, as
// proven by TestLoadRealOverridesFile reading "overrides.toml").
func repoRoot(t *testing.T) string {
	t.Helper()
	root := filepath.Join("..", "..")
	if _, err := os.Stat(filepath.Join(root, "go.mod")); err != nil {
		t.Fatalf("repo root %q has no go.mod: %v", root, err)
	}
	return root
}

func unmarshalManifestForTest(t *testing.T, b []byte) Manifest {
	t.Helper()
	var m Manifest
	if err := json.Unmarshal(b, &m); err != nil {
		t.Fatalf("unmarshal manifest: %v", err)
	}
	return m
}

// TestReverseClosurePureCore exercises the I/O-free closure decision: a verb is
// accounted iff it is a manifest goMapping OR matched by a rule; everything else
// is reported. Covers the exact/suffix/prefix rule forms and the failure path.
func TestReverseClosurePureCore(t *testing.T) {
	syms := map[string]bool{"Unit.SetLife": true, "Game.CreateUnit": true}
	rules := []ClosureRule{
		{ruleSuffix, ".Valid"},
		{ruleSuffix, ".IsZero"},
		{rulePrefixFunc, "With"},
		{ruleExact, "Missile.Detonate"},
	}
	exports := []string{
		"Unit.SetLife",     // manifest -> accounted
		"Game.CreateUnit",  // manifest -> accounted
		"Unit.Valid",       // suffix .Valid -> accounted
		"Missile.IsZero",   // suffix .IsZero -> accounted
		"WithAttackType",   // prefix func With -> accounted
		"Missile.Detonate", // exact -> accounted
		"Unit.ScopeCreep",  // UNACCOUNTED
		"OrphanFreeFunc",   // UNACCOUNTED
	}
	got := ReverseClosure(exports, syms, rules)
	want := []string{"OrphanFreeFunc", "Unit.ScopeCreep"}
	if strings.Join(got, ",") != strings.Join(want, ",") {
		t.Fatalf("unaccounted = %v, want %v", got, want)
	}

	// Edge: prefix rule must NOT match a method (only bare funcs). "With" prefix
	// on "Unit.Within" must stay unaccounted.
	got2 := ReverseClosure([]string{"Unit.Within"}, nil, []ClosureRule{{rulePrefixFunc, "With"}})
	if len(got2) != 1 || got2[0] != "Unit.Within" {
		t.Fatalf("prefix-func rule wrongly matched a method: %v", got2)
	}

	// Edge: suffix rule must match on the full method name, not a substring.
	// ".Valid" must not match "Unit.Validate".
	got3 := ReverseClosure([]string{"Unit.Validate"}, nil, []ClosureRule{{ruleSuffix, ".Valid"}})
	if len(got3) != 1 || got3[0] != "Unit.Validate" {
		t.Fatalf("suffix rule matched a substring: %v", got3)
	}
}

// TestLoadClosureRulesParsing checks the whitelist grammar and its fail-closed
// errors (a malformed whitelist must not silently yield an empty ruleset).
func TestLoadClosureRulesParsing(t *testing.T) {
	dir := t.TempDir()
	good := filepath.Join(dir, "ok.txt")
	os.WriteFile(good, []byte(
		"# comment\n\nsuffix  .Valid\nprefix  func With\nexact   Missile.Detonate\n"), 0o644)
	rules, err := LoadClosureRules(good)
	if err != nil {
		t.Fatalf("load good: %v", err)
	}
	if len(rules) != 3 {
		t.Fatalf("parsed %d rules, want 3", len(rules))
	}

	cases := []struct{ body, sub string }{
		{"suffix Valid\n", "must start with '.'"},
		{"prefix Within\n", "prefix func"},
		{"bogus x\n", "unknown rule kind"},
		{"loneword\n", "malformed rule"},
	}
	for _, c := range cases {
		p := filepath.Join(dir, "bad.txt")
		os.WriteFile(p, []byte(c.body), 0o644)
		if _, err := LoadClosureRules(p); err == nil || !strings.Contains(err.Error(), c.sub) {
			t.Errorf("body %q: err = %v, want contains %q", c.body, err, c.sub)
		}
	}

	// A missing whitelist is an error (the gate must not pass for want of it).
	if _, err := LoadClosureRules(filepath.Join(dir, "nope.txt")); err == nil {
		t.Error("missing whitelist must error, not silently pass")
	}
}

// TestParsePanicStubs verifies the unimplemented-canonical detector: only an
// exported func/method whose body is a single bare panic(...) is reported; real
// bodies, multi-statement bodies, unexported names, and non-panic single calls
// are not.
func TestParsePanicStubs(t *testing.T) {
	dir := t.TempDir()
	src := `package p
type T struct{}
func (T) Stub() bool { panic("todo") }       // stub -> reported
func (T) Real() bool { return true }          // real -> not reported
func FreeStub() {}                            // empty body -> not a panic
func PanicStubFree() int { panic("nope") }    // stub -> reported
func multi() { panic("x"); _ = 1 }            // unexported -> skipped
func Logged() { println("x"); panic("y") }    // two stmts -> not reported
func NotPanic() { recover() }                 // single non-panic call -> not reported
`
	os.WriteFile(filepath.Join(dir, "p.go"), []byte(src), 0o644)
	os.WriteFile(filepath.Join(dir, "p_test.go"), []byte("package p\nfunc TestStub() { panic(\"x\") }\n"), 0o644)
	got, err := ParsePanicStubs(dir)
	if err != nil {
		t.Fatalf("parse: %v", err)
	}
	want := "PanicStubFree,T.Stub"
	if strings.Join(got, ",") != want {
		t.Fatalf("panic stubs = %v, want [%s]", got, want)
	}
}

// TestReverseClosureRealTreeGreen is the live gate: it parses the actual
// litd/api exports against the committed manifest + whitelist and asserts zero
// unaccounted verbs. This fires from `go test ./tools/jassgen/` even when the
// -revclosure CLI step is not run, so an added exported verb with no mapping/
// whitelist entry breaks the build. Paths are repo-root-relative; the test
// chdirs up from tools/jassgen.
func TestReverseClosureRealTreeGreen(t *testing.T) {
	root := repoRoot(t)
	exports, err := ParseAPIExports(filepath.Join(root, apiPackageDir))
	if err != nil {
		t.Fatalf("parse api exports: %v", err)
	}
	rules, err := LoadClosureRules(filepath.Join(root, newCapabilitiesPath))
	if err != nil {
		t.Fatalf("load whitelist: %v", err)
	}
	mb, err := os.ReadFile(filepath.Join(root, "api-manifest.json"))
	if err != nil {
		t.Fatalf("read manifest: %v", err)
	}
	m := unmarshalManifestForTest(t, mb)
	syms := manifestGoMappingSet(m, "litd/api")

	unaccounted := ReverseClosure(exports, syms, rules)
	t.Logf("FSV reverse-closure: %d verbs, %d mappings, %d rules, %d unaccounted",
		len(exports), len(syms), len(rules), len(unaccounted))
	if len(unaccounted) > 0 {
		t.Fatalf("%d exported verb(s) unaccounted (add a mapping or new-capabilities.txt rule): %v",
			len(unaccounted), unaccounted)
	}
}

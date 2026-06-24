package main

import (
	"strings"
	"testing"
)

// importcheck guards a core PRD §4.1 invariant — litd/sim must never reach
// litd/render, G3N, or GL. The binary was previously unverified by any test: it
// had an anti-vacuity guard but nothing proved it actually flags a forbidden
// import. These tests feed synthetic import graphs (known input → known
// violations) so the gate's teeth are evidence, not assumption.
//
// SoT = the returned []Violation vs the graph we constructed. X+X=Y: a graph
// with a sim→render edge MUST yield exactly that violation; a clean graph MUST
// yield none.

const mod = "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine"

var banned = []string{
	mod + "/litd/render",
	"github.com/g3n/engine",
	"github.com/go-gl/glfw",
}

// graph builds a path→pkg map from (path → its imports) pairs. Standard-lib
// packages are marked via stdlib().
func graph(edges map[string][]string, std map[string]bool) map[string]pkg {
	g := map[string]pkg{}
	for path, imps := range edges {
		g[path] = pkg{ImportPath: path, Imports: imps, Standard: std[path]}
	}
	return g
}

func TestIsBannedBoundaryFSV(t *testing.T) {
	cases := []struct {
		path string
		want bool
	}{
		{mod + "/litd/render", true},          // exact
		{mod + "/litd/render/mesh", true},     // under prefix
		{mod + "/litd/rendering", false},      // sibling — must NOT false-match
		{mod + "/litd/renderx", false},        // sibling
		{"github.com/g3n/engine", true},       // exact
		{"github.com/g3n/engine/math32", true},// under
		{mod + "/litd/sim", false},            // the root itself is allowed
		{"fmt", false},                        // stdlib
	}
	for _, c := range cases {
		if got := isBanned(c.path, banned); got != c.want {
			t.Errorf("isBanned(%q) = %v, want %v", c.path, got, c.want)
		}
	}
	t.Log("FSV: prefix boundary correct — 'rendering'/'renderx' do not match 'render'")
}

func TestFindViolationsCleanGraphFSV(t *testing.T) {
	// sim → fixed → (nothing banned). Happy path: zero violations.
	g := graph(map[string][]string{
		mod + "/litd/sim":   {mod + "/litd/fixed", "fmt"},
		mod + "/litd/fixed": {"math"},
	}, map[string]bool{"fmt": true, "math": true})
	v := findViolations(g, []string{mod + "/litd/sim"}, banned)
	if len(v) != 0 {
		t.Fatalf("clean graph yielded %d violation(s): %+v", len(v), v)
	}
	t.Log("FSV happy: a sim graph touching only fixed+stdlib has no violations")
}

func TestFindViolationsDirectEdgeFSV(t *testing.T) {
	// sim → render directly. Exactly one violation, chain [sim, render].
	g := graph(map[string][]string{
		mod + "/litd/sim":    {mod + "/litd/render"},
		mod + "/litd/render": {"fmt"},
	}, map[string]bool{"fmt": true})
	v := findViolations(g, []string{mod + "/litd/sim"}, banned)
	if len(v) != 1 {
		t.Fatalf("direct sim→render: got %d violations, want 1: %+v", len(v), v)
	}
	if v[0].Banned != mod+"/litd/render" {
		t.Fatalf("banned pkg = %q, want litd/render", v[0].Banned)
	}
	wantChain := []string{mod + "/litd/sim", mod + "/litd/render"}
	if strings.Join(v[0].Chain, ",") != strings.Join(wantChain, ",") {
		t.Fatalf("chain = %v, want %v", v[0].Chain, wantChain)
	}
	t.Logf("FSV direct: sim→render flagged, chain %v", v[0].Chain)
}

func TestFindViolationsTransitiveChainFSV(t *testing.T) {
	// sim → ai → audio → g3n. One violation; chain length 4 in walk order.
	g := graph(map[string][]string{
		mod + "/litd/sim":   {mod + "/litd/ai"},
		mod + "/litd/ai":    {mod + "/litd/audio"},
		mod + "/litd/audio": {"github.com/g3n/engine/loader"},
	}, nil)
	v := findViolations(g, []string{mod + "/litd/sim"}, banned)
	if len(v) != 1 {
		t.Fatalf("transitive: got %d violations, want 1: %+v", len(v), v)
	}
	want := []string{mod + "/litd/sim", mod + "/litd/ai", mod + "/litd/audio", "github.com/g3n/engine/loader"}
	if strings.Join(v[0].Chain, ",") != strings.Join(want, ",") {
		t.Fatalf("transitive chain = %v, want %v", v[0].Chain, want)
	}
	t.Logf("FSV transitive: 4-hop chain reconstructed: %v", v[0].Chain)
}

func TestFindViolationsStdlibStopsFSV(t *testing.T) {
	// A Standard package that (hypothetically) imports a banned pkg must NOT be
	// expanded — stdlib can't reach project code, so BFS terminates at it. This
	// guards the !Standard short-circuit: drop it and this graph would falsely
	// report a violation.
	g := graph(map[string][]string{
		mod + "/litd/sim": {"fmt"},
		"fmt":             {"github.com/g3n/engine"}, // synthetic; never happens, but proves the stop
	}, map[string]bool{"fmt": true})
	v := findViolations(g, []string{mod + "/litd/sim"}, banned)
	if len(v) != 0 {
		t.Fatalf("stdlib traversal not stopped: got %d violations: %+v", len(v), v)
	}
	t.Log("FSV stdlib-stop: a Standard package is not expanded, so its (synthetic) banned import is not reached")
}

func TestConfigSimRuleWiredFSV(t *testing.T) {
	// #311's "lint-enforced no import into litd/sim" for the settings data layer
	// must actually be in the rules list — guards against silent removal. And a
	// synthetic config→sim graph must be flagged by the same findViolations core.
	var found *rule
	for i := range rules {
		if rules[i].rootPrefix == mod+"/litd/config" {
			found = &rules[i]
		}
	}
	if found == nil {
		t.Fatal("config⊥sim rule missing from rules — #311 determinism guard not enforced")
	}
	if len(found.banned) != 1 || found.banned[0] != mod+"/litd/sim" {
		t.Fatalf("config rule bans %v, want [litd/sim]", found.banned)
	}
	// listTargets must include the config tree, else the rule's roots are empty
	// and the vacuity guard fires at runtime.
	hasConfig := false
	for _, tgt := range listTargets {
		if tgt == mod+"/litd/config/..." {
			hasConfig = true
		}
	}
	if !hasConfig {
		t.Fatalf("listTargets %v omits litd/config/... — config rule would be vacuous", listTargets)
	}
	// Teeth: a synthetic config pkg reaching sim is flagged.
	g := graph(map[string][]string{
		mod + "/litd/config": {mod + "/litd/sim"},
		mod + "/litd/sim":    {"fmt"},
	}, map[string]bool{"fmt": true})
	v := findViolations(g, []string{mod + "/litd/config"}, found.banned)
	if len(v) != 1 || v[0].Banned != mod+"/litd/sim" {
		t.Fatalf("synthetic config→sim not flagged: %+v", v)
	}
	t.Logf("FSV #311: config⊥sim rule wired; synthetic config→sim flagged, chain %v", v[0].Chain)
}

func TestRootsByPrefixFSV(t *testing.T) {
	g := graph(map[string][]string{
		mod + "/litd/sim":       nil,
		mod + "/litd/sim/order": nil,
		mod + "/litd/render":    nil,
		"fmt":                   nil,
	}, nil)
	roots := rootsByPrefix(g, mod+"/litd/sim")
	if len(roots) != 2 {
		t.Fatalf("rootsByPrefix found %d sim roots, want 2: %v", len(roots), roots)
	}
	// Empty prefix-miss → no roots (drives main()'s vacuity guard).
	if r := rootsByPrefix(g, mod+"/litd/nonexistent"); len(r) != 0 {
		t.Fatalf("missing prefix yielded roots: %v", r)
	}
	t.Log("FSV roots: both sim packages selected; a missing prefix yields zero (vacuity guard fires)")
}

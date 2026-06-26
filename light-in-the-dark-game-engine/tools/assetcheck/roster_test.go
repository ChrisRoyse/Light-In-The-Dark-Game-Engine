package main

// #517 roster cross-ref validator FSV. SoT = the findings slice the pass emits
// against synthetic roster trees with KNOWN-good and KNOWN-bad rows (the X+X=Y
// discipline): a clean roster yields zero findings; each kind of dangling ref
// yields exactly its rule. Text-only — no asset binaries needed.

import (
	"os"
	"path/filepath"
	"testing"
)

// writeRoster materializes a temp <root>/data tree + a sibling
// <root>/assets/MANIFEST, and returns (dataDir, relFiles) ready for
// checkRosterTables. Each entry in files maps a rel path -> contents.
func writeRoster(t *testing.T, files map[string]string, manifestPaths ...string) (string, []string) {
	t.Helper()
	root := t.TempDir()
	dataDir := filepath.Join(root, "data")
	var rel []string
	for p, body := range files {
		full := filepath.Join(dataDir, p)
		if err := os.MkdirAll(filepath.Dir(full), 0o755); err != nil {
			t.Fatal(err)
		}
		if err := os.WriteFile(full, []byte(body), 0o644); err != nil {
			t.Fatal(err)
		}
		rel = append(rel, p)
	}
	// Sibling MANIFEST.
	man := "# test manifest\n"
	for _, mp := range manifestPaths {
		man += "[[asset]]\npath = \"" + mp + "\"\n"
	}
	mdir := filepath.Join(root, "assets")
	if err := os.MkdirAll(mdir, 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(mdir, "MANIFEST"), []byte(man), 0o644); err != nil {
		t.Fatal(err)
	}
	return dataDir, rel
}

func runRoster(dataDir string, rel []string) []finding {
	var fs []finding
	add := func(path, rule, msg string) { fs = append(fs, finding{path, rule, msg}) }
	checkRosterTables(dataDir, rel, add)
	return fs
}

func rulesOf(fs []finding) map[string]int {
	m := map[string]int{}
	for _, f := range fs {
		m[f.Rule]++
	}
	return m
}

func TestRosterCleanNoFindingsFSV(t *testing.T) {
	dataDir, rel := writeRoster(t, map[string]string{
		"abilities/core.toml": "[[ability]]\nid = \"defend\"\n",
		"upgrades/up.toml":    "[[upgrade]]\nid = \"plating\"\n",
		"units/human.toml": `
[[unit]]
id = "footman"
model = "units/footman.glb"
abilities = ["defend"]
upgrades-used = ["plating"]

[[unit]]
id = "barracks"
model = "buildings/barracks.glb"

[[unit]]
id = "rifle"
model = "units/rifle.glb"
trained-at = "barracks"
requires = ["plating"]
`,
	}, "units/footman.glb", "buildings/barracks.glb", "units/rifle.glb")

	fs := runRoster(dataDir, rel)
	t.Logf("FSV clean roster findings=%v", fs)
	if len(fs) != 0 {
		t.Fatalf("clean roster should yield 0 findings, got %d: %v", len(fs), fs)
	}
}

func TestRosterDanglingRefsFSV(t *testing.T) {
	dataDir, rel := writeRoster(t, map[string]string{
		"abilities/core.toml": "[[ability]]\nid = \"defend\"\n",
		"upgrades/up.toml":    "[[upgrade]]\nid = \"plating\"\n",
		"units/bad.toml": `
[[unit]]
id = "dup"
model = "units/dup.glb"

[[unit]]
id = "dup"
model = "units/dup.glb"

[[unit]]
id = "x"
model = "units/dup.glb"
abilities = ["nope"]

[[unit]]
id = "y"
model = "units/dup.glb"
trained-at = "ghost"

[[unit]]
id = "z"
model = "units/dup.glb"
requires = ["missing"]

[[unit]]
id = "w"
model = "units/dup.glb"
upgrades-used = ["noup"]

[[unit]]
id = "m"
model = "units/dangling.glb"
`,
	}, "units/dup.glb") // dangling.glb deliberately absent from MANIFEST

	fs := runRoster(dataDir, rel)
	got := rulesOf(fs)
	t.Logf("FSV dangling roster findings=%v\n%v", got, fs)
	want := map[string]int{
		"ROSTER-DUP-ID":        1, // second "dup"
		"ROSTER-ABILITY-REF":   1, // x -> nope
		"ROSTER-TRAINEDAT-REF": 1, // y -> ghost
		"ROSTER-REQUIRES-REF":  1, // z -> missing
		"ROSTER-UPGRADE-REF":   1, // w -> noup
		"ROSTER-MODEL":         1, // m -> dangling.glb
	}
	for rule, n := range want {
		if got[rule] != n {
			t.Fatalf("rule %s: got %d findings, want %d\nall=%v", rule, got[rule], n, fs)
		}
	}
	// No other rules should fire.
	for rule := range got {
		if _, ok := want[rule]; !ok {
			t.Fatalf("unexpected rule %s fired: %v", rule, fs)
		}
	}
}

func TestRosterNoManifestSkipsModelCheckFSV(t *testing.T) {
	// Same dangling model, but without a reachable MANIFEST the asset cross-ref
	// is SKIPPED (fail-closed: can't validate => don't false-flag), while the
	// entry cross-ref (bad ability) still fires.
	root := t.TempDir()
	dataDir := filepath.Join(root, "isolated") // no sibling assets/MANIFEST
	files := map[string]string{
		"abilities/core.toml": "[[ability]]\nid = \"defend\"\n",
		"units/u.toml":        "[[unit]]\nid = \"a\"\nmodel = \"units/ghost.glb\"\nabilities = [\"nope\"]\n",
	}
	var rel []string
	for p, body := range files {
		full := filepath.Join(dataDir, p)
		if err := os.MkdirAll(filepath.Dir(full), 0o755); err != nil {
			t.Fatal(err)
		}
		if err := os.WriteFile(full, []byte(body), 0o644); err != nil {
			t.Fatal(err)
		}
		rel = append(rel, p)
	}
	fs := runRoster(dataDir, rel)
	got := rulesOf(fs)
	t.Logf("FSV no-manifest findings=%v", fs)
	if got["ROSTER-MODEL"] != 0 {
		t.Fatalf("model check should be skipped without a MANIFEST, got %v", fs)
	}
	if got["ROSTER-ABILITY-REF"] != 1 {
		t.Fatalf("entry cross-ref should still fire without a MANIFEST, got %v", fs)
	}
}

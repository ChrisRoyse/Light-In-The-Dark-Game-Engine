package main

import (
	"os"
	"strings"
	"testing"
)

// #187 FSV: the public Lua API reference generator. SoT = the rendered Markdown
// vs the manifest. Synthetic: a mapped function appears with its signature, a
// tombstoned one is omitted. Real: the committed docs/api/lua-reference.md is
// byte-identical to a fresh generation from api-manifest.json (the drift gate).

func synthManifest() manifest {
	return manifest{
		SchemaVersion: 1,
		Functions: []function{
			{
				Name: "SetUnitLife", Origin: "common.j", Disposition: "mapped",
				Signature: signature{Params: []param{{Name: "u", JassType: "unit"}, {Name: "hp", JassType: "real"}}, Returns: "nothing"},
				GoMapping: &goMap{Package: "litd/api", Symbol: "Unit.SetLife", GoSignature: "(u Unit) SetLife(hp Fixed)"},
			},
			{
				Name: "AbortCinematicFadeBJ", Origin: "blizzard.j", Disposition: "tombstoned",
				Signature: signature{Returns: "nothing"},
			},
		},
	}
}

func TestLuadocSyntheticFSV(t *testing.T) {
	ref := GenerateReference(synthManifest())
	t.Logf("AFTER:\n%s", ref)

	if !strings.Contains(ref, "## SetUnitLife") {
		t.Fatal("mapped function SetUnitLife missing from reference")
	}
	if !strings.Contains(ref, "`SetUnitLife(u: unit, hp: real) -> nothing`") {
		t.Fatalf("SetUnitLife signature wrong:\n%s", ref)
	}
	if !strings.Contains(ref, "litd/api.Unit.SetLife") {
		t.Fatal("Go mapping missing")
	}
	if strings.Contains(ref, "AbortCinematicFadeBJ") {
		t.Fatal("tombstoned function leaked into the public reference")
	}
	if !strings.Contains(ref, "1 callable functions") {
		t.Fatalf("count line wrong (want 1 callable):\n%s", ref)
	}

	// Determinism: a second render is byte-identical.
	if GenerateReference(synthManifest()) != ref {
		t.Fatal("reference generation is not deterministic")
	}
}

func TestLuadocRealManifestInSyncFSV(t *testing.T) {
	m, err := loadManifest("../../api-manifest.json")
	if err != nil {
		t.Skipf("api-manifest.json not present: %v", err)
	}
	ref := GenerateReference(m)

	// Spot-check: an active function present with signature; a tombstoned absent.
	if !strings.Contains(ref, "## AngleBetweenPoints") ||
		!strings.Contains(ref, "`AngleBetweenPoints(locA: location, locB: location) -> real`") {
		t.Fatal("real manifest: AngleBetweenPoints entry/signature missing")
	}
	if strings.Contains(ref, "AbortCinematicFadeBJ") {
		t.Fatal("real manifest: tombstoned function leaked into the reference")
	}
	got := strings.Count(ref, "\n## ")
	t.Logf("real manifest → %d callable function entries", got)
	if got != 410 {
		t.Fatalf("entry count = %d, want 410 (the mapped functions)", got)
	}

	// Drift gate: the committed file must equal a fresh generation.
	committed, err := os.ReadFile("../../docs/api/lua-reference.md")
	if err != nil {
		t.Fatalf("read committed reference: %v", err)
	}
	if string(committed) != ref {
		t.Fatal("docs/api/lua-reference.md is STALE vs api-manifest.json — run `go run ./tools/luadoc`")
	}
	t.Log("FSV drift: committed lua-reference.md is byte-identical to a fresh generation")
}

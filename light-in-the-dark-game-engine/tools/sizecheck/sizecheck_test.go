package main

import (
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/tools/assetcheck/manifest"
)

// #310 FSV: the binary+assets size gate. SoT = the computed Report vs known
// synthetic inputs, and the real budgets.toml value. No mocks of the summing.

const mb = 1 << 20

func TestSizecheckMeasureFSV(t *testing.T) {
	assets := []manifest.Asset{
		{Path: "u1.glb", Category: "unit", Bytes: 100 * mb},
		{Path: "u2.glb", Category: "unit", Bytes: 20 * mb},
		{Path: "b1.glb", Category: "building", Bytes: 30 * mb},
		{Path: "icon.png", Category: "", Bytes: 5 * mb}, // uncategorized
	}
	budget := int64(300 * mb)
	r := Measure(50*mb, assets, budget)
	t.Logf("BEFORE budget=%d MiB\n%s", budget/mb, r.Format())

	// 50 binary + (100+20+30+5) assets = 205 MiB; every asset measured.
	if r.AssetBytes != 155*mb {
		t.Fatalf("asset bytes = %d MiB, want 155", r.AssetBytes/mb)
	}
	if r.Total != 205*mb {
		t.Fatalf("total = %d MiB, want 205", r.Total/mb)
	}
	if r.ByCategory["unit"] != 120*mb || r.ByCategory["building"] != 30*mb || r.ByCategory["uncategorized"] != 5*mb {
		t.Fatalf("category breakdown wrong: %v", r.ByCategory)
	}
	if len(r.MissingBytes) != 0 || r.Incomplete() {
		t.Fatalf("all assets measured, but MissingBytes = %v", r.MissingBytes)
	}
	if r.OverBudget() {
		t.Fatal("205 MiB must be within a 300 MiB budget")
	}
	if !strings.Contains(r.Format(), "within budget") {
		t.Fatalf("format missing within-budget line:\n%s", r.Format())
	}
	// Determinism.
	if Measure(50*mb, assets, budget).Format() != r.Format() {
		t.Fatal("Format not deterministic")
	}
}

// TestSizecheckIncompleteFailsClosedFSV: an asset with no declared size makes the
// gate FAIL even when the measured total is within budget — otherwise a large
// asset added without a bytes field would silently evade the 300 MiB ceiling
// (§2.4 fail-closed). Regression for the #539/#310 gap where 0 of 632 MANIFEST
// rows carried bytes and the budget effectively measured the binary alone.
func TestSizecheckIncompleteFailsClosedFSV(t *testing.T) {
	// One unmeasured asset; the measured part is trivially within budget.
	assets := []manifest.Asset{
		{Path: "measured.glb", Category: "unit", Bytes: 10 * mb},
		{Path: "sneaky.glb", Category: "unit", Bytes: 0}, // no declared size
	}
	r := Measure(5*mb, assets, 300*mb)
	t.Logf("incomplete report:\n%s", r.Format())
	if r.OverBudget() {
		t.Fatal("measured total (15 MiB) is within 300 MiB — the failure here must come from incompleteness, not over-budget")
	}
	if !r.Incomplete() {
		t.Fatal("an asset with unspecified Bytes must make the report Incomplete (fail-closed)")
	}
	if len(r.MissingBytes) != 1 || r.MissingBytes[0] != "sneaky.glb" {
		t.Fatalf("missing-bytes = %v, want [sneaky.glb]", r.MissingBytes)
	}
	if !strings.Contains(r.Format(), "GATE FAILS") {
		t.Fatalf("incomplete report must print a GATE FAILS verdict, not within-budget:\n%s", r.Format())
	}
	// And a fully-measured report is NOT incomplete (the gate stays green normally).
	ok := Measure(5*mb, []manifest.Asset{{Path: "measured.glb", Bytes: 10 * mb}}, 300*mb)
	if ok.Incomplete() {
		t.Fatalf("fully-measured report wrongly Incomplete: %v", ok.MissingBytes)
	}
	t.Log("FSV fail-closed: an unmeasured asset fails the gate; a fully-measured set passes")
}

func TestSizecheckOverBudgetFSV(t *testing.T) {
	// Issue edge 2: a 400 MiB dummy asset blows the budget; breakdown names it.
	assets := []manifest.Asset{
		{Path: "huge.glb", Category: "unit", Bytes: 400 * mb},
	}
	r := Measure(20*mb, assets, 300*mb)
	t.Logf("AFTER (over):\n%s", r.Format())
	if !r.OverBudget() {
		t.Fatal("420 MiB must exceed a 300 MiB budget")
	}
	out := r.Format()
	if !strings.Contains(out, "OVER BUDGET") || !strings.Contains(out, "unit") {
		t.Fatalf("over-budget format must name the category:\n%s", out)
	}
}

func TestSizecheckBudgetFromRealFileFSV(t *testing.T) {
	got, err := budgetFromFile("../../benchmarks/budgets.toml")
	if err != nil {
		t.Fatalf("budgetFromFile(real): %v", err)
	}
	t.Logf("FSV: real budgets.toml binary_assets_bytes_max = %d (%d MiB)", got, got/mb)
	if got != 314572800 {
		t.Fatalf("budget = %d, want 314572800 (300 MiB)", got)
	}
}

func TestSizecheckBudgetFailClosedFSV(t *testing.T) {
	dir := t.TempDir()
	// Unknown key → error (fail-closed).
	bad := filepath.Join(dir, "bad.toml")
	if err := os.WriteFile(bad, []byte("[budgets]\nbogus_key = 5\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	if _, err := budgetFromFile(bad); err == nil {
		t.Fatal("unknown budget key must error")
	}
	// Tolerates the other known keys but still requires our key.
	noKey := filepath.Join(dir, "nokey.toml")
	if err := os.WriteFile(noKey, []byte("[budgets]\ntick_ms_max = 10\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	if _, err := budgetFromFile(noKey); err == nil {
		t.Fatal("missing binary_assets_bytes_max must error")
	}
	t.Log("FSV fail-closed: unknown key and missing key both rejected")
}

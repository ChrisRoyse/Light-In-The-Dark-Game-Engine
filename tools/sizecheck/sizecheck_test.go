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
		{Path: "todo.glb", Category: "unit", Bytes: 0},  // unspecified → not counted
	}
	budget := int64(300 * mb)
	r := Measure(50*mb, assets, budget)
	t.Logf("BEFORE budget=%d MiB\n%s", budget/mb, r.Format())

	// 50 binary + (100+20+30+5) assets = 205 MiB; todo.glb excluded.
	if r.AssetBytes != 155*mb {
		t.Fatalf("asset bytes = %d MiB, want 155", r.AssetBytes/mb)
	}
	if r.Total != 205*mb {
		t.Fatalf("total = %d MiB, want 205", r.Total/mb)
	}
	if r.ByCategory["unit"] != 120*mb || r.ByCategory["building"] != 30*mb || r.ByCategory["uncategorized"] != 5*mb {
		t.Fatalf("category breakdown wrong: %v", r.ByCategory)
	}
	if len(r.MissingBytes) != 1 || r.MissingBytes[0] != "todo.glb" {
		t.Fatalf("missing-bytes = %v, want [todo.glb]", r.MissingBytes)
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

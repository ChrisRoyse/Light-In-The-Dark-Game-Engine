// Command sizecheck is the #310 binary+assets size gate: it sums the release
// binary size and the per-category asset bytes declared in assets/MANIFEST (the
// Bytes field, #539 — so the gitignored asset files need not be present) and
// fails if the total exceeds binary_assets_bytes_max in benchmarks/budgets.toml
// (PRD §5.3, hard 300 MB ceiling). It prints a category breakdown.
//
// Assets whose Bytes are unspecified (0) make the gate FAIL (fail-closed): an
// unmeasured asset is not silently counted as 0, because that would let a large
// asset added without a bytes field evade the ceiling. scripts/manifest-add.sh
// emits bytes for every new asset, so the normal add path never trips this. The cold-start ≤5 s and map-
// load ≤10 s halves of #310 need the render path + the firstflame archive (#209)
// and stay gated; this is the size budget, which is honestly headless.
package main

import (
	"bufio"
	"fmt"
	"os"
	"sort"
	"strconv"
	"strings"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/tools/assetcheck/manifest"
)

// Report is the measured size breakdown.
type Report struct {
	BinaryBytes  int64
	ByCategory   map[string]int64 // category ("" → "uncategorized") → asset bytes
	AssetBytes   int64            // Σ ByCategory
	Total        int64            // BinaryBytes + AssetBytes
	Budget       int64
	MissingBytes []string // asset paths with unspecified Bytes (not counted)
}

// OverBudget reports whether the gate fails.
func (r Report) OverBudget() bool { return r.Total > r.Budget }

// Incomplete reports whether any asset lacked a declared size, so the measured
// total is only a lower bound. Fail-closed: a budget that ignores unmeasured
// assets can be silently evaded by adding a large asset without a bytes field, so
// an incomplete report fails the gate just like an over-budget one.
func (r Report) Incomplete() bool { return len(r.MissingBytes) > 0 }

// Measure builds the report from a binary size, the manifest assets, and a budget.
func Measure(binaryBytes int64, assets []manifest.Asset, budget int64) Report {
	byCat := map[string]int64{}
	for _, a := range assets {
		if a.Bytes <= 0 {
			continue
		}
		cat := a.Category
		if cat == "" {
			cat = "uncategorized"
		}
		byCat[cat] += a.Bytes
	}
	total, missing := manifest.TotalBytes(assets)
	r := Report{
		BinaryBytes:  binaryBytes,
		ByCategory:   byCat,
		AssetBytes:   total,
		Total:        binaryBytes + total,
		Budget:       budget,
		MissingBytes: missing,
	}
	return r
}

// Format renders a deterministic breakdown.
func (r Report) Format() string {
	var b strings.Builder
	fmt.Fprintf(&b, "size budget (#310): total %s / budget %s\n", mib(r.Total), mib(r.Budget))
	fmt.Fprintf(&b, "  binary           %s\n", mib(r.BinaryBytes))
	fmt.Fprintf(&b, "  assets           %s\n", mib(r.AssetBytes))
	cats := make([]string, 0, len(r.ByCategory))
	for c := range r.ByCategory {
		cats = append(cats, c)
	}
	sort.Strings(cats)
	for _, c := range cats {
		fmt.Fprintf(&b, "    %-14s %s\n", c, mib(r.ByCategory[c]))
	}
	if len(r.MissingBytes) > 0 {
		fmt.Fprintf(&b, "  %d asset(s) have unspecified Bytes (not counted) — size is only a lower bound\n", len(r.MissingBytes))
	}
	switch {
	case r.OverBudget():
		fmt.Fprintf(&b, "OVER BUDGET by %s\n", mib(r.Total-r.Budget))
	case r.Incomplete():
		fmt.Fprintf(&b, "GATE FAILS: %d asset(s) unmeasured — populate MANIFEST bytes (scripts/manifest-add.sh emits it); the budget cannot be certified\n", len(r.MissingBytes))
	default:
		fmt.Fprintf(&b, "within budget (%s headroom)\n", mib(r.Budget-r.Total))
	}
	return b.String()
}

func mib(n int64) string {
	return fmt.Sprintf("%.2f MiB (%d B)", float64(n)/(1<<20), n)
}

// budgetFromFile reads binary_assets_bytes_max from a budgets.toml, tolerating
// (ignoring) the other known budget keys so the shared single-source file stays
// readable. Unknown keys and non-integers are errors (fail-closed).
func budgetFromFile(path string) (int64, error) {
	f, err := os.Open(path)
	if err != nil {
		return 0, err
	}
	defer f.Close()
	known := map[string]bool{"tick_ms_max": true, "allocs_per_tick": true, "binary_assets_bytes_max": true}
	var got int64
	var found bool
	sc := bufio.NewScanner(f)
	line := 0
	for sc.Scan() {
		line++
		text := strings.TrimSpace(sc.Text())
		if text == "" || strings.HasPrefix(text, "#") || text == "[budgets]" {
			continue
		}
		k, v, ok := strings.Cut(text, "=")
		if !ok {
			return 0, fmt.Errorf("%s:%d: not key = value: %q", path, line, text)
		}
		key := strings.TrimSpace(k)
		val := strings.TrimSpace(v)
		if i := strings.Index(val, "#"); i >= 0 {
			val = strings.TrimSpace(val[:i])
		}
		if !known[key] {
			return 0, fmt.Errorf("%s:%d: unknown budget key %q", path, line, key)
		}
		n, err := strconv.ParseInt(val, 10, 64)
		if err != nil {
			return 0, fmt.Errorf("%s:%d: value for %s not an integer: %q", path, line, key, val)
		}
		if key == "binary_assets_bytes_max" {
			got, found = n, true
		}
	}
	if err := sc.Err(); err != nil {
		return 0, err
	}
	if !found {
		return 0, fmt.Errorf("%s: missing binary_assets_bytes_max", path)
	}
	return got, nil
}

func main() {
	var binPath, assetsDir, budgetsPath string
	args := os.Args[1:]
	for i := 0; i < len(args); i++ {
		switch args[i] {
		case "-bin":
			i++
			binPath = args[i]
		case "-assets":
			i++
			assetsDir = args[i]
		case "-budgets":
			i++
			budgetsPath = args[i]
		default:
			fmt.Fprintf(os.Stderr, "sizecheck: unknown arg %q\n", args[i])
			os.Exit(2)
		}
	}
	if assetsDir == "" {
		assetsDir = "assets"
	}
	if budgetsPath == "" {
		budgetsPath = "benchmarks/budgets.toml"
	}
	if binPath == "" {
		fmt.Fprintln(os.Stderr, "sizecheck: -bin <release-binary> is required")
		os.Exit(2)
	}

	fi, err := os.Stat(binPath)
	if err != nil {
		fmt.Fprintln(os.Stderr, "sizecheck:", err)
		os.Exit(1)
	}
	assets, err := manifest.Load(assetsDir)
	if err != nil {
		fmt.Fprintln(os.Stderr, "sizecheck: load manifest:", err)
		os.Exit(1)
	}
	budget, err := budgetFromFile(budgetsPath)
	if err != nil {
		fmt.Fprintln(os.Stderr, "sizecheck: budget:", err)
		os.Exit(1)
	}

	r := Measure(fi.Size(), assets, budget)
	fmt.Print(r.Format())
	if r.OverBudget() || r.Incomplete() {
		os.Exit(1)
	}
}

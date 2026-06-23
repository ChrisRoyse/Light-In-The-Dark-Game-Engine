// benchharness runs the registered benchmark suites, compares every
// metric against benchmarks/budgets.toml (the single source of budget
// numbers, tooling.md §4.3), appends one JSON line per metric to the
// results history, and prints deltas against the previous run.
//
// Gate semantics (R-GC-5), exactly:
//   - allocs/op > allocs_per_tick on a per-tick metric  -> FAIL
//   - ns/op over the tick_ms_max budget                 -> FAIL
//   - within budget but ≥10% worse than the rolling
//     window (last 5 runs) mean                         -> WARN, not fail
//
// Usage: go run ./tools/benchharness [-history benchmarks/history.jsonl]
package main

import (
	"bufio"
	"encoding/json"
	"flag"
	"fmt"
	"os"
	"os/exec"
	"regexp"
	"strconv"
	"strings"
	"time"
)

// suite registers one benchmark as a harness metric. perTick metrics
// are gated against tick_ms_max/allocs_per_tick; others are
// trend-tracked only.
type suite struct {
	metric  string
	pkg     string
	bench   string // exact Benchmark name
	perTick bool
}

var suites = []suite{
	{metric: "fixed_mul_ns", pkg: "./litd/fixed", bench: "BenchmarkMul"},
	{metric: "fixed_div_ns", pkg: "./litd/fixed", bench: "BenchmarkDiv"},
	{metric: "sched_step_1000_scripts", pkg: "./litd/sim/sched", bench: "BenchmarkStepSteadyState1000", perTick: true},
	{metric: "world_tick", pkg: "./litd/sim", bench: "BenchmarkWorldTick", perTick: true},
}

type budgets struct {
	tickMsMax     int64
	allocsPerTick int64
	// binaryAssetsBytesMax is consumed by tools/sizecheck (#310), not here; the
	// harness accepts the key so the shared single-source budgets.toml stays
	// readable by both tools (the file is THE place a budget literal lives).
	binaryAssetsBytesMax int64
}

// parseBudgets is a strict TOML-subset reader: one [budgets] table,
// `key = integer` lines, comments with #. Unknown keys, duplicate
// keys, or non-integer values are errors — a half-read budget file
// must never gate anything (fail-closed).
func parseBudgets(path string) (budgets, error) {
	var b budgets
	f, err := os.Open(path)
	if err != nil {
		return b, err
	}
	defer f.Close()
	seen := map[string]bool{}
	inTable := false
	sc := bufio.NewScanner(f)
	line := 0
	for sc.Scan() {
		line++
		text := strings.TrimSpace(sc.Text())
		if text == "" || strings.HasPrefix(text, "#") {
			continue
		}
		if text == "[budgets]" {
			inTable = true
			continue
		}
		if strings.HasPrefix(text, "[") {
			return b, fmt.Errorf("%s:%d: unknown table %s", path, line, text)
		}
		if !inTable {
			return b, fmt.Errorf("%s:%d: key outside [budgets]", path, line)
		}
		k, v, ok := strings.Cut(text, "=")
		if !ok {
			return b, fmt.Errorf("%s:%d: not key = value: %q", path, line, text)
		}
		key := strings.TrimSpace(k)
		val := strings.TrimSpace(v)
		if i := strings.Index(val, "#"); i >= 0 {
			val = strings.TrimSpace(val[:i])
		}
		n, err := strconv.ParseInt(val, 10, 64)
		if err != nil {
			return b, fmt.Errorf("%s:%d: value for %s not an integer: %q", path, line, key, val)
		}
		if seen[key] {
			return b, fmt.Errorf("%s:%d: duplicate key %s", path, line, key)
		}
		seen[key] = true
		switch key {
		case "tick_ms_max":
			b.tickMsMax = n
		case "allocs_per_tick":
			b.allocsPerTick = n
		case "binary_assets_bytes_max":
			b.binaryAssetsBytesMax = n // read by tools/sizecheck, not the harness
		default:
			return b, fmt.Errorf("%s:%d: unknown budget key %q", path, line, key)
		}
	}
	if err := sc.Err(); err != nil {
		return b, err
	}
	if !seen["tick_ms_max"] || !seen["allocs_per_tick"] {
		return b, fmt.Errorf("%s: missing required budget keys", path)
	}
	return b, nil
}

type result struct {
	Time     string  `json:"time"`
	Metric   string  `json:"metric"`
	NsPerOp  float64 `json:"ns_per_op"`
	AllocsOp int64   `json:"allocs_per_op"`
	BytesOp  int64   `json:"bytes_per_op"`
	Verdict  string  `json:"verdict"`
	Budget   string  `json:"budget_key,omitempty"`
}

var benchLine = regexp.MustCompile(`^(Benchmark\S+)(?:-\d+)?\s+(\d+)\s+([\d.]+) ns/op(?:\s+(\d+) B/op)?(?:\s+(\d+) allocs/op)?`)

func runBench(s suite) (result, error) {
	cmd := exec.Command("go", "test", "-run", "^$", "-bench", "^"+s.bench+"$",
		"-benchmem", "-count", "1", s.pkg)
	out, err := cmd.CombinedOutput()
	if err != nil {
		return result{}, fmt.Errorf("%s: %v\n%s", s.pkg, err, out)
	}
	for _, line := range strings.Split(string(out), "\n") {
		m := benchLine.FindStringSubmatch(strings.TrimSpace(line))
		if m == nil {
			continue
		}
		ns, _ := strconv.ParseFloat(m[3], 64)
		bytes, _ := strconv.ParseInt(m[4], 10, 64)
		allocs, _ := strconv.ParseInt(m[5], 10, 64)
		return result{Metric: s.metric, NsPerOp: ns, BytesOp: bytes, AllocsOp: allocs}, nil
	}
	return result{}, fmt.Errorf("%s: no benchmark line for %s in output:\n%s", s.pkg, s.bench, out)
}

func loadHistory(path string) map[string][]result {
	h := map[string][]result{}
	f, err := os.Open(path)
	if err != nil {
		return h
	}
	defer f.Close()
	sc := bufio.NewScanner(f)
	for sc.Scan() {
		var r result
		if json.Unmarshal(sc.Bytes(), &r) == nil && r.Metric != "" {
			h[r.Metric] = append(h[r.Metric], r)
		}
	}
	return h
}

func main() {
	historyPath := flag.String("history", "benchmarks/history.jsonl", "JSON-lines results history")
	budgetsPath := flag.String("budgets", "benchmarks/budgets.toml", "budget file (single source of truth)")
	flag.Parse()

	bud, err := parseBudgets(*budgetsPath)
	if err != nil {
		fmt.Fprintf(os.Stderr, "benchharness: %v\n", err)
		os.Exit(2)
	}
	history := loadHistory(*historyPath)
	tickBudgetNs := float64(bud.tickMsMax) * 1e6

	fmt.Printf("benchharness: budgets from %s: tick_ms_max=%d allocs_per_tick=%d\n\n",
		*budgetsPath, bud.tickMsMax, bud.allocsPerTick)
	fmt.Printf("%-26s %14s %10s %-22s %s\n", "metric", "ns/op", "allocs/op", "gate", "trend")

	now := time.Now().UTC().Format(time.RFC3339)
	failed := false
	var lines []result
	for _, s := range suites {
		r, err := runBench(s)
		if err != nil {
			fmt.Fprintf(os.Stderr, "benchharness: %v\n", err)
			os.Exit(2)
		}
		r.Time = now

		gate := "trend-only"
		r.Verdict = "OK"
		if s.perTick {
			switch {
			case r.AllocsOp > bud.allocsPerTick:
				r.Verdict = "FAIL"
				r.Budget = "allocs_per_tick"
				gate = fmt.Sprintf("FAIL allocs %d > allocs_per_tick=%d", r.AllocsOp, bud.allocsPerTick)
				failed = true
			case r.NsPerOp > tickBudgetNs:
				r.Verdict = "FAIL"
				r.Budget = "tick_ms_max"
				gate = fmt.Sprintf("FAIL %.0fns > tick_ms_max=%dms", r.NsPerOp, bud.tickMsMax)
				failed = true
			default:
				r.Budget = "tick_ms_max"
				gate = fmt.Sprintf("OK (≤ tick_ms_max=%dms, allocs=%d)", bud.tickMsMax, r.AllocsOp)
			}
		}

		// rolling-window trend: last ≤5 prior entries; ≥10% worse than
		// the window mean is a WARNING, never a failure (R-GC-5).
		trend := "no history"
		if prev := history[r.Metric]; len(prev) > 0 {
			win := prev
			if len(win) > 5 {
				win = win[len(win)-5:]
			}
			var sum float64
			for _, p := range win {
				sum += p.NsPerOp
			}
			mean := sum / float64(len(win))
			deltaPct := (r.NsPerOp - mean) / mean * 100
			last := prev[len(prev)-1]
			trend = fmt.Sprintf("Δprev %+0.1f%%, Δwindow(%d) %+0.1f%%",
				(r.NsPerOp-last.NsPerOp)/last.NsPerOp*100, len(win), deltaPct)
			if deltaPct >= 10 && r.Verdict == "OK" {
				r.Verdict = "WARN"
				trend += "  WARNING: ≥10% over rolling window (non-blocking)"
			}
		}

		fmt.Printf("%-26s %14.1f %10d %-22s %s\n", r.Metric, r.NsPerOp, r.AllocsOp, gate, trend)
		lines = append(lines, r)
	}

	if err := appendHistory(*historyPath, lines); err != nil {
		fmt.Fprintf(os.Stderr, "benchharness: history append: %v\n", err)
		os.Exit(2)
	}
	fmt.Printf("\nhistory: %d lines appended to %s\n", len(lines), *historyPath)

	if failed {
		fmt.Println("benchharness: FAIL (budget violation, R-GC-5)")
		os.Exit(1)
	}
	fmt.Println("benchharness: PASS")
}

func appendHistory(path string, lines []result) error {
	if dir := strings.TrimSuffix(path, "/"+pathBase(path)); dir != path && dir != "" {
		if err := os.MkdirAll(dir, 0o755); err != nil {
			return err
		}
	}
	f, err := os.OpenFile(path, os.O_APPEND|os.O_CREATE|os.O_WRONLY, 0o644)
	if err != nil {
		return err
	}
	defer f.Close()
	enc := json.NewEncoder(f)
	for _, r := range lines {
		if err := enc.Encode(r); err != nil {
			return err
		}
	}
	return nil
}

func pathBase(p string) string {
	if i := strings.LastIndex(p, "/"); i >= 0 {
		return p[i+1:]
	}
	return p
}

package main

import (
	"os"
	"path/filepath"
	"testing"
)

// benchharness is the R-GC-5 perf gate (allocs/op and ns/op vs budgets.toml) and
// runs in the FULL preflight, but had NO test: the verdict logic that decides
// FAIL vs OK was unverified, so a > vs >= slip or a wrong-field comparison would
// silently neuter the gate. These feed synthetic results with known budgets and
// assert the verdict — the gate's teeth as evidence (the #543 gate-teeth pattern).

var testBud = budgets{tickMsMax: 10, allocsPerTick: 0} // 10 ms, zero-alloc

func TestEvaluateAllocsBreachFSV(t *testing.T) {
	v, _, key := evaluate(result{AllocsOp: 1, NsPerOp: 1e6}, testBud, true)
	if v != "FAIL" || key != "allocs_per_tick" {
		t.Fatalf("1 alloc over a 0 budget: got (%s,%s), want (FAIL,allocs_per_tick)", v, key)
	}
}

func TestEvaluateAllocsBoundaryFSV(t *testing.T) {
	// allocs == budget is WITHIN budget. This is the > vs >= guard: a regression
	// to >= would fail a zero-alloc tick that exactly meets the zero budget.
	v, _, _ := evaluate(result{AllocsOp: 0, NsPerOp: 1e6}, testBud, true)
	if v != "OK" {
		t.Fatalf("0 allocs at a 0 budget must be OK (equality within budget), got %s", v)
	}
}

func TestEvaluateTimeBreachFSV(t *testing.T) {
	// 10 ms budget = 10e6 ns; 10e6+1 must fail on time.
	v, _, key := evaluate(result{AllocsOp: 0, NsPerOp: 10e6 + 1}, testBud, true)
	if v != "FAIL" || key != "tick_ms_max" {
		t.Fatalf("just-over time budget: got (%s,%s), want (FAIL,tick_ms_max)", v, key)
	}
}

func TestEvaluateTimeBoundaryFSV(t *testing.T) {
	// ns/op exactly at the budget (10e6 ns == 10 ms) is within budget.
	v, _, _ := evaluate(result{AllocsOp: 0, NsPerOp: 10e6}, testBud, true)
	if v != "OK" {
		t.Fatalf("ns/op exactly at budget must be OK (≤), got %s", v)
	}
}

func TestEvaluateAllocsTakePrecedenceFSV(t *testing.T) {
	// Both budgets breached → the verdict is attributed to allocs (switch order),
	// so the operator sees the alloc regression first.
	v, _, key := evaluate(result{AllocsOp: 5, NsPerOp: 50e6}, testBud, true)
	if v != "FAIL" || key != "allocs_per_tick" {
		t.Fatalf("both breached: got (%s,%s), want (FAIL,allocs_per_tick)", v, key)
	}
}

func TestEvaluateNonPerTickNeverGatedFSV(t *testing.T) {
	// A trend-only metric is never failed, no matter how large.
	v, gate, key := evaluate(result{AllocsOp: 9999, NsPerOp: 1e12}, testBud, false)
	if v != "OK" || gate != "trend-only" || key != "" {
		t.Fatalf("non-perTick metric gated: got (%s,%s,%s), want (OK,trend-only,\"\")", v, gate, key)
	}
}

func TestParseBudgetsRealFileFSV(t *testing.T) {
	b, err := parseBudgets("../../benchmarks/budgets.toml")
	if err != nil {
		t.Fatalf("parse real budgets.toml: %v", err)
	}
	if b.tickMsMax != 10 || b.allocsPerTick != 0 || b.binaryAssetsBytesMax != 314572800 {
		t.Fatalf("real budgets = %+v, want tick=10 allocs=0 bytes=314572800", b)
	}
	t.Logf("FSV: real budgets tick_ms_max=%d allocs_per_tick=%d binary_assets_bytes_max=%d",
		b.tickMsMax, b.allocsPerTick, b.binaryAssetsBytesMax)
}

func TestParseBudgetsFailClosedFSV(t *testing.T) {
	dir := t.TempDir()
	write := func(name, body string) string {
		p := filepath.Join(dir, name)
		if err := os.WriteFile(p, []byte(body), 0o644); err != nil {
			t.Fatal(err)
		}
		return p
	}
	cases := []struct{ name, body string }{
		{"unknown key", "[budgets]\ntick_ms_max = 10\nallocs_per_tick = 0\nbogus = 5\n"},
		{"non-integer", "[budgets]\ntick_ms_max = ten\nallocs_per_tick = 0\n"},
		{"missing required", "[budgets]\ntick_ms_max = 10\n"}, // no allocs_per_tick
		{"duplicate key", "[budgets]\ntick_ms_max = 10\ntick_ms_max = 11\nallocs_per_tick = 0\n"},
		{"key outside table", "tick_ms_max = 10\n"},
		{"unknown table", "[other]\nx = 1\n"},
	}
	for _, c := range cases {
		if _, err := parseBudgets(write("b.toml", c.body)); err == nil {
			t.Errorf("%s: parseBudgets must error (fail-closed), got nil", c.name)
		} else {
			t.Logf("FSV fail-closed [%s]: rejected — %v", c.name, err)
		}
	}
}

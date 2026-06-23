package litd_test

// #237 RAM-ceiling SIM CORE (headless). The issue's full deliverable is a windowed
// RSS-through-a-match gate ≤ 1.5 GB sampled ≥1 Hz — that needs the render path and
// the match-flow-to-victory-screen, both gated. This proves the part that is
// honestly headless and is in fact the issue's highest-value regression signal:
// across a full match AND across match teardown, the sim's live heap must not
// grow (R-SIM-3 / R-GC-2 — capacities are fixed at map load, so post-load growth
// is a defect). It is distinct from the per-tick zero-alloc gate (TestZeroAlloc),
// which cannot catch a slow leak in match setup/teardown or a retained world.
//
// SoT = runtime.MemStats.HeapAlloc after a forced GC (live heap), sampled after
// each of N back-to-back matches. A per-match teardown leak (e.g. a retained
// world or an accreting global registry) shows as live heap climbing ~linearly
// with match count; a clean teardown shows flat-within-noise. The leak-injection
// subtest is the positive control: it deliberately retains each world and asserts
// the SAME drift metric goes red — so the gate is proven non-vacuous.

import (
	"runtime"
	"testing"

	litd "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
)

// liveHeap returns bytes of reachable heap after settling the GC. Two GCs: the
// first runs finalizers/sweeps, the second collects anything they freed.
func liveHeap() uint64 {
	runtime.GC()
	runtime.GC()
	var ms runtime.MemStats
	runtime.ReadMemStats(&ms)
	return ms.HeapAlloc
}

func TestMatchMemoryNoTeardownLeakFSV(t *testing.T) {
	if testing.Short() {
		t.Skip("runs several full matches; -short skips")
	}
	const matches = 6
	base := liveHeap()
	samples := make([]uint64, matches)
	for i := 0; i < matches; i++ {
		// A full fought-to-elimination match; its world is local to playMatch and
		// becomes unreachable when it returns, so a clean engine collects it.
		playMatch(t, 20000, 5, 3)
		samples[i] = liveHeap()
	}
	for i, s := range samples {
		t.Logf("FSV #237 timeline: after match %d → live heap %d KiB (Δ base %+d KiB)",
			i+1, s/1024, (int64(s)-int64(base))/1024)
	}

	// Drift across the back-to-back matches: a retained world per match would make
	// this grow ~linearly. Clean teardown → flat within GC noise.
	drift := int64(samples[matches-1]) - int64(samples[0])
	t.Logf("FSV #237: live-heap drift match1→match%d = %+d KiB", matches, drift/1024)
	const leakBudget = 1 << 20 // 1 MiB across 6 matches — far under one world's footprint
	if drift > leakBudget {
		t.Fatalf("live heap grew %+d KiB across %d matches — teardown leak (budget %d KiB)",
			drift/1024, matches, leakBudget/1024)
	}
}

// TestMatchMemoryLeakInjectionFSV is the positive control: deliberately retain
// every match's world. The same drift metric MUST now exceed the budget — proving
// the no-leak test above is not vacuously green.
func TestMatchMemoryLeakInjectionFSV(t *testing.T) {
	if testing.Short() {
		t.Skip("runs several full matches; -short skips")
	}
	const matches = 6
	leaked := make([]*sim.World, 0, matches) // the injected leak: nothing is freed
	samples := make([]uint64, matches)
	for i := 0; i < matches; i++ {
		w, _, p1, p2 := setupMatch(t, 5, 3)
		for tick := 1; tick <= 20000; tick++ {
			w.Step()
			if p1.Result() != litd.ResultPlaying || p2.Result() != litd.ResultPlaying {
				break
			}
		}
		leaked = append(leaked, w) // retain → simulate a teardown leak
		samples[i] = liveHeap()
	}
	drift := int64(samples[matches-1]) - int64(samples[0])
	t.Logf("FSV #237 leak-injection: retained %d worlds, live-heap drift = %+d KiB", len(leaked), drift/1024)
	const leakBudget = 1 << 20
	if drift <= leakBudget {
		t.Fatalf("retaining %d worlds did NOT grow live heap past %d KiB (drift %+d KiB) — the leak gate is vacuous",
			matches, leakBudget/1024, drift/1024)
	}
	runtime.KeepAlive(leaked)
}

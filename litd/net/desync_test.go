package net

// #77 FSV: desync detection over REAL state hashes. The test drives the actual
// litd/statehash Registry/Hasher (the same machinery the sim uses) with a known
// single-system perturbation, so SoT = the DesyncEvent payload + the on-disk
// dump files, cross-checked against the canonical Registry.FirstDivergence. No
// mock hashing — only the perturbation is synthetic (X+X=Y: perturb "combat" →
// detector must name "combat").

import (
	"encoding/json"
	"os"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/statehash"
)

var desyncSystems = []string{"movement", "combat", "fog"}

// snapshot builds a real Registry over desyncSystems, writes per-system u64
// values, and returns the registry + snapshot.
func snapshot(move, combat, fog uint64) (*statehash.Registry, *statehash.Snapshot) {
	reg := statehash.NewRegistry()
	mh := reg.Register("movement")
	ch := reg.Register("combat")
	fh := reg.Register("fog")
	mh.WriteU64(move)
	ch.WriteU64(combat)
	fh.WriteU64(fog)
	var snap statehash.Snapshot
	reg.Sum(&snap)
	return reg, &snap
}

func report(turn uint64, snap *statehash.Snapshot) HashReport {
	return HashReport{Turn: turn, Top: snap.Top, Subs: append([]uint64(nil), snap.Subs...)}
}

func TestDesyncDetectSingleSystem(t *testing.T) {
	dir := t.TempDir()
	det, err := NewDesyncDetector(desyncSystems, []uint8{0, 1}, dir)
	if err != nil {
		t.Fatalf("NewDesyncDetector: %v", err)
	}

	// Client 0: baseline. Client 1: identical EXCEPT the combat system.
	reg0, snap0 := snapshot(100, 200, 300)
	_, snap1 := snapshot(100, 999, 300) // only "combat" perturbed

	// Sanity: tops actually differ, and the canonical bisection names combat.
	if snap0.Top == snap1.Top {
		t.Fatal("perturbed snapshot has equal top hash — test setup broken")
	}
	if sys, ok := reg0.FirstDivergence(snap0, snap1); !ok || sys != "combat" {
		t.Fatalf("canonical FirstDivergence = %q,%v, want combat", sys, ok)
	}

	// First client report defers; second triggers the event.
	if ev, err := det.Report(0, report(7, snap0)); err != nil || ev != nil {
		t.Fatalf("first report: ev=%v err=%v (want deferred)", ev, err)
	}
	ev, err := det.Report(1, report(7, snap1))
	if err != nil {
		t.Fatalf("second report: %v", err)
	}
	if ev == nil {
		t.Fatal("top-hash split produced no desync event")
	}
	if ev.DivergingSystem != "combat" {
		t.Fatalf("named diverging system %q, want combat", ev.DivergingSystem)
	}
	if ev.Tops[0] == ev.Tops[1] {
		t.Fatalf("event tops equal (%#x), expected split", ev.Tops[0])
	}
	t.Logf("FSV desync event: tick=%d ref=%d system=%q tops={0:%#x 1:%#x}", ev.Turn, ev.ReferenceClient, ev.DivergingSystem, ev.Tops[0], ev.Tops[1])

	// Open both dump files: sub-hash tables must differ ONLY in "combat".
	d0 := readDump(t, ev.DumpPaths[0])
	d1 := readDump(t, ev.DumpPaths[1])
	for i, name := range desyncSystems {
		h0, h1 := d0.Systems[i].Hash, d1.Systems[i].Hash
		if name == "combat" {
			if h0 == h1 {
				t.Fatalf("combat sub-hash equal in dumps (%#x); expected difference", h0)
			}
			t.Logf("FSV dump diff: %s 0=%#x 1=%#x (DIVERGES)", name, h0, h1)
		} else {
			if h0 != h1 {
				t.Fatalf("system %s differs in dumps (0=%#x 1=%#x) but only combat was perturbed", name, h0, h1)
			}
			t.Logf("FSV dump same: %s = %#x", name, h0)
		}
	}
}

func TestDesyncNoFalsePositive100Turns(t *testing.T) {
	det, _ := NewDesyncDetector(desyncSystems, []uint8{0, 1}, t.TempDir())
	for turn := uint64(0); turn < 100; turn++ {
		// Both clients identical each turn (vary by turn so it's not trivially constant).
		_, s0 := snapshot(turn, turn*2, turn*3)
		_, s1 := snapshot(turn, turn*2, turn*3)
		if ev, err := det.Report(0, report(turn, s0)); err != nil || ev != nil {
			t.Fatalf("turn %d report0: ev=%v err=%v", turn, ev, err)
		}
		if ev, err := det.Report(1, report(turn, s1)); err != nil {
			t.Fatalf("turn %d report1: %v", turn, err)
		} else if ev != nil {
			t.Fatalf("turn %d raised a FALSE desync: %+v", turn, ev)
		}
	}
	if det.Comparisons() != 100 {
		t.Fatalf("comparisons=%d, want 100", det.Comparisons())
	}
	if len(det.PendingTurns()) != 0 {
		t.Fatalf("pending turns after all agreed: %v", det.PendingTurns())
	}
	t.Logf("FSV no-false-positive: 100 turns compared, 0 events, 0 pending")
}

func TestDesyncLateReportDeferred(t *testing.T) {
	det, _ := NewDesyncDetector(desyncSystems, []uint8{0, 1, 2}, t.TempDir())
	_, s := snapshot(5, 5, 5)

	// Only clients 0 and 2 report turn 9; client 1's report is late.
	if ev, _ := det.Report(0, report(9, s)); ev != nil {
		t.Fatal("event before all clients reported")
	}
	if ev, _ := det.Report(2, report(9, s)); ev != nil {
		t.Fatal("event with client 1 still missing")
	}
	pend := det.PendingTurns()
	if len(pend) != 1 || pend[0] != 9 {
		t.Fatalf("pending=%v, want [9] (comparison deferred, not a false desync)", pend)
	}
	if det.Comparisons() != 0 {
		t.Fatalf("comparisons=%d before late report, want 0", det.Comparisons())
	}
	t.Logf("FSV late report: before — pending=%v comparisons=%d (no false desync)", pend, det.Comparisons())

	// Late report arrives → comparison resolves (all equal → no event).
	if ev, err := det.Report(1, report(9, s)); err != nil || ev != nil {
		t.Fatalf("late report resolution: ev=%v err=%v", ev, err)
	}
	if len(det.PendingTurns()) != 0 || det.Comparisons() != 1 {
		t.Fatalf("after late report: pending=%v comparisons=%d, want [] and 1", det.PendingTurns(), det.Comparisons())
	}
	t.Logf("FSV late report: after — pending=%v comparisons=%d (resolved cleanly)", det.PendingTurns(), det.Comparisons())
}

func readDump(t *testing.T, path string) dumpFile {
	t.Helper()
	blob, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("read dump %s: %v", path, err)
	}
	var df dumpFile
	if err := json.Unmarshal(blob, &df); err != nil {
		t.Fatalf("parse dump %s: %v", path, err)
	}
	if len(df.Systems) != len(desyncSystems) {
		t.Fatalf("dump %s has %d systems, want %d", path, len(df.Systems), len(desyncSystems))
	}
	return df
}

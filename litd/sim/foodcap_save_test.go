package sim

// Regression for the foodCap save/load bug (#652): foodCap is authoritative
// mutable state — a SetFoodCap base PLUS AddEcon building contributions — not a
// value purely derived from econ rows. The prior load zeroed foodCap and
// recomputed it from econ FoodProvided alone, silently dropping any SetFoodCap
// base across a round-trip. This fails before the v49 fix (load foodCap == the
// building portion only) and passes after (base + building).

import (
	"bytes"
	"testing"
)

func TestFoodCapBaseSurvivesSaveLoadFSV(t *testing.T) {
	const base = 12          // a SetFoodCap base (e.g. a melee starting supply cap)
	const provided = 10      // a food-providing building (town hall)
	const wantCap = base + provided

	src := econWorld(t) // BindEconomy(2)
	// A SetFoodCap base that is NOT backed by any econ row.
	src.SetFoodCap(0, base)
	// A building that adds food via AddEcon (foodProvided=10).
	addDepot(t, src, pt2(100, 100), provided)

	if got := src.FoodCap(0); got != wantCap {
		t.Fatalf("precondition: live FoodCap=%d, want %d (base %d + provided %d)", got, wantCap, base, provided)
	}
	t.Logf("FSV before save: FoodCap(0)=%d (base %d + building %d)", src.FoodCap(0), base, provided)

	var buf bytes.Buffer
	if err := src.SaveState(&buf, 0); err != nil {
		t.Fatalf("SaveState: %v", err)
	}

	// Restore into a FRESH world that has the SAME econ binding but no SetFoodCap
	// base applied — exactly the firstclash restore shape, where the base came
	// from a script that the load must reconstruct from the container, not re-run.
	dst := econWorld(t)
	if err := dst.LoadState(bytes.NewReader(buf.Bytes()), 0); err != nil {
		t.Fatalf("LoadState: %v", err)
	}
	got := dst.FoodCap(0)
	t.Logf("FSV after load: FoodCap(0)=%d (want %d)", got, wantCap)
	if got != wantCap {
		t.Fatalf("foodCap base lost across save/load: got %d, want %d (the SetFoodCap base %d was dropped)", got, wantCap, base)
	}
}

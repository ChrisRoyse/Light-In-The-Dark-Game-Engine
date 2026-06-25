package worldhost_test

// Regression for the windowed tick-rate control (#655, ultimate-test-plan Phase
// 6 G6'). The cmd/game -speed/-maxspeed flags change ONLY how many deterministic
// Advance(1) calls happen per render frame; they must not change the sim outcome.
// SoT = StateHash after the same number of total ticks reached via different
// per-frame batch sizes. Bug class guarded: "advancing N ticks per frame diverges
// from advancing 1 tick per frame" — a divergence here would mean the speed
// multiplier corrupts determinism (R-SIM-2). Headless/GL-free: the multiplier is
// pure Advance() arithmetic, no window needed.

import (
	"testing"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/worldhost"
)

// advanceBatched advances g to exactly `total` ticks in steps of `batch`,
// mirroring cmd/game's advanceSim(speed) called once per frame. total must be a
// multiple of batch so the runs land on the same tick.
func advanceBatched(g *api.Game, total, batch int) {
	for t := 0; t < total; t += batch {
		g.Advance(batch)
	}
}

func TestFirstclashSpeedInvariantHashFSV(t *testing.T) {
	if testing.Short() {
		t.Skip("loads firstclash 3x + advances 600 ticks each; full preflight gate")
	}
	const seed = 4242
	const total = 600 // divisible by every batch below so all land on tick 600

	type run struct {
		name  string
		batch int
	}
	runs := []run{
		{"speed1", 1},   // real-time baseline: 1 tick/frame
		{"speed8", 8},   // -speed 8
		{"speed50", 50}, // -maxspeed -speed 50
	}

	var base uint64
	for i, r := range runs {
		if total%r.batch != 0 {
			t.Fatalf("%s: total %d not divisible by batch %d — runs would not align", r.name, total, r.batch)
		}
		h, err := worldhost.Load(firstclashDir, seed, 50_000_000)
		if err != nil {
			t.Fatalf("%s: load: %v", r.name, err)
		}
		before := h.Game.StateHash()
		advanceBatched(h.Game, total, r.batch)
		got := h.Game.StateHash()
		h.Close()
		t.Logf("FSV %-8s before=%#016x after(tick %d, batch %d)=%#016x", r.name, before, total, r.batch, got)
		if i == 0 {
			base = got
			continue
		}
		if got != base {
			t.Fatalf("%s: hash %#016x != speed1 baseline %#016x — tick-batch size changed the sim outcome (determinism broken)", r.name, got, base)
		}
	}
	t.Logf("FSV #655: StateHash after 600 ticks identical for batch sizes 1/8/50 (%#016x) — -speed/-maxspeed do not affect determinism", base)
}

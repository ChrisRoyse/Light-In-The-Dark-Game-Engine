package main

// #251 desync-bisect FSV. SoT = the Divergence the tool computes from two
// checkpoint traces with KNOWN, hand-injected divergences (synthetic
// known-input/known-output), plus a real Encode→DecodeReplay round-trip so the
// result is proven on the actual serialized file format, not just in-memory.

import (
	"bytes"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
)

func sysIndex(t *testing.T, name string) int {
	t.Helper()
	for i, n := range sim.HashSystems {
		if n == name {
			return i
		}
	}
	t.Fatalf("system %q not in HashSystems %v", name, sim.HashSystems)
	return -1
}

// subVec returns a zeroed sub-hash vector of the correct width, then applies
// (index->value) overrides.
func subVec(overrides map[int]uint64) []uint64 {
	v := make([]uint64, len(sim.HashSystems))
	for i, val := range overrides {
		v[i] = val
	}
	return v
}

// topOf folds the subs into a top hash so top differs iff the subs differ —
// matching the real invariant (top is a hash over the per-system subs).
func topOf(subs []uint64) uint64 {
	var x uint64 = 0x9e3779b97f4a7c15
	for i, s := range subs {
		x ^= s + uint64(i)*0x100000001b3
		x = x<<7 | x>>57
	}
	return x
}

func cp(tick uint32, subs []uint64) sim.ReplayCheckpoint {
	return sim.ReplayCheckpoint{Tick: tick, Top: topOf(subs), Subs: subs}
}

// baseReplay builds a comparable replay (matching headers) with the given
// checkpoints.
func baseReplay(cps ...sim.ReplayCheckpoint) *sim.Replay {
	return &sim.Replay{
		Version:     sim.ReplayFormatVersion,
		Seed:        42,
		Roster:      64,
		Interval:    sim.DefaultCheckpointInterval,
		Ticks:       500,
		Checkpoints: cps,
	}
}

// TestBisectSingleSystemDivergenceFSV — A and B identical except the combat
// sub-hash at tick 500; the tool must name tick 500 + combat.
func TestBisectSingleSystemDivergenceFSV(t *testing.T) {
	combat := sysIndex(t, "combat")
	a := baseReplay(
		cp(100, subVec(nil)),
		cp(200, subVec(nil)),
		cp(500, subVec(map[int]uint64{combat: 0xAAAA})),
	)
	b := baseReplay(
		cp(100, subVec(nil)),
		cp(200, subVec(nil)),
		cp(500, subVec(map[int]uint64{combat: 0xBBBB})), // <- only difference
	)

	div, err := Bisect(a, b)
	if err != nil {
		t.Fatalf("Bisect: %v", err)
	}
	t.Logf("FSV divergence: found=%v tick=%d system=%q subA=%x subB=%x", div.Found, div.Tick, div.System, div.SubA, div.SubB)
	if !div.Found || div.Tick != 500 || div.System != "combat" {
		t.Fatalf("want tick=500 system=combat, got found=%v tick=%d system=%q", div.Found, div.Tick, div.System)
	}
	if div.SubA != 0xAAAA || div.SubB != 0xBBBB {
		t.Fatalf("sub values wrong: A=%x B=%x", div.SubA, div.SubB)
	}
}

// TestBisectIdentical — identical traces report no divergence.
func TestBisectIdentical(t *testing.T) {
	a := baseReplay(cp(100, subVec(nil)), cp(200, subVec(map[int]uint64{0: 7})))
	b := baseReplay(cp(100, subVec(nil)), cp(200, subVec(map[int]uint64{0: 7})))
	div, err := Bisect(a, b)
	if err != nil {
		t.Fatal(err)
	}
	t.Logf("FSV identical: found=%v", div.Found)
	if div.Found {
		t.Fatalf("identical traces should not diverge: %+v", div)
	}
}

// TestBisectDivergenceAtTick0 — earliest checkpoint divergence is caught.
func TestBisectDivergenceAtTick0(t *testing.T) {
	owners := sysIndex(t, "owners")
	a := baseReplay(cp(0, subVec(map[int]uint64{owners: 1})), cp(100, subVec(nil)))
	b := baseReplay(cp(0, subVec(map[int]uint64{owners: 2})), cp(100, subVec(nil)))
	div, err := Bisect(a, b)
	if err != nil {
		t.Fatal(err)
	}
	t.Logf("FSV tick0: found=%v tick=%d system=%q", div.Found, div.Tick, div.System)
	if !div.Found || div.Tick != 0 || div.System != "owners" {
		t.Fatalf("want tick=0 owners, got found=%v tick=%d system=%q", div.Found, div.Tick, div.System)
	}
}

// TestBisectTwoSystemsSameTick — when two systems diverge on one tick, the
// lower HashSystems index is reported (deterministic).
func TestBisectTwoSystemsSameTick(t *testing.T) {
	movement := sysIndex(t, "movement")
	combat := sysIndex(t, "combat")
	if movement >= combat {
		t.Fatalf("test assumes movement(%d) before combat(%d) in HashSystems order", movement, combat)
	}
	a := baseReplay(cp(300, subVec(map[int]uint64{movement: 1, combat: 1})))
	b := baseReplay(cp(300, subVec(map[int]uint64{movement: 9, combat: 9})))
	div, err := Bisect(a, b)
	if err != nil {
		t.Fatal(err)
	}
	t.Logf("FSV two-system: tick=%d system=%q (want the earlier system, movement)", div.Tick, div.System)
	if !div.Found || div.Tick != 300 || div.System != "movement" {
		t.Fatalf("want first-by-order movement@300, got %q@%d", div.System, div.Tick)
	}
}

// TestBisectHeaderMismatchRefused — replays of different inputs are not a
// desync; the tool refuses fail-closed rather than reporting a false culprit.
func TestBisectHeaderMismatchRefused(t *testing.T) {
	a := baseReplay(cp(100, subVec(nil)))
	b := baseReplay(cp(100, subVec(nil)))
	b.Seed = 999
	_, err := Bisect(a, b)
	t.Logf("FSV header-mismatch err: %v", err)
	if err == nil {
		t.Fatal("different seeds must be refused, not bisected")
	}
}

// TestBisectRealFileRoundTrip — proves the result holds on the ACTUAL
// serialized .litdreplay format (Encode → DecodeReplay → Bisect).
func TestBisectRealFileRoundTrip(t *testing.T) {
	sched := sysIndex(t, "sched")
	a := baseReplay(cp(100, subVec(nil)), cp(200, subVec(nil)), cp(300, subVec(map[int]uint64{sched: 0xDEAD})))
	b := baseReplay(cp(100, subVec(nil)), cp(200, subVec(nil)), cp(300, subVec(map[int]uint64{sched: 0xFEED})))

	var ba, bb bytes.Buffer
	if err := a.Encode(&ba); err != nil {
		t.Fatal(err)
	}
	if err := b.Encode(&bb); err != nil {
		t.Fatal(err)
	}
	da, err := sim.DecodeReplay(&ba)
	if err != nil {
		t.Fatalf("decode A: %v", err)
	}
	db, err := sim.DecodeReplay(&bb)
	if err != nil {
		t.Fatalf("decode B: %v", err)
	}
	div, err := Bisect(da, db)
	if err != nil {
		t.Fatal(err)
	}
	t.Logf("FSV roundtrip: tick=%d system=%q subA=%x subB=%x", div.Tick, div.System, div.SubA, div.SubB)
	if !div.Found || div.Tick != 300 || div.System != "sched" || div.SubA != 0xDEAD || div.SubB != 0xFEED {
		t.Fatalf("roundtrip wrong: %+v", div)
	}
}

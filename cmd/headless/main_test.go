package main

// #210 (G5.3) end-to-end determinism thesis, automated and headless: record a
// command stream through a real sim run, serialize it to the .litdreplay format,
// decode it back, REPLAY it on a fresh world, and prove the final Game.StateHash
// and every checkpoint reproduce bit-for-bit. The manual CLI (-replay/-verify)
// already does this by hand; this locks it in the `go test` gate so a determinism
// regression fails the build, not a human's eyeball. The harness functions
// (buildWorld/runWorld/applyReplayCommand) are the SAME ones the CLI uses — no
// parallel re-implementation that could drift (the record and verify paths must
// apply a command identically or the test would be meaningless).
//
// SoT = the recomputed state hash + per-system checkpoint sub-hashes after each
// run, compared across the record run, the replay run, and a deliberately
// perturbed run.

import (
	"bytes"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/statehash"
)

// finalTopHash recomputes the world's top state hash (the SoT read, separate from
// any return value the run produced).
func finalTopHash(w *sim.World) uint64 {
	reg := sim.NewHashRegistry()
	var snap statehash.Snapshot
	w.HashState(reg, &snap)
	return snap.Top
}

// syntheticStream is a deterministic, boundary-varied command stream: every order
// kind that takes a point/target, ascending ticks (DecodeReplay requires it),
// unit indices inside the roster. Known input → known (recorded) output.
func syntheticStream() []sim.ReplayCommand {
	return []sim.ReplayCommand{
		{Tick: 5, Player: 0, Kind: sim.ReplayMove, Unit: 0, Target: sim.NoRosterRef, X: 100 << 32, Y: 40 << 32},
		{Tick: 5, Player: 0, Kind: sim.ReplayMove, Unit: 1, Target: sim.NoRosterRef, X: 60 << 32, Y: 200 << 32},
		{Tick: 12, Player: 0, Kind: sim.ReplayPatrol, Unit: 2, Target: sim.NoRosterRef, X: 300 << 32, Y: 300 << 32},
		{Tick: 30, Player: 0, Kind: sim.ReplayAttack, Unit: 3, Target: sim.NoRosterRef, X: 150 << 32, Y: 150 << 32},
		{Tick: 48, Player: 0, Kind: sim.ReplayMove, Unit: 4, Target: sim.NoRosterRef, X: 10 << 32, Y: 10 << 32},
		{Tick: 120, Player: 0, Kind: sim.ReplayStop, Unit: 0, Target: sim.NoRosterRef},
		{Tick: 240, Player: 0, Kind: sim.ReplayHold, Unit: 1, Target: sim.NoRosterRef},
	}
}

// traceTopAt returns the recorded top hashes keyed by tick, for divergence search.
func cpsByTick(cps []sim.ReplayCheckpoint) map[uint32]sim.ReplayCheckpoint {
	m := make(map[uint32]sim.ReplayCheckpoint, len(cps))
	for _, cp := range cps {
		m[cp.Tick] = cp
	}
	return m
}

func TestRecordReplayDeterminismFSV(t *testing.T) {
	const (
		seed  = uint64(7)
		units = 64
		ticks = 600
	)
	cmds := syntheticStream()

	// RECORD: a real run, capturing the checkpoint trace and the final hash.
	w1, ids1 := buildWorld(seed, units)
	cps1 := runWorld(w1, ids1, cmds, ticks, true, 0, nil)
	h1 := finalTopHash(w1)
	if len(cps1) == 0 {
		t.Fatal("record run produced no checkpoints")
	}

	// SERIALIZE → DESERIALIZE through the on-disk format (round-trips the bytes).
	rec := &sim.Replay{
		Version: sim.ReplayFormatVersion, Seed: seed, Roster: units,
		Interval: sim.DefaultCheckpointInterval, Ticks: ticks,
		Commands: cmds, Checkpoints: cps1,
	}
	var buf bytes.Buffer
	if err := rec.Encode(&buf); err != nil {
		t.Fatalf("encode: %v", err)
	}
	dec, err := sim.DecodeReplay(bytes.NewReader(buf.Bytes()))
	if err != nil {
		t.Fatalf("decode: %v", err)
	}

	// REPLAY: a FRESH world, same seed, the DECODED command stream. The determinism
	// thesis: identical inputs from identical initial state ⇒ identical final hash
	// and identical per-checkpoint sub-hashes.
	w2, ids2 := buildWorld(dec.Seed, int(dec.Roster))
	cps2 := runWorld(w2, ids2, dec.Commands, int(dec.Ticks), true, 0, nil)
	h2 := finalTopHash(w2)

	if h1 != h2 {
		t.Fatalf("replay diverged from record: recorded final %016x, replayed %016x", h1, h2)
	}
	if len(cps1) != len(cps2) {
		t.Fatalf("checkpoint count: recorded %d, replayed %d", len(cps1), len(cps2))
	}
	for i := range cps1 {
		snap := &statehash.Snapshot{Top: cps2[i].Top, Subs: cps2[i].Subs}
		if culprit, match := sim.CompareCheckpoint(&cps1[i], snap); !match {
			t.Fatalf("checkpoint %d (tick %d) diverged at system %q on replay", i, cps1[i].Tick, culprit)
		}
	}
	t.Logf("FSV #210: record→encode→decode→replay reproduced final hash %016x across %d checkpoints (%d bytes, %d commands)",
		h1, len(cps1), buf.Len(), len(dec.Commands))

	// INDUCED-FAULT edge (teeth): perturb ONE command's destination by one world
	// unit. A real simulation-outcome change MUST move the final hash AND be caught
	// by the checkpoint trace, naming the first divergent tick + culprit system —
	// proving the verifier is a real detector, not a test that cannot fail.
	bad := append([]sim.ReplayCommand(nil), cmds...)
	bad[0].X += 1 << 32
	w3, ids3 := buildWorld(seed, units)
	cps3 := runWorld(w3, ids3, bad, ticks, true, 0, nil)
	h3 := finalTopHash(w3)
	if h3 == h1 {
		t.Fatal("induced fault did not change the final hash — gate is blind to a real divergence")
	}
	rec3 := cpsByTick(cps3)
	firstDivTick, culpritSys := uint32(0), ""
	for i := range cps1 {
		cp3, ok := rec3[cps1[i].Tick]
		if !ok {
			continue
		}
		snap := &statehash.Snapshot{Top: cp3.Top, Subs: cp3.Subs}
		if culprit, match := sim.CompareCheckpoint(&cps1[i], snap); !match {
			firstDivTick, culpritSys = cps1[i].Tick, culprit
			break
		}
	}
	if firstDivTick == 0 {
		t.Fatal("induced fault moved the final hash but no checkpoint flagged it — bisection is blind")
	}
	t.Logf("FSV #210 induced-fault: perturbed cmd → final %016x != %016x; first divergent checkpoint tick %d, culprit system %q",
		h3, h1, firstDivTick, culpritSys)
}

// TestRecordReplayMatchesStandaloneHashFSV proves the recorded final hash is the
// real Game.StateHash a caller would observe — not a private accounting number.
func TestRecordReplayMatchesStandaloneHashFSV(t *testing.T) {
	const (
		seed  = uint64(7)
		units = 64
		ticks = 300
	)
	cmds := syntheticStream()
	w1, ids1 := buildWorld(seed, units)
	cps := runWorld(w1, ids1, cmds, ticks, true, 0, nil)
	// The last checkpoint is captured at the final tick; its top must equal a
	// fresh recompute of the world hash — the trace is honest about end state.
	last := cps[len(cps)-1]
	if last.Tick != ticks {
		t.Fatalf("last checkpoint at tick %d, want %d", last.Tick, ticks)
	}
	if got := finalTopHash(w1); got != last.Top {
		t.Fatalf("final checkpoint top %016x != recomputed world hash %016x", last.Top, got)
	}
	t.Logf("FSV #210: final checkpoint top %016x == recomputed Game state hash (trace is honest)", last.Top)
}

package ai_test

// #404 — production .litdreplay recording of a real melee-AI match. This
// promotes #398 (which proved the mechanism with an ad-hoc per-test command
// struct) into the REAL replay format: melee.RecordingBridge taps every
// state-changing bridge call into sim.ReplayCommand v3 (the player-level economy
// kinds + unit-order moves), the stream is serialized to .litdreplay bytes and
// decoded back, and a fresh identical world re-applies it through
// sim.ReplayCommand.Apply with NO controllers — reaching a bit-identical sim
// state.
//
// SoT = statehash .Top of the world after the run (the match outcome): live
// (AI on, recording) vs replay (AI off, stream applied). Recording is also
// proven non-intrusive: the recorded run's hash equals a plain run's.

import (
	"bytes"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/ai"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/ai/melee"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/statehash"
)

func TestAIMatchRecordsProductionReplayFSV(t *testing.T) {
	if testing.Short() {
		t.Skip("production record→replay reproduction skipped in -short (runs in the full gate)")
	}
	const ticks = 2000
	reg := sim.NewHashRegistry()
	vigil, unbound := dLoadFactions(t)

	// 1. Record a live AI match into the production replay format. The recording
	//    bridge wraps each player's detBridge; the inner bridge is what the AI
	//    domain reads through (recording taps only the state-changing calls).
	var cmds []sim.ReplayCommand
	wRec := dWorld(t)
	dSetupUnits(t, wRec, 0)
	in1, in2 := &detBridge{w: wRec, player: 1}, &detBridge{w: wRec, player: 2}
	br1 := melee.NewRecordingBridge(in1, &cmds)
	br2 := melee.NewRecordingBridge(in2, &cmds)
	c1 := melee.NewController(vigil, dCfg(1, 1500, 1500, 3500, 1500), br1)
	c2 := melee.NewController(unbound, dCfg(2, 3500, 1500, 1500, 1500), br2)
	dom := ai.NewDomain()
	dom.SetDiagnostics(nil)
	dom.AddPlayer(1, in1, in1, ai.NewFuncController(c1.Step))
	dom.AddPlayer(2, in2, in2, ai.NewFuncController(c2.Step))
	wRec.OnAIPhase = func(uint32) { dom.Tick(0) }
	dStep(dMatch{w: wRec, dom: dom, c1: c1, c2: c2}, ticks)
	var snapLive statehash.Snapshot
	wRec.HashState(reg, &snapLive)

	// 1a. Recording is non-intrusive: the recorded run's sim hash equals a plain
	//     (non-recording) run's. If this fails, the tap perturbed the sim.
	mPlain := dNewMatch(t, vigil, unbound, 0, false)
	dStep(mPlain, ticks)
	var snapPlain statehash.Snapshot
	mPlain.w.HashState(reg, &snapPlain)
	if snapLive.Top != snapPlain.Top {
		t.Fatalf("recording perturbed the live sim: recorded=%016x plain=%016x", snapLive.Top, snapPlain.Top)
	}
	if len(cmds) == 0 {
		t.Fatal("recorded zero commands — the production tap is not firing")
	}
	var kinds [sim.ReplayMaxKind + 1]int
	for _, c := range cmds {
		kinds[c.Kind]++
	}
	t.Logf("FSV #404: recorded %d production commands over %d ticks (move=%d train=%d harvestAssign=%d place=%d)",
		len(cmds), ticks, kinds[sim.ReplayMove], kinds[sim.ReplayTrain], kinds[sim.ReplayHarvestAssign], kinds[sim.ReplayPlaceBuilding])

	// 2. Serialize to .litdreplay bytes and decode back — proves the recorded
	//    stream is a VALID production replay (decode fails closed on any
	//    malformation, including non-monotonic ticks).
	rep := &sim.Replay{
		Version: sim.ReplayFormatVersion, Interval: sim.DefaultCheckpointInterval,
		Ticks: ticks, Commands: cmds,
	}
	var buf bytes.Buffer
	if err := rep.Encode(&buf); err != nil {
		t.Fatalf("encode .litdreplay: %v", err)
	}
	dec, err := sim.DecodeReplay(bytes.NewReader(buf.Bytes()))
	if err != nil {
		t.Fatalf("decode .litdreplay: %v", err)
	}
	if len(dec.Commands) != len(cmds) {
		t.Fatalf("decoded %d commands, want %d", len(dec.Commands), len(cmds))
	}
	t.Logf("FSV #404: %d commands round-trip through %d .litdreplay bytes", len(dec.Commands), buf.Len())

	// 3. Replay the decoded stream with NO controllers: a fresh identical world
	//    applies each command at its recorded tick via ReplayCommand.Apply.
	wRep := dWorld(t)
	dSetupUnits(t, wRep, 0)
	resolve := melee.EntityResolver(wRep)
	next := 0
	wRep.OnAIPhase = func(uint32) {
		tk := wRep.Tick()
		for next < len(dec.Commands) && dec.Commands[next].Tick == tk {
			dec.Commands[next].Apply(wRep, resolve)
			next++
		}
	}
	for i := 0; i < ticks; i++ {
		wRep.RecomputeVisibility()
		wRep.Step()
	}
	var snapRep statehash.Snapshot
	wRep.HashState(reg, &snapRep)

	// 4. SoT: the AI-disabled production replay reproduces the live match exactly.
	t.Logf("FSV #404: live=%016x replay=%016x applied=%d/%d", snapLive.Top, snapRep.Top, next, len(dec.Commands))
	if next != len(dec.Commands) {
		t.Fatalf("replay consumed %d of %d commands — tick addressing mismatch", next, len(dec.Commands))
	}
	if snapRep.Top != snapLive.Top {
		culprit, _ := sim.CompareCheckpoint(&sim.ReplayCheckpoint{Top: snapLive.Top, Subs: snapLive.Subs}, &snapRep)
		t.Fatalf("production replay diverged: replay=%016x live=%016x (first culprit system=%s)", snapRep.Top, snapLive.Top, culprit)
	}
	t.Log("production .litdreplay reproduces the live AI match with NO controllers running (#404)")
}

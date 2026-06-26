package litd

// #404 — live Game.RecordReplay end-to-end. The public api surface records a
// real melee-AI match into the production .litdreplay (Game.RecordReplay →
// AttachMeleeAI → Advance → BuildReplay), and a fresh identical game re-applies
// the decoded stream with NO AI attached, reproducing the match exactly.
//
// SoT = worldFingerprint (every entity's id/owner/type) + the AI player's
// footman count, read back from the sim after each run: live (AI on, recording)
// vs replay (AI off, stream applied) must be identical, and recording must not
// perturb the live sim (recorded fingerprint == a plain non-recording run's).

import (
	"bytes"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/ai/melee"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
)

func TestRecordReplayMeleeMatchFSV(t *testing.T) {
	const aiPlayer = uint8(1)
	const target = 3

	// 1. Live recorded match through the public surface.
	gRec := lvGame(t, aiPlayer)
	gRec.RecordReplay()
	gRec.AttachMeleeAI(gRec.Player(int(aiPlayer)), meleeArmyStrategy(target),
		melee.Config{GoldID: 0, WoodID: 1}, DifficultyNormal)
	gRec.Advance(300)
	liveFp := worldFingerprint(gRec)
	liveArmy := liveFootmen(gRec, aiPlayer)
	rep := gRec.BuildReplay()
	if len(rep.Commands) == 0 {
		t.Fatal("recorded zero commands — RecordReplay did not tap the live melee bridge")
	}

	// 1a. Non-intrusive: a plain (non-recording) run reaches the same fingerprint.
	gPlain := lvGame(t, aiPlayer)
	gPlain.AttachMeleeAI(gPlain.Player(int(aiPlayer)), meleeArmyStrategy(target),
		melee.Config{GoldID: 0, WoodID: 1}, DifficultyNormal)
	gPlain.Advance(300)
	if fp := worldFingerprint(gPlain); fp != liveFp {
		t.Fatalf("recording perturbed the live sim: recorded=%#x plain=%#x", liveFp, fp)
	}

	// 2. Serialize to .litdreplay and decode back (valid production replay).
	var buf bytes.Buffer
	if err := rep.Encode(&buf); err != nil {
		t.Fatalf("encode .litdreplay: %v", err)
	}
	dec, err := sim.DecodeReplay(bytes.NewReader(buf.Bytes()))
	if err != nil {
		t.Fatalf("decode .litdreplay: %v", err)
	}

	// 3. Replay on a fresh identical game with NO AI: apply each command at its
	//    recorded tick via ReplayCommand.Apply (entity-id resolver).
	gRep := lvGame(t, aiPlayer)
	resolve := melee.EntityResolver(gRep.w)
	next := 0
	gRep.w.OnAIPhase = func(uint32) {
		tk := gRep.w.Tick()
		for next < len(dec.Commands) && dec.Commands[next].Tick == tk {
			dec.Commands[next].Apply(gRep.w, resolve)
			next++
		}
	}
	gRep.Advance(300)

	// 4. SoT: identical fingerprint + army, every command consumed.
	replayFp := worldFingerprint(gRep)
	replayArmy := liveFootmen(gRep, aiPlayer)
	t.Logf("FSV #404 api: live fp=%#x footmen=%d | replay fp=%#x footmen=%d | cmds=%d applied=%d/%d",
		liveFp, liveArmy, replayFp, replayArmy, len(dec.Commands), next, len(dec.Commands))
	if next != len(dec.Commands) {
		t.Fatalf("replay consumed %d of %d commands — tick addressing mismatch", next, len(dec.Commands))
	}
	if replayFp != liveFp {
		t.Fatalf("api replay diverged: replay fingerprint %#x != live %#x", replayFp, liveFp)
	}
	if replayArmy != target {
		t.Fatalf("replay trained %d footmen, want %d (the recorded train commands did not reproduce)", replayArmy, target)
	}
	t.Logf("FSV PASS #404: Game.RecordReplay → .litdreplay reproduces the live melee match (%d footmen) with NO AI attached", target)
}

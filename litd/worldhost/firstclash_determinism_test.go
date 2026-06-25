package worldhost_test

// FSV for the determinism thesis on the SHIPPED world (#649, ultimate-test-plan
// Phase 4): a real firstclash AI match, recorded as a command stream, replays
// BIT-IDENTICAL with the controllers detached (commands-only model, R-SIM-4 /
// #404 scaled to firstclash). SoT = the per-checkpoint StateHash trace + the
// final StateHash of the live recorded run vs the no-controller replay. Zero
// tolerance: any single checkpoint mismatch fails.

import (
	"testing"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/worldhost"
)

const detSeed = 20260624
const detCap = 24050         // a hair past the 24,000-tick score-decide backstop
const detCheckpoint = 2000   // sample the StateHash every 2000 ticks

// stepTrace steps g until a player result latches (or cap), capturing the
// StateHash at every detCheckpoint tick. Returns (terminalTick, trace, final).
func stepTrace(g *api.Game, cap int) (int, map[int]uint64, uint64) {
	trace := map[int]uint64{}
	term := -1
	for int(g.Tick()) < cap {
		if int(g.Tick())%detCheckpoint == 0 {
			trace[int(g.Tick())] = g.StateHash()
		}
		g.Advance(1)
		if g.Player(0).Result() != api.ResultPlaying || g.Player(1).Result() != api.ResultPlaying {
			term = int(g.Tick())
			break
		}
	}
	trace[int(g.Tick())] = g.StateHash() // always capture the terminal tick
	return term, trace, g.StateHash()
}

func TestFirstclashRecordReplayDeterminismFSV(t *testing.T) {
	if testing.Short() {
		t.Skip("two 24,000-tick AI matches (~8s); runs in the full preflight gate")
	}

	// --- Live recorded run: AIs on, recording the command stream. ---
	hRec, err := worldhost.LoadRecording(firstclashDir, detSeed, 50_000_000)
	if err != nil {
		t.Fatalf("LoadRecording firstclash: %v", err)
	}
	termLive, traceLive, hashLive := stepTrace(hRec.Game, detCap)
	cmds := append([]sim.ReplayCommand(nil), hRec.Game.BuildReplay().Commands...)
	hRec.Close()
	t.Logf("FSV record: terminal@%d  finalHash=%#016x  commands=%d  checkpoints=%d", termLive, hashLive, len(cmds), len(traceLive))
	if termLive < 0 {
		t.Fatalf("recorded match did not terminate within %d ticks", detCap)
	}
	if len(cmds) == 0 {
		t.Fatal("recorded zero commands — the AI tap is not firing through worldhost")
	}

	// --- Non-intrusive check: a PLAIN run (AIs on, no recording tap) must reach
	//     the identical final hash — proves the recording bridge delegates
	//     unchanged and does not perturb the sim (#404 property, on firstclash). ---
	hPlain, err := worldhost.Load(firstclashDir, detSeed, 50_000_000)
	if err != nil {
		t.Fatalf("plain Load firstclash: %v", err)
	}
	_, _, hashPlain := stepTrace(hPlain.Game, detCap)
	hPlain.Close()
	if hashPlain != hashLive {
		t.Fatalf("recording perturbed the sim: plain=%#016x recorded=%#016x", hashPlain, hashLive)
	}
	t.Logf("FSV non-intrusive: plain run finalHash=%#016x == recorded (tap is transparent)", hashPlain)

	// --- Replay run: SAME world+seed, NO controllers, command stream applied. ---
	hRep, err := worldhost.LoadReplay(firstclashDir, detSeed, 50_000_000, cmds)
	if err != nil {
		t.Fatalf("LoadReplay firstclash: %v", err)
	}
	defer hRep.Close()
	termRep, traceRep, hashRep := stepTrace(hRep.Game, detCap)
	applied := hRep.Game.ReplayApplied()
	t.Logf("FSV replay: terminal@%d  finalHash=%#016x  applied=%d/%d", termRep, hashRep, applied, len(cmds))

	// 1. Every recorded command consumed at its tick → tick addressing lined up.
	if applied != len(cmds) {
		t.Fatalf("replay applied %d of %d commands — tick-addressing mismatch (dropped/extra)", applied, len(cmds))
	}
	// 2. Same terminal tick.
	if termRep != termLive {
		t.Fatalf("replay terminated at tick %d, live at %d", termRep, termLive)
	}
	// 3. Full checkpoint trace identical — zero tolerance.
	if len(traceRep) != len(traceLive) {
		t.Fatalf("checkpoint count differs: live=%d replay=%d", len(traceLive), len(traceRep))
	}
	for tick, hLive := range traceLive {
		hRepv, ok := traceRep[tick]
		if !ok {
			t.Fatalf("replay missing checkpoint @tick %d", tick)
		}
		if hRepv != hLive {
			t.Fatalf("DIVERGENCE @tick %d: live=%#016x replay=%#016x", tick, hLive, hRepv)
		}
	}
	// 4. Final hash bit-identical.
	if hashRep != hashLive {
		t.Fatalf("final StateHash diverged: live=%#016x replay=%#016x", hashLive, hashRep)
	}
	t.Logf("FSV #649: firstclash AI match reproduces bit-identically with NO controllers — %d checkpoints + final hash %#016x match across %d commands", len(traceLive), hashLive, len(cmds))
}

package luabind

// #204 regression — the cmd/game quicksave bug. cmd/game's F5/F9 used the
// SIM-ONLY save path (api.Game.SaveState/LoadState) and never persisted the Lua
// scheduler (SaveScripts/LoadScripts). Every catalog/melee world drives itself
// with a Game_Every periodic (the victory timeout, AI cadence), whose counter
// lives in a closure upvalue in the scheduler — NOT in sim state. So a sim-only
// save silently dropped it: the restored match did not resume bit-identically.
//
// This test pins that: the SIM-ONLY round-trip diverges from the unbroken run,
// while the FULL container (savegame.Write/Load == SaveState+SaveScripts) matches.
// The fix wires cmd/game to the full container; this guards against a regression
// back to sim-only. SoT = the Storage counter + StateHash after resume.

import (
	"bytes"
	"testing"
)

func TestSimOnlySaveDropsSchedulerFSV(t *testing.T) {
	const fp = uint64(0xC0DEF00D)

	// Unbroken reference: counter ticks every Advance, reaching 20 at tick 20.
	gRef, LRef, regRef := newScriptGame(t)
	defer LRef.Close()
	defer regRef.Close()
	runRegisteredChunk(t, LRef, regRef, everyProbeScript)
	gRef.Advance(20)
	refCount, _ := gRef.Storage().GetInt("probe", "n")
	refHash := gRef.StateHash()
	t.Logf("UNBROKEN @20: counter=%d hash=%#x", refCount, refHash)
	if refCount != 20 {
		t.Fatalf("unbroken counter=%d, want 20", refCount)
	}

	// Save at tick 10 (counter=10): capture BOTH the sim blob and the script blob.
	sim, scr, atSave := saveEveryProbeAt(t, 10)
	if atSave != 10 {
		t.Fatalf("counter at save = %d, want 10", atSave)
	}

	// --- SIM-ONLY restore (the old cmd/game path): re-run the entry, load only the
	//     sim. The scheduler blob is NOT applied — the periodic's counter upvalue
	//     stays at the fresh re-run value, so resume diverges from unbroken. ---
	gSim, LSim, regSim := newScriptGame(t)
	defer LSim.Close()
	defer regSim.Close()
	runRegisteredChunk(t, LSim, regSim, everyProbeScript)
	if err := gSim.LoadState(bytes.NewReader(sim), fp); err != nil {
		t.Fatalf("sim-only LoadState: %v", err)
	}
	gSim.Advance(10) // 10 -> 20 in unbroken terms
	simCount, _ := gSim.Storage().GetInt("probe", "n")
	simHash := gSim.StateHash()
	t.Logf("SIM-ONLY @20: counter=%d hash=%#x (unbroken counter=%d hash=%#x)", simCount, simHash, refCount, refHash)

	// The headline: the periodic's counter — which lives in the closure upvalue the
	// scheduler owns — does NOT advance after a sim-only restore. It freezes at the
	// save value (10) instead of reaching the unbroken 20, because the scheduler
	// blob was never applied. (Note the StateHash is a WEAKER witness here: the
	// probe's Storage counter is not folded into the sim hash, so the hash alone
	// would NOT reveal the dropped scheduler — only observable game state does. That
	// is precisely why the bug was silent.)
	if simCount == refCount {
		t.Fatalf("sim-only counter=%d reached the unbroken %d — the scheduler was NOT dropped, making this regression guard vacuous", simCount, refCount)
	}
	if simCount != atSave {
		t.Logf("note: sim-only counter=%d (neither frozen-at-save %d nor unbroken %d) — still wrong, scheduler not faithfully resumed", simCount, atSave, refCount)
	}
	t.Logf("FSV #204: sim-only save dropped the Game_Every scheduler — counter froze at %d, unbroken reaches %d", simCount, refCount)

	// --- FULL container restore (the new cmd/game path): re-run the entry, load the
	//     sim AND the scripts. The periodic resumes bit-identically. ---
	gFull, LFull, regFull := newScriptGame(t)
	defer LFull.Close()
	defer regFull.Close()
	runRegisteredChunk(t, LFull, regFull, everyProbeScript)
	if err := gFull.LoadState(bytes.NewReader(sim), fp); err != nil {
		t.Fatalf("full LoadState: %v", err)
	}
	if err := LoadScripts(LFull, regFull, bytes.NewReader(scr)); err != nil {
		t.Fatalf("full LoadScripts: %v", err)
	}
	gFull.Advance(10)
	fullCount, _ := gFull.Storage().GetInt("probe", "n")
	fullHash := gFull.StateHash()
	t.Logf("FULL @20: counter=%d hash=%#x", fullCount, fullHash)
	if fullCount != refCount || fullHash != refHash {
		t.Fatalf("full container resume diverged: counter=%d (want %d) hash=%#x (want %#x) — the scheduler did not round-trip", fullCount, refCount, fullHash, refHash)
	}
	t.Logf("FSV #204: full container resumed bit-identically (counter=%d hash=%#x) — the cmd/game fix restores the dropped scheduler", fullCount, fullHash)
}

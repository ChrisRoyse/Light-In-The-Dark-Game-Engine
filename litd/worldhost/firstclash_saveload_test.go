package worldhost_test

// FSV for mid-match save/load over a REAL firstclash AI match (#652,
// ultimate-test-plan Phase 5; exercises the shipped #204 savegame). SoT =
// Game.StateHash(): an unbroken run to terminal (H1) vs save@N → fresh restore →
// resume to terminal (H2), plus the hero level captured before save and after
// load. The restore re-runs the world (rebuilding the Lua trigger graph AND
// re-attaching the stateless melee AI controllers with their strategy/config),
// then savegame.Load overwrites sim+scheduler to the saved tick — so the AI
// domain, hero, and in-flight waves must resume bit-identically.

import (
	"bytes"
	"testing"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/savegame"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/worldhost"
)

const fcFP = uint64(0xF1257C1A) // firstclash save fingerprint
const slSeed = 7654321
const saveAt = 5000 // mid-match: AIs active, well before the 24,000-tick terminal

// runToTerm steps g to a latched result (or detCap) and returns (tick, hash).
func runToTerm(g *api.Game) (int, uint64) {
	for int(g.Tick()) < detCap {
		g.Advance(1)
		if g.Player(0).Result() != api.ResultPlaying || g.Player(1).Result() != api.ResultPlaying {
			return int(g.Tick()), g.StateHash()
		}
	}
	return int(g.Tick()), g.StateHash()
}

func heroLevel(g *api.Game, slot int) int {
	if h, ok := firstHero(g, slot); ok {
		return h.HeroLevel()
	}
	return -1
}

func TestFirstclashMidMatchSaveLoadFSV(t *testing.T) {
	if testing.Short() {
		t.Skip("3 firstclash matches incl. save/load (~10s); full preflight gate")
	}

	// --- Unbroken reference run to terminal. ---
	hu, err := worldhost.Load(firstclashDir, slSeed, 50_000_000)
	if err != nil {
		t.Fatalf("unbroken load: %v", err)
	}
	termRef, hashRef := runToTerm(hu.Game)
	hu.Close()
	t.Logf("FSV unbroken: terminal@%d hashRef=%#016x", termRef, hashRef)

	// --- Save run: advance to saveAt, capture AI/hero state, write container. ---
	hs, err := worldhost.Load(firstclashDir, slSeed, 50_000_000)
	if err != nil {
		t.Fatalf("save load: %v", err)
	}
	hs.Game.Advance(saveAt)
	heroBefore := heroLevel(hs.Game, 0)
	aiBefore := hs.Game.IsAIPlayer(hs.Game.Player(0)) && hs.Game.IsAIPlayer(hs.Game.Player(1))
	hashAtSave := hs.Game.StateHash()
	var buf bytes.Buffer
	if err := savegame.Write(&buf, hs.Game, hs.L, hs.Reg, fcFP); err != nil {
		t.Fatalf("savegame.Write: %v", err)
	}
	// Saving must NOT perturb the running sim.
	if after := hs.Game.StateHash(); after != hashAtSave {
		t.Fatalf("Write perturbed the sim: %#016x -> %#016x", hashAtSave, after)
	}
	hs.Close()
	t.Logf("FSV save@%d: hero0Lvl=%d bothAI=%v container=%d bytes hash=%#016x", saveAt, heroBefore, aiBefore, buf.Len(), hashAtSave)
	if !aiBefore {
		t.Fatal("precondition: both players must be AI at save")
	}

	// --- Restore: re-run firstclash (re-attaches AI + rebuilds triggers), then
	//     load the container (overwrites sim+scheduler to saveAt), resume. ---
	hr, err := worldhost.Load(firstclashDir, slSeed, 50_000_000)
	if err != nil {
		t.Fatalf("restore load: %v", err)
	}
	defer hr.Close()
	if err := savegame.Load(bytes.NewReader(buf.Bytes()), hr.Game, hr.L, hr.Reg, fcFP); err != nil {
		t.Fatalf("savegame.Load: %v", err)
	}
	// Post-load state must match the save point exactly (sim restored).
	heroAfter := heroLevel(hr.Game, 0)
	hashAfterLoad := hr.Game.StateHash()
	t.Logf("FSV post-load@%d: hero0Lvl=%d hash=%#016x (saved hash=%#016x)", hr.Game.Tick(), heroAfter, hashAfterLoad, hashAtSave)
	if int(hr.Game.Tick()) != saveAt {
		t.Fatalf("restored tick %d, want %d", hr.Game.Tick(), saveAt)
	}
	if hashAfterLoad != hashAtSave {
		t.Fatalf("post-load hash %#016x != saved hash %#016x — restore not bit-identical at the save point", hashAfterLoad, hashAtSave)
	}
	if heroAfter != heroBefore {
		t.Fatalf("hero level changed across save/load: before=%d after=%d", heroBefore, heroAfter)
	}

	// --- Resume to terminal: must equal the unbroken run. ---
	termGot, hashGot := runToTerm(hr.Game)
	t.Logf("FSV resume: terminal@%d hashGot=%#016x (ref %#016x)", termGot, hashGot, hashRef)
	if termGot != termRef {
		t.Fatalf("resumed match terminated @%d, unbroken @%d", termGot, termRef)
	}
	if hashGot != hashRef {
		t.Fatalf("HASH MISMATCH: save/load/resume %#016x != unbroken %#016x — AI-match save/load not bit-identical", hashGot, hashRef)
	}
	t.Logf("FSV #652: firstclash mid-match save@%d → fresh restore → resume → terminal hash %#016x == unbroken (AI domain + hero round-tripped)", saveAt, hashGot)
}

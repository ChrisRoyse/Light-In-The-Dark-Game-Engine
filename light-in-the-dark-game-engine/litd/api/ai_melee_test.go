package litd

// Production melee-AI wiring FSV (closes the gap #404 named: the melee RTS brain
// could previously run only against a test simBridge, never a real litd/api.Game,
// because aiBridge did not implement melee.Bridge). This proves AttachMeleeAI
// installs the controller on the live AI domain and that the controller drives
// the real sim deterministically.
//
// SoT = the headless sim read back: footman ENTITIES owned by the AI player
// (counted from the sim stores via liveFootmen) + a whole-world fingerprint for
// cross-run equality. No mocks: the controller runs its full economy/build/
// production/wave Step against the production bridge.

import (
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/ai/melee"
)

// meleeArmyStrategy builds the smallest strategy that isolates PRODUCTION: no
// harvest (lvGame has no workers — GoldWorkers/WoodWorkers 0 ⇒ Economy.SetHarvest
// skipped), no placement (the barracks already exists ⇒ empty Build), and a
// wave Size set high enough that no wave is staged during the test window — so
// the observable is purely "the AI trained its standing army to target".
func meleeArmyStrategy(target int) *melee.Strategy {
	return &melee.Strategy{
		Name:    "test-army",
		Economy: melee.EconomyPlan{}, // all tiers unset ⇒ EconPct defaults to 100
		Army:    melee.ArmyPlan{SoldierType: int(lvFootman), Maintain: target},
		Waves:   melee.WavePlan{Size: 1_000_000}, // never reached: isolate production
		Build:   nil,                             // barracks pre-placed by lvGame
	}
}

// TestAttachMeleeAITrainsArmyFSV — the headline. Attach the real melee controller
// to a production Game; step the headless sim; the AI trains its army to target
// (sim SoT), and the world fingerprint is identical across two independent runs
// (determinism — the lockstep-critical property).
func TestAttachMeleeAITrainsArmyFSV(t *testing.T) {
	const aiPlayer = uint8(1)
	const target = 3

	run := func() (before, after int, fp uint64) {
		g := lvGame(t, aiPlayer)
		before = liveFootmen(g, aiPlayer) // SoT BEFORE: barracks only, no footmen

		g.AttachMeleeAI(g.Player(int(aiPlayer)), meleeArmyStrategy(target),
			melee.Config{GoldID: 0, WoodID: 1}, DifficultyNormal)
		if g.aiDomain == nil || g.aiDomain.Context(int(aiPlayer)) == nil {
			t.Fatal("AttachMeleeAI did not install a live AI context")
		}

		// footman TrainTicks=40; the barracks trains serially, so 3 footmen need
		// ~3×40 ticks plus admission/queue slack. 300 ticks is comfortably enough.
		g.Advance(300)
		after = liveFootmen(g, aiPlayer) // SoT AFTER
		fp = worldFingerprint(g)
		return
	}

	b1, a1, fp1 := run()
	t.Logf("FSV #404/melee run1: footmen before=%d after=%d (target=%d) fingerprint=%#x", b1, a1, target, fp1)
	if b1 != 0 {
		t.Fatalf("precondition: expected 0 footmen before the AI runs, got %d", b1)
	}
	if a1 != target {
		t.Fatalf("melee AI did not train its army: footmen=%d, want %d (production not driven through the bridge?)", a1, target)
	}

	b2, a2, fp2 := run()
	t.Logf("FSV #404/melee run2: footmen before=%d after=%d fingerprint=%#x", b2, a2, fp2)
	if a2 != target {
		t.Fatalf("run2 footmen=%d, want %d", a2, target)
	}
	if fp1 != fp2 {
		t.Fatalf("NON-DETERMINISTIC melee AI: run1 fingerprint %#x != run2 %#x", fp1, fp2)
	}
	t.Logf("FSV PASS: melee AI is production-wired and deterministic — trained %d footmen via the real sim, identical fingerprint %#x across runs", target, fp1)
}

// TestAttachMeleeAIDefeatedNoOpFSV — parity with AttachAI: attaching to a
// defeated player must be a no-op (no context installed).
func TestAttachMeleeAIDefeatedNoOpFSV(t *testing.T) {
	const aiPlayer = uint8(1)
	g := lvGame(t, aiPlayer)
	if !g.w.SetDefeat(aiPlayer) { // mark defeated, then latch via a step
		t.Fatal("SetDefeat refused")
	}
	g.w.Step()

	g.AttachMeleeAI(g.Player(int(aiPlayer)), meleeArmyStrategy(3),
		melee.Config{GoldID: 0, WoodID: 1}, DifficultyNormal)

	if g.aiDomain != nil && g.aiDomain.Context(int(aiPlayer)) != nil {
		t.Fatal("AttachMeleeAI to a defeated player installed a context — must be a no-op")
	}
	t.Log("FSV: AttachMeleeAI to a defeated player is a no-op (parity with AttachAI)")
}

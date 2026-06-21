package main

// FSV for #479: the trigger-authored firebolt spell shipped in worlds/dev-sandbox.
// SoT = target HP + the burn-buff presence + the render-event snapshot, observed
// through the public api after a REAL cast. Proves a complete custom spell works
// purely via the ECA path (cast event → condition → damage+burn+VFX actions).

import (
	"bytes"
	"path/filepath"
	"testing"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
)

const fireboltRef = api.AbilityRef(1) // firebolt is the only ability in dev-sandbox

func fireboltWorld(t *testing.T, seed int64) (*api.Game, func()) {
	t.Helper()
	g, cleanup, err := loadWorld(filepath.Join("..", "..", "worlds", "dev-sandbox"), seed, 200_000_000)
	if err != nil {
		t.Fatalf("dev-sandbox must load: %v", err)
	}
	return g, cleanup
}

// advanceCollectingCues steps n ticks, returning any spell-cue render events
// targeting key (entity index) seen across the window.
func advanceCollectingCues(g *api.Game, n int) int {
	cues := 0
	var buf []api.RenderEvent
	for i := 0; i < n; i++ {
		g.Advance(1)
		buf = g.RenderEvents(buf)
		for _, e := range buf {
			if e.Kind == api.RenderSpellCue {
				cues++
			}
		}
	}
	return cues
}

// TestFireboltCastEnemyFSV — cast firebolt at an enemy: the bolt damages, the
// burn DoT is applied (and ticks), and a non-hashing render cue is emitted.
func TestFireboltCastEnemyFSV(t *testing.T) {
	g, cleanup := fireboltWorld(t, 1)
	defer cleanup()
	p0, p1 := g.Player(0), g.Player(1)
	p0.SetAlliance(p1, 0)
	p1.SetAlliance(p0, 0)
	if !p0.IsEnemy(p1) {
		t.Fatal("p0/p1 not enemies after clearing alliances")
	}
	caster := g.CreateUnit(p0, g.UnitType("hfoo"), api.Vec2{X: 200, Y: 200}, api.Deg(0))
	enemy := g.CreateUnit(p1, g.UnitType("hfoo"), api.Vec2{X: 260, Y: 200}, api.Deg(0))
	fb := caster.AddAbility(fireboltRef)
	if !fb.Valid() {
		t.Fatal("firebolt ability did not equip (ref 1)")
	}
	burn := g.BuffType("burn")

	t.Logf("BEFORE cast: enemy life=%.0f hasBurn=%v", enemy.Life(), enemy.HasBuff(burn))
	if !caster.Cast(fb, enemy) {
		t.Fatal("Cast(firebolt, enemy) returned false")
	}
	cues := advanceCollectingCues(g, 12) // through cast-point + EFFECT edge
	hpAfterBolt := enemy.Life()
	hasBurn := enemy.HasBuff(burn)
	t.Logf("AFTER bolt: enemy life=%.0f hasBurn=%v renderCues=%d", hpAfterBolt, hasBurn, cues)

	// let the burn run its course.
	advanceCollectingCues(g, 120)
	hpFinal := enemy.Life()
	t.Logf("AFTER burn: enemy life=%.0f hasBurn=%v", hpFinal, enemy.HasBuff(burn))

	if hpAfterBolt >= 100 {
		t.Fatalf("bolt dealt no damage: enemy life=%.0f", hpAfterBolt)
	}
	if !hasBurn {
		t.Fatal("burn debuff not applied by the spell")
	}
	if cues == 0 {
		t.Fatal("no render spell-cue emitted (non-hashing VFX channel)")
	}
	if hpFinal >= hpAfterBolt {
		t.Fatalf("burn DoT dealt no further damage: %.0f → %.0f", hpAfterBolt, hpFinal)
	}
}

// TestFireboltCastAllyBlockedFSV — casting at an ally is blocked by the
// target-is-enemy condition: no damage, no burn.
func TestFireboltCastAllyBlockedFSV(t *testing.T) {
	g, cleanup := fireboltWorld(t, 1)
	defer cleanup()
	p0 := g.Player(0)
	caster := g.CreateUnit(p0, g.UnitType("hfoo"), api.Vec2{X: 200, Y: 200}, api.Deg(0))
	ally := g.CreateUnit(p0, g.UnitType("hfoo"), api.Vec2{X: 260, Y: 200}, api.Deg(0))
	fb := caster.AddAbility(fireboltRef)
	burn := g.BuffType("burn")

	caster.Cast(fb, ally)
	g.Advance(30)
	t.Logf("FSV #479 ally-cast blocked: ally life=%.0f hasBurn=%v", ally.Life(), ally.HasBuff(burn))
	if ally.Life() != 100 || ally.HasBuff(burn) {
		t.Fatalf("ally took spell effect despite the enemy-only condition: life=%.0f burn=%v", ally.Life(), ally.HasBuff(burn))
	}
}

// TestFireboltKillsTargetFSV — a bolt that exceeds the target's life kills it;
// the unit dies and its burn debuff is cleaned up (no lingering instance).
func TestFireboltKillsTargetFSV(t *testing.T) {
	g, cleanup := fireboltWorld(t, 1)
	defer cleanup()
	p0, p1 := g.Player(0), g.Player(1)
	p0.SetAlliance(p1, 0)
	p1.SetAlliance(p0, 0)
	caster := g.CreateUnit(p0, g.UnitType("hfoo"), api.Vec2{X: 200, Y: 200}, api.Deg(0))
	enemy := g.CreateUnit(p1, g.UnitType("hfoo"), api.Vec2{X: 260, Y: 200}, api.Deg(0))
	enemy.SetLife(20) // less than the 30 bolt → lethal
	fb := caster.AddAbility(fireboltRef)
	caster.Cast(fb, enemy)
	g.Advance(30)
	t.Logf("FSV #479 lethal bolt: enemy alive=%v hasBurn=%v", enemy.Alive(), enemy.HasBuff(g.BuffType("burn")))
	if enemy.Alive() {
		t.Fatalf("enemy (20 life) survived a 30 bolt")
	}
	if enemy.HasBuff(g.BuffType("burn")) {
		t.Fatal("dead unit still carries the burn debuff (cleanup failed)")
	}
}

// TestFireboltDeterminismFSV — the whole trigger-authored spell is
// deterministic: two identical runs hash identically.
func TestFireboltDeterminismFSV(t *testing.T) {
	run := func() uint64 {
		g, cleanup := fireboltWorld(t, 7)
		defer cleanup()
		p0, p1 := g.Player(0), g.Player(1)
		p0.SetAlliance(p1, 0)
		p1.SetAlliance(p0, 0)
		caster := g.CreateUnit(p0, g.UnitType("hfoo"), api.Vec2{X: 200, Y: 200}, api.Deg(0))
		enemy := g.CreateUnit(p1, g.UnitType("hfoo"), api.Vec2{X: 260, Y: 200}, api.Deg(0))
		fb := caster.AddAbility(fireboltRef)
		caster.Cast(fb, enemy)
		g.Advance(60)
		return g.StateHash()
	}
	a, b := run(), run()
	t.Logf("FSV #479 determinism: run1=%#016x run2=%#016x", a, b)
	if a != b {
		t.Fatalf("trigger-spell nondeterministic: %#x != %#x", a, b)
	}
}

// TestFireboltSaveResumeFSV — saving mid-burn and loading into a re-bound world
// (the world re-loaded, re-creating + re-binding the firebolt trigger) resumes
// the burn with an identical hash.
func TestFireboltSaveResumeFSV(t *testing.T) {
	g, cleanup := fireboltWorld(t, 3)
	defer cleanup()
	p0, p1 := g.Player(0), g.Player(1)
	p0.SetAlliance(p1, 0)
	p1.SetAlliance(p0, 0)
	caster := g.CreateUnit(p0, g.UnitType("hfoo"), api.Vec2{X: 200, Y: 200}, api.Deg(0))
	enemy := g.CreateUnit(p1, g.UnitType("hfoo"), api.Vec2{X: 260, Y: 200}, api.Deg(0))
	fb := caster.AddAbility(fireboltRef)
	caster.Cast(fb, enemy)
	g.Advance(12) // bolt landed, burn ticking
	if !enemy.HasBuff(g.BuffType("burn")) {
		t.Fatal("precondition: enemy must be burning before the save")
	}
	srcHash := g.StateHash()
	var buf bytes.Buffer
	if err := g.SaveState(&buf, 0); err != nil {
		t.Fatalf("SaveState: %v", err)
	}

	// re-bound world: a fresh load (re-creates + re-binds the firebolt trigger),
	// then LoadState restores the mid-burn state.
	g2, cleanup2 := fireboltWorld(t, 3)
	defer cleanup2()
	if err := g2.LoadState(bytes.NewReader(buf.Bytes()), 0); err != nil {
		t.Fatalf("LoadState into re-bound world: %v", err)
	}
	dstHash := g2.StateHash()
	t.Logf("FSV #479 save/resume: srcHash=%#016x dstHash=%#016x", srcHash, dstHash)
	if dstHash != srcHash {
		t.Fatalf("post-load hash %#x != pre-save %#x — burn did not resume identically", dstHash, srcHash)
	}
}

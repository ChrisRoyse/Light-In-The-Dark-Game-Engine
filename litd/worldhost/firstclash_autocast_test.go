package worldhost_test

// FSV for the scripted hero auto-cast ability (#641, ultimate-test-plan Phase 1
// D6). The vigil race script gives its hero Warden's Smite and casts it at the
// nearest enemy in range when ready. SoT = the target unit's Life read via the
// api before vs after the cast window — Smite deals 60 magic damage, so a target
// in range loses life (the cast fired AND landed), while one out of range or an
// ally is untouched (no spurious cast). Synthetic: we stage the enemy beside the
// hero (the turtle match never brings them together on its own).

import (
	"testing"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/worldhost"
)

// stageTargetNearVigilHero loads firstclash, makes p0/p1 mutual enemies, and
// spawns an enemy bastion (1500 HP, owned by slot 1) at the given offset from the
// vigil hero. Returns the host, the hero, and the target. Caller closes the host.
func stageTargetNearVigilHero(t *testing.T, offset float64) (*worldhost.Host, api.Unit, api.Unit) {
	t.Helper()
	h, err := worldhost.Load(firstclashDir, 24680, 50_000_000)
	if err != nil {
		t.Fatalf("load firstclash: %v", err)
	}
	g := h.Game
	p0, p1 := g.Player(0), g.Player(1)
	p0.SetAlliance(p1, 0) // 0 = enemy
	p1.SetAlliance(p0, 0)

	hero, ok := firstHero(g, 0)
	if !ok {
		h.Close()
		t.Fatal("vigil hero not found")
	}
	hp := hero.Position()
	target := g.CreateUnit(p1, g.UnitType("bastion"), api.Vec2{X: hp.X + offset, Y: hp.Y}, api.Deg(0))
	if !target.Valid() {
		h.Close()
		t.Fatal("could not stage target bastion")
	}
	return h, hero, target
}

func TestFirstclashHeroAutoCastFSV(t *testing.T) {
	if testing.Short() {
		t.Skip("hero auto-cast windows (~1s); full preflight gate")
	}
	const window = 200 // 10s: past mana avail + the 0.3s cast point + 6s cooldown

	// The CAST is isolated from the hero's normal auto-attack by the hero's MANA:
	// Warden's Smite costs 75 mana; auto-attacks are free. So a mana DROP proves
	// the scripted ability fired (not just that something hit the target), and the
	// target's life drop confirms it LANDED.

	// Edge 1 — enemy IN range (200 < 600): the warden auto-casts Smite. Hero mana
	// drops (cast fired) and the target loses life (cast landed).
	h, hero, target := stageTargetNearVigilHero(t, 200)
	manaBefore := hero.Mana()
	lifeBefore := target.Life()
	h.Game.Advance(window)
	manaAfter, lifeAfter := hero.Mana(), target.Life()
	t.Logf("FSV in-range: hero mana %.0f->%.0f  target life %.0f->%.0f", manaBefore, manaAfter, lifeBefore, lifeAfter)
	if manaAfter >= manaBefore {
		t.Fatalf("hero mana did not drop (%.0f->%.0f) — the scripted Warden's Smite never cast", manaBefore, manaAfter)
	}
	if lifeAfter >= lifeBefore {
		t.Fatalf("target took no damage (%.0f->%.0f) — Smite cast but did not land", lifeBefore, lifeAfter)
	}
	h.Close()

	// Edge 2 — enemy OUT of range (1200 > 600 cast-range): NO cast (mana steady),
	// NO damage. Mana is the decisive signal — the hero cannot even auto-attack at
	// 1200 either, so mana isolates the cast-range gate specifically.
	h2, hero2, target2 := stageTargetNearVigilHero(t, 1200)
	mana2, life2 := hero2.Mana(), target2.Life()
	h2.Game.Advance(window)
	t.Logf("FSV out-of-range: hero mana %.0f->%.0f  target life %.0f->%.0f (want both unchanged)", mana2, hero2.Mana(), life2, target2.Life())
	if hero2.Mana() != mana2 {
		t.Fatalf("hero spent mana with no enemy in 600 range (%.0f->%.0f) — auto-cast ignored cast-range", mana2, hero2.Mana())
	}
	if target2.Life() != life2 {
		t.Fatalf("out-of-range enemy took damage (%.0f->%.0f)", life2, target2.Life())
	}
	h2.Close()

	// Edge 3 — an ALLY in range must NOT be targeted (enemyOf filter): hero mana
	// steady (no cast at a friendly), ally untouched.
	h3, _, _ := stageTargetNearVigilHero(t, 5000) // far enemy, irrelevant here
	g3 := h3.Game
	defer h3.Close()
	hero3, _ := firstHero(g3, 0)
	hp3 := hero3.Position()
	ally := g3.CreateUnit(g3.Player(0), g3.UnitType("bastion"), api.Vec2{X: hp3.X + 150, Y: hp3.Y}, api.Deg(0))
	if !ally.Valid() {
		t.Fatal("could not stage ally bastion")
	}
	mana3, allyLife := hero3.Mana(), ally.Life()
	g3.Advance(window)
	t.Logf("FSV ally-in-range: hero mana %.0f->%.0f  ally life %.0f->%.0f (want both unchanged)", mana3, hero3.Mana(), allyLife, ally.Life())
	if hero3.Mana() != mana3 {
		t.Fatalf("hero cast (mana %.0f->%.0f) with only an ALLY in range — enemyOf filter broken", mana3, hero3.Mana())
	}
	if ally.Life() != allyLife {
		t.Fatalf("ally bastion took damage (%.0f->%.0f) — auto-cast hit a friendly", allyLife, ally.Life())
	}
	t.Logf("FSV #641: vigil hero auto-casts Warden's Smite at an enemy in range — mana spent (cast fired) + 60 dmg landed; inert (mana steady) vs out-of-range + allied targets")
}

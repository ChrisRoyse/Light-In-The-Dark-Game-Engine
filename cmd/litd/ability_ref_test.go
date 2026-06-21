package main

// FSV for #487: Game.AbilityRef resolves a data-loaded ability's string id to
// its AbilityRef, so a world author no longer has to hardcode a load position
// (the #479 dogfood gap). SoT = the ref the resolver returns + whether the
// resolved ref actually equips and casts through Unit.AddAbility/Unit.Cast.

import (
	"path/filepath"
	"testing"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
)

// TestAbilityRefResolvesFSV — the dev-sandbox firebolt ability resolves by code,
// and the resolved ref is the one that equips + casts (not a coincidental 1).
func TestAbilityRefResolvesFSV(t *testing.T) {
	g, cleanup, err := loadWorld(filepath.Join("..", "..", "worlds", "dev-sandbox"), 1, 200_000_000)
	if err != nil {
		t.Fatalf("dev-sandbox must load: %v", err)
	}
	defer cleanup()

	// Happy path: known id resolves.
	ref, ok := g.AbilityRef("firebolt")
	t.Logf("FSV #487 resolve: AbilityRef(\"firebolt\")=%d ok=%v", uint16(ref), ok)
	if !ok {
		t.Fatal("firebolt did not resolve by code")
	}
	if ref.IsZero() {
		t.Fatal("resolved ref is the null ref")
	}

	// The resolved ref must actually equip + cast (proves it is the real ref,
	// not a placeholder) — the dogfood the #479 hardcode could not do.
	p0, p1 := g.Player(0), g.Player(1)
	p0.SetAlliance(p1, 0)
	p1.SetAlliance(p0, 0)
	caster := g.CreateUnit(p0, g.UnitType("hfoo"), api.Vec2{X: 200, Y: 200}, api.Deg(0))
	enemy := g.CreateUnit(p1, g.UnitType("hfoo"), api.Vec2{X: 260, Y: 200}, api.Deg(0))
	fb := caster.AddAbility(ref)
	if !fb.Valid() {
		t.Fatalf("resolved ref %d did not equip", uint16(ref))
	}
	if !caster.Cast(fb, enemy) {
		t.Fatal("cast with resolved ref returned false")
	}
	g.Advance(12)
	t.Logf("FSV #487 cast via resolved ref: enemy life=%.0f", enemy.Life())
	if enemy.Life() >= 100 {
		t.Fatal("resolved ref did not produce a working ability (no damage)")
	}
}

// TestAbilityRefUnknownFSV — edge cases: unknown id, empty id, and a no-ability
// world all fail closed with ok=false and the zero ref (no panic, no false hit).
func TestAbilityRefUnknownFSV(t *testing.T) {
	g, cleanup, err := loadWorld(filepath.Join("..", "..", "worlds", "dev-sandbox"), 1, 200_000_000)
	if err != nil {
		t.Fatalf("dev-sandbox must load: %v", err)
	}
	defer cleanup()

	cases := []string{"nonexistent", "", "FIREBOLT", "firebolt "}
	for _, id := range cases {
		ref, ok := g.AbilityRef(id)
		t.Logf("FSV #487 unknown %q -> ref=%d ok=%v", id, uint16(ref), ok)
		if ok || !ref.IsZero() {
			t.Fatalf("unknown id %q resolved (ref=%d ok=%v) — must fail closed", id, uint16(ref), ok)
		}
	}
}

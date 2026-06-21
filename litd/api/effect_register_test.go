package litd

// FSV for the #477 api surface: Game.RegisterEffect / RunEffect. SoT = the
// source unit's life after a registered "lifesteal" effect runs, plus the
// fail-closed behavior of the setup verb.

import (
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
)

func effectGame(t *testing.T) (*Game, Unit, Unit) {
	t.Helper()
	g, err := NewGame(GameOptions{MaxUnits: 16, Seed: 7})
	if err != nil {
		t.Fatalf("NewGame: %v", err)
	}
	if err := g.DefineUnits([]data.Unit{
		{ID: "hfoo", Life: 1000, MoveSpeedPerTick: 8 * fixed.One, TurnRatePerTick: 65535, CollisionSize: 16},
	}); err != nil {
		t.Fatalf("DefineUnits: %v", err)
	}
	src := g.CreateUnit(g.Player(1), g.UnitType("hfoo"), Vec2{X: 0, Y: 0}, Deg(0))
	tgt := g.CreateUnit(g.Player(2), g.UnitType("hfoo"), Vec2{X: 50, Y: 0}, Deg(0))
	if !src.Valid() || !tgt.Valid() {
		t.Fatal("spawn failed")
	}
	g.w.Healths.Life[g.w.Healths.Row(src.id)] = 500 * fixed.One // below max, room to heal
	return g, src, tgt
}

// TestGameRegisterAndRunEffectFSV — register a lifesteal effect in setup, invoke
// it via RunEffect, and hand-check the source healed.
func TestGameRegisterAndRunEffectFSV(t *testing.T) {
	g, src, tgt := effectGame(t)
	if err := g.RegisterEffect("lifesteal", func(e EffectInvocation) {
		e.Heal(e.Source(), 200)
	}); err != nil {
		t.Fatalf("RegisterEffect: %v", err)
	}
	if !g.EffectRegistered("lifesteal") {
		t.Fatal("EffectRegistered=false after register")
	}
	before := src.Life()
	if !g.RunEffect("lifesteal", src, tgt) {
		t.Fatal("RunEffect returned false")
	}
	after := src.Life()
	t.Logf("FSV #477 api: lifesteal source life %.0f→%.0f", before, after)
	if after != 700 {
		t.Fatalf("source life = %.0f, want 700 (500 + 200 lifesteal)", after)
	}
	if g.RunEffect("nonesuch", src, tgt) {
		t.Fatal("RunEffect on an unregistered name returned true")
	}
}

// TestGameRegisterEffectFailClosedFSV — nil fn, duplicate name, and registration
// after the match starts ticking are all refused.
func TestGameRegisterEffectFailClosedFSV(t *testing.T) {
	g, _, _ := effectGame(t)
	if err := g.RegisterEffect("nilfn", nil); err == nil {
		t.Fatal("nil fn accepted")
	}
	if err := g.RegisterEffect("dup", func(EffectInvocation) {}); err != nil {
		t.Fatalf("first register failed: %v", err)
	}
	if err := g.RegisterEffect("dup", func(EffectInvocation) {}); err == nil {
		t.Fatal("duplicate name accepted")
	}
	g.Advance(1) // match starts ticking → registry frozen
	if err := g.RegisterEffect("late", func(EffectInvocation) {}); err == nil {
		t.Fatal("registration after first Advance accepted")
	}
	t.Log("FSV #477 api fail-closed: nil fn / duplicate / post-Advance all refused")
}

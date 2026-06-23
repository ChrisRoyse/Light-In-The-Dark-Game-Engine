package litd

// #532 regression. SoT = the sim hero store read back through the api handle
// (IsHero / HeroLevel), not CreateUnit's return alone. WC3 parity: creating a
// unit of a HERO type must yield a level-1 hero (its progression row) so
// SetHeroLevel/HeroLevel work; creating a non-hero type must still yield a plain
// unit. Before the fix, CreateUnit always called SpawnFromTable, so a hero-typed
// unit had no hero row: IsHero=false, HeroLevel=0, SetHeroLevel a silent no-op.

import (
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
)

func TestCreateUnitHeroTypeMakesHeroFSV(t *testing.T) {
	w := sim.NewWorld(sim.Caps{Units: 8})
	g := newGame(w)
	// Two unit types: index 0 "caldus" is backed by a hero row; index 1 "grunt"
	// is a plain unit.
	if err := g.DefineUnits([]data.Unit{
		{ID: "caldus", Life: 320, MoveSpeedPerTick: 8 * fixed.One, TurnRatePerTick: 65535, CollisionSize: 16},
		{ID: "grunt", Life: 100, MoveSpeedPerTick: 8 * fixed.One, TurnRatePerTick: 65535, CollisionSize: 16},
	}); err != nil {
		t.Fatalf("DefineUnits: %v", err)
	}
	if err := g.DefineHeroes(&data.HeroTables{
		Curve:  []int64{0, 240, 580},
		Bounty: []int64{0, 0}, // one entry per unit type
		Heroes: []data.HeroDef{{Unit: 0}},
	}); err != nil {
		t.Fatalf("DefineHeroes: %v", err)
	}
	owner := Player{idx: 0, g: g}

	// Hero type -> a real level-1 hero. SoT = the sim hero store via the handle.
	hero := g.CreateUnit(owner, g.UnitType("caldus"), Vec2{X: 256, Y: 256}, Deg(90))
	if !hero.Valid() {
		t.Fatal("CreateUnit(caldus) returned invalid unit")
	}
	t.Logf("FSV hero spawn: IsHero=%v HeroLevel=%d", hero.IsHero(), hero.HeroLevel())
	if !hero.IsHero() {
		t.Fatal("CreateUnit of a hero type produced a non-hero (no hero row) — #532 regressed")
	}
	if hero.HeroLevel() != 1 {
		t.Fatalf("fresh hero level = %d, want 1", hero.HeroLevel())
	}
	// SetHeroLevel now actually takes (it no-ops without a hero row). X+X=Y.
	hero.SetHeroLevel(3)
	if got := hero.HeroLevel(); got != 3 {
		t.Fatalf("after SetHeroLevel(3): HeroLevel=%d, want 3", got)
	}

	// Non-hero type -> a plain unit, unchanged behavior.
	grunt := g.CreateUnit(owner, g.UnitType("grunt"), Vec2{X: 300, Y: 300}, Deg(0))
	if !grunt.Valid() {
		t.Fatal("CreateUnit(grunt) returned invalid unit")
	}
	t.Logf("FSV plain spawn: IsHero=%v HeroLevel=%d", grunt.IsHero(), grunt.HeroLevel())
	if grunt.IsHero() {
		t.Fatal("CreateUnit of a non-hero type produced a hero")
	}
	if grunt.HeroLevel() != 0 {
		t.Fatalf("non-hero HeroLevel=%d, want 0", grunt.HeroLevel())
	}
}

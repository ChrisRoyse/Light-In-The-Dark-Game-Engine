package luabind

// Portable-flame integration FSV (#171, dogfooding #267): loads worlds/the-flame
// — written purely against the bound surface (Game_Every, Game_AllUnits,
// Unit_Type/Alive/ApplyBuff, Game_Storage) — and drives it headlessly. The script
// keeps a flame_aura on every live pyre_wagon; the sim aura system (litd/sim/
// aura.go) then maintains the flame_warmth child (+1 armor) on friendlies in
// radius. SoT = each unit's buff/armor state via the Go api + the published
// Storage tracking row. Verifies: warmed IFF (friendly AND in-radius AND a live
// wagon exists), carrier included, enemies/out-of-range excluded, death
// extinguishes after linger, two wagons do not stack, and the run is deterministic.
//
// The Flicker-immunity half (#170: the carried light's radius does not shrink in
// the dim phase) lives in unit DATA — sight-night == sight-day on the wagon — and
// is verified at the loaded-table SoT in TestPortableFlameDimImmuneDataFSV below.

import (
	"os"
	"path/filepath"
	"testing"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	lua "github.com/yuin/gopher-lua"
)

const flameWorld = "the-flame"

// flameBuffs mirrors worlds/the-flame/data/buffs/flame.toml as hand-built tables
// (the catalog harness defines data programmatically and only executes main.lua).
// Order is [flame_aura(0), flame_warmth(1)] so flame_aura.AuraChild == 1.
func flameBuffs() []data.BuffType {
	return []data.BuffType{
		{
			ID: "flame_aura", DurationTicks: 20, Stacking: data.StackRefresh, MaxStacks: 1,
			AuraRadius: 200 * fixed.One, AuraChild: 1, AuraLingerTicks: 10,
		},
		{
			ID: "flame_warmth", DurationTicks: 20, Stacking: data.StackRefresh, MaxStacks: 1,
			Mods: []data.StatMod{{Stat: data.StatArmor, Add: 1, Permille: 1000}},
		},
	}
}

func flameUnits() []data.Unit {
	return []data.Unit{
		// Wagon: base armor 0 so the warmth (+1) reads cleanly as Armor()==1.
		{ID: "pyre_wagon", Life: 420, Armor: 0, MoveSpeedPerTick: 0, TurnRatePerTick: 32768, CollisionSize: 24,
			SightDay: 2400 * fixed.One, SightNight: 2400 * fixed.One},
		{ID: "forager", Life: 140, Armor: 0, MoveSpeedPerTick: 6 * fixed.One, TurnRatePerTick: 45000, CollisionSize: 16,
			SightDay: 600 * fixed.One, SightNight: 300 * fixed.One},
	}
}

// flameRuntime builds a game with the flame data and the-flame world loaded.
func flameRuntime(t *testing.T, seed int64) (*api.Game, *lua.LState, *ChunkRegistry) {
	t.Helper()
	g, err := api.NewGame(api.GameOptions{MaxUnits: 32, Seed: seed})
	if err != nil {
		t.Fatalf("NewGame: %v", err)
	}
	if err := g.DefineUnits(flameUnits()); err != nil {
		t.Fatalf("DefineUnits: %v", err)
	}
	if err := g.DefineBuffTypes(flameBuffs()); err != nil {
		t.Fatalf("DefineBuffTypes: %v", err)
	}
	L := lua.NewState()
	if err := Register(L, g); err != nil {
		t.Fatalf("Register: %v", err)
	}
	reg := NewChunkRegistry()
	t.Cleanup(func() { L.Close(); reg.Close() })
	if _, err := LoadWorld(L, reg, filepath.Join("..", "..", "worlds", flameWorld)); err != nil {
		t.Fatalf("LoadWorld(%s): %v", flameWorld, err)
	}
	return g, L, reg
}

// TestPortableFlameAuraFSV — happy path + carrier-included + enemy/out-of-range
// exclusion. Geometry (radius 200): wagon at origin; allyIn 140wu E (in); enemy
// 140wu N (in, hostile); allyOut 500wu E (out).
func TestPortableFlameAuraFSV(t *testing.T) {
	g, _, _ := flameRuntime(t, 7)
	warmth := g.BuffType("flame_warmth")
	wagonT := g.UnitType("pyre_wagon")
	forT := g.UnitType("forager")

	wagon := g.CreateUnit(g.Player(1), wagonT, api.Vec2{X: 1000, Y: 1000}, api.Deg(0))
	allyIn := g.CreateUnit(g.Player(1), forT, api.Vec2{X: 1140, Y: 1000}, api.Deg(0))
	enemy := g.CreateUnit(g.Player(2), forT, api.Vec2{X: 1000, Y: 1140}, api.Deg(0))
	allyOut := g.CreateUnit(g.Player(1), forT, api.Vec2{X: 1500, Y: 1000}, api.Deg(0))
	for n, u := range map[string]api.Unit{"wagon": wagon, "allyIn": allyIn, "enemy": enemy, "allyOut": allyOut} {
		if !u.Valid() {
			t.Fatalf("%s invalid", n)
		}
	}

	// BEFORE: nobody is warmed (the script has not run a tick yet).
	if allyIn.HasBuff(warmth) || allyIn.Armor() != 0 {
		t.Fatalf("BEFORE: allyIn already warmed (armor=%.0f)", allyIn.Armor())
	}
	t.Logf("BEFORE tick: allyIn armor=%.0f warmed=%v", allyIn.Armor(), allyIn.HasBuff(warmth))

	// Run past several 5-tick aura evaluations.
	g.Advance(15)

	// AFTER: friendly in-radius units AND the carrier are warmed (+1 armor);
	// the hostile and the out-of-range ally are not.
	checks := []struct {
		name   string
		u      api.Unit
		warmed bool
	}{
		{"wagon(carrier)", wagon, true},
		{"allyIn", allyIn, true},
		{"enemy", enemy, false},
		{"allyOut", allyOut, false},
	}
	for _, c := range checks {
		gotBuff, gotArmor := c.u.HasBuff(warmth), c.u.Armor()
		wantArmor := 0.0
		if c.warmed {
			wantArmor = 1.0
		}
		t.Logf("AFTER 15t: %-14s warmed=%-5v armor=%.0f (want warmed=%v armor=%.0f)", c.name, gotBuff, gotArmor, c.warmed, wantArmor)
		if gotBuff != c.warmed || gotArmor != wantArmor {
			t.Errorf("%s: warmed=%v armor=%.0f, want warmed=%v armor=%.0f", c.name, gotBuff, gotArmor, c.warmed, wantArmor)
		}
	}

	lit, _ := g.Storage().GetInt("flame", "lit")
	wag, _ := g.Storage().GetInt("flame", "wagons")
	if lit != 1 || wag != 1 {
		t.Errorf("Storage tracking: lit=%d wagons=%d, want 1/1", lit, wag)
	}
	t.Logf("FSV #171 aura: carrier+in-radius friendly warmed, enemy+out-of-range cold; tracking lit=%d wagons=%d", lit, wag)
}

// TestPortableFlameMobileFSV (#171 primary SoT — the "portable"/MOVING light):
// the flame travels with the wagon. An ally warmed at the start loses warmth when
// the wagon drives away (its warmth expires after linger) and regains it when the
// wagon returns. SoT = the ally's warmth state + the published flame center, which
// must follow the wagon across the map.
func TestPortableFlameMobileFSV(t *testing.T) {
	g, _, _ := flameRuntime(t, 7)
	warmth := g.BuffType("flame_warmth")
	wagon := g.CreateUnit(g.Player(1), g.UnitType("pyre_wagon"), api.Vec2{X: 1000, Y: 1000}, api.Deg(0))
	ally := g.CreateUnit(g.Player(1), g.UnitType("forager"), api.Vec2{X: 1140, Y: 1000}, api.Deg(0)) // 140 < 200: in range
	if !wagon.Valid() || !ally.Valid() {
		t.Fatal("setup invalid")
	}

	// 1) Start: ally warmed, flame lit at the wagon's origin.
	g.Advance(15)
	x0, _ := g.Storage().GetInt("flame", "x")
	if !ally.HasBuff(warmth) || x0 != 1000 {
		t.Fatalf("start: warmed=%v flameX=%d, want true/1000", ally.HasBuff(warmth), x0)
	}
	t.Logf("START: ally warmed=%v, flame center x=%d (at wagon)", ally.HasBuff(warmth), x0)

	// 2) Drive the wagon far away — the flame center follows; the now-distant ally
	//    loses warmth after linger.
	wagon.SetPosition(api.Vec2{X: 2000, Y: 1000})
	g.Advance(15)
	xFar, _ := g.Storage().GetInt("flame", "x")
	if xFar != 2000 {
		t.Fatalf("flame did not follow the wagon: x=%d, want 2000", xFar)
	}
	if ally.HasBuff(warmth) {
		t.Fatalf("ally still warmed after the wagon drove 860wu away (warmth must expire after linger)")
	}
	t.Logf("MOVED: wagon→(2000,1000), flame center followed x=%d, distant ally warmed=%v", xFar, ally.HasBuff(warmth))

	// 3) Drive back next to the ally — warmth re-applies (mobile both ways).
	wagon.SetPosition(api.Vec2{X: 1140, Y: 1000})
	g.Advance(15)
	xBack, _ := g.Storage().GetInt("flame", "x")
	if !ally.HasBuff(warmth) {
		t.Fatalf("ally did not re-warm when the wagon returned (flame center x=%d)", xBack)
	}
	t.Logf("RETURNED: wagon→(1140,1000), flame center x=%d, ally re-warmed=%v", xBack, ally.HasBuff(warmth))
	t.Logf("FSV #171 mobile: the flame is genuinely portable — warmth follows the wagon across the map both ways")
}

// TestPortableFlameExtinguishOnDeathFSV (#171 edge 1): killing the wagon
// extinguishes the flame — the warmed ally loses warmth within `linger` ticks,
// with no ghost light lingering on the corpse. SoT = ally armor before/after.
func TestPortableFlameExtinguishOnDeathFSV(t *testing.T) {
	g, _, _ := flameRuntime(t, 7)
	warmth := g.BuffType("flame_warmth")
	wagon := g.CreateUnit(g.Player(1), g.UnitType("pyre_wagon"), api.Vec2{X: 1000, Y: 1000}, api.Deg(0))
	ally := g.CreateUnit(g.Player(1), g.UnitType("forager"), api.Vec2{X: 1140, Y: 1000}, api.Deg(0))
	if !wagon.Valid() || !ally.Valid() {
		t.Fatal("setup invalid")
	}

	g.Advance(15)
	if !ally.HasBuff(warmth) || ally.Armor() != 1 {
		t.Fatalf("precondition: ally must be warmed before the kill (armor=%.0f warmed=%v)", ally.Armor(), ally.HasBuff(warmth))
	}
	t.Logf("BEFORE kill: ally armor=%.0f warmed=%v; wagon alive=%v", ally.Armor(), ally.HasBuff(warmth), wagon.Alive())

	wagon.Kill()
	// linger=10 + 5-tick cadence ⇒ the child expires within ~15 ticks of the
	// last in-range evaluation. Advance generously and assert it is gone.
	g.Advance(20)

	if wagon.Alive() {
		t.Fatalf("wagon should be dead after Kill()")
	}
	if ally.HasBuff(warmth) || ally.Armor() != 0 {
		t.Fatalf("EXTINGUISH FAILED: ally still warmed %dt after the wagon died (armor=%.0f warmed=%v) — ghost light",
			20, ally.Armor(), ally.HasBuff(warmth))
	}
	lit, _ := g.Storage().GetInt("flame", "lit")
	wag, _ := g.Storage().GetInt("flame", "wagons")
	t.Logf("AFTER kill+20t: ally armor=%.0f warmed=%v; tracking lit=%d wagons=%d (flame out)", ally.Armor(), ally.HasBuff(warmth), lit, wag)
	if lit != 0 || wag != 0 {
		t.Errorf("tracking after death: lit=%d wagons=%d, want 0/0", lit, wag)
	}
}

// TestPortableFlameNonStackingFSV (#171 edge: two overlapping wagons): an ally in
// range of TWO live wagons holds ONE warmth child (armor +1, not +2) — `refresh`
// stacking keys the child by (type,target), not per-source. SoT = ally armor.
func TestPortableFlameNonStackingFSV(t *testing.T) {
	g, _, _ := flameRuntime(t, 7)
	warmth := g.BuffType("flame_warmth")
	wagonT := g.UnitType("pyre_wagon")
	w1 := g.CreateUnit(g.Player(1), wagonT, api.Vec2{X: 1000, Y: 1000}, api.Deg(0))
	w2 := g.CreateUnit(g.Player(1), wagonT, api.Vec2{X: 1000, Y: 1200}, api.Deg(0))
	// Midpoint: 100wu from each wagon — inside both radii (200).
	ally := g.CreateUnit(g.Player(1), g.UnitType("forager"), api.Vec2{X: 1000, Y: 1100}, api.Deg(0))
	if !w1.Valid() || !w2.Valid() || !ally.Valid() {
		t.Fatal("setup invalid")
	}

	g.Advance(15)
	if !ally.HasBuff(warmth) {
		t.Fatal("ally between two wagons should be warmed")
	}
	if ally.Armor() != 1 {
		t.Fatalf("STACKING BUG: ally in range of TWO wagons has armor=%.0f, want 1 (one shared warmth child, not per-source)", ally.Armor())
	}
	t.Logf("FSV #171 non-stacking: ally in range of 2 wagons holds ONE warmth (armor=%.0f)", ally.Armor())
}

// TestPortableFlameDeterminismFSV (#171): two seeded runs of the world produce an
// identical final StateHash.
func TestPortableFlameDeterminismFSV(t *testing.T) {
	run := func(seed int64) uint64 {
		g, _, _ := flameRuntime(t, seed)
		g.CreateUnit(g.Player(1), g.UnitType("pyre_wagon"), api.Vec2{X: 1000, Y: 1000}, api.Deg(0))
		g.CreateUnit(g.Player(1), g.UnitType("forager"), api.Vec2{X: 1140, Y: 1000}, api.Deg(0))
		g.CreateUnit(g.Player(2), g.UnitType("forager"), api.Vec2{X: 1000, Y: 1140}, api.Deg(0))
		g.Advance(60)
		return g.StateHash()
	}
	h1, h2 := run(11), run(11)
	if h1 != h2 {
		t.Fatalf("non-deterministic: run1 hash=%#x run2 hash=%#x", h1, h2)
	}
	t.Logf("FSV #171 determinism: two seeded runs identical hash=%#x", h1)
}

// TestPortableFlameDimImmuneDataFSV (#171/#170): the carried light's
// Flicker-immunity is a DATA property — sight-night == sight-day on the wagon, so
// unitSightRadius (litd/sim/visibility.go) returns the same radius in day and
// night. The contrast forager dims (sight-night < sight-day). SoT = the loaded
// worlds/the-flame/data table read straight through the strict loader.
func TestPortableFlameDimImmuneDataFSV(t *testing.T) {
	dir := filepath.Join("..", "..", "worlds", flameWorld, "data")
	tb, err := data.Load(os.DirFS(dir))
	if err != nil {
		t.Fatalf("data.Load(%s): %v", dir, err)
	}
	find := func(id string) *data.Unit {
		for i := range tb.Units {
			if tb.Units[i].ID == id {
				return &tb.Units[i]
			}
		}
		t.Fatalf("unit %q not in loaded table", id)
		return nil
	}
	wagon, forager := find("pyre_wagon"), find("forager")
	t.Logf("wagon  sight day=%v night=%v (immune iff equal)", wagon.SightDay, wagon.SightNight)
	t.Logf("forager sight day=%v night=%v (dims iff night<day)", forager.SightDay, forager.SightNight)
	if wagon.SightDay != wagon.SightNight {
		t.Errorf("FLICKER-IMMUNITY BROKEN: wagon sight day=%v != night=%v — the carried light shrinks in the dim phase", wagon.SightDay, wagon.SightNight)
	}
	if !(forager.SightNight < forager.SightDay) {
		t.Errorf("contrast unit forager should dim: night=%v not < day=%v", forager.SightNight, forager.SightDay)
	}
	t.Logf("FSV #171 dim-immunity: wagon radius constant across phases; normal unit dims")
}

package luabind

// Unbound portable-flame integration FSV (#171, dogfooding #267): loads
// worlds/the-flame — written purely against the bound surface (Game_Every,
// Game_AllUnits, Unit_Type/Alive/Position/Owner, Game_UnitsInRange, Player_IsEnemy,
// Unit_ApplyBuff/HasBuff/RemoveBuff, Game_SetFogState, Storage) — and drives a
// pyre_wagon headlessly. Primary SoT (per spec) = wagon position tracking + the
// buffed-unit list, via the Go api. Vision is logged (confounded by the wagon's
// own intrinsic sight, so the aura is the rigorous death/range SoT).

import (
	"path/filepath"
	"testing"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	lua "github.com/yuin/gopher-lua"
)

func flameGame(t *testing.T, seed int64) *api.Game {
	t.Helper()
	g, err := api.NewGame(api.GameOptions{MaxUnits: 32, Seed: seed})
	if err != nil {
		t.Fatalf("NewGame: %v", err)
	}
	if err := g.DefineUnits([]data.Unit{
		{ID: "pyre_wagon", Life: 200, MoveSpeedPerTick: 8 * fixed.One, TurnRatePerTick: 65535, CollisionSize: 20},
		{ID: "footman", Life: 100, MoveSpeedPerTick: 8 * fixed.One, TurnRatePerTick: 65535, CollisionSize: 16},
	}); err != nil {
		t.Fatalf("DefineUnits: %v", err)
	}
	if err := g.DefineBuffTypes([]data.BuffType{
		{ID: "flame_aura", DurationTicks: 100, Stacking: data.StackRefresh, MaxStacks: 1},
	}); err != nil {
		t.Fatalf("DefineBuffTypes: %v", err)
	}
	return g
}

func flameInt(g *api.Game, key string) int { v, _ := g.Storage().GetInt("flame", key); return v }

func loadFlame(t *testing.T, g *api.Game) {
	t.Helper()
	L := lua.NewState()
	if err := Register(L, g); err != nil {
		t.Fatalf("Register: %v", err)
	}
	reg := NewChunkRegistry()
	t.Cleanup(func() { L.Close(); reg.Close() })
	if _, err := LoadWorld(L, reg, filepath.Join("..", "..", "worlds", "the-flame")); err != nil {
		t.Fatalf("LoadWorld(the-flame): %v", err)
	}
}

func TestPortableFlameWorldFSV(t *testing.T) {
	g := flameGame(t, 5)
	aura := g.BuffType("flame_aura")
	wagon := g.CreateUnit(g.Player(1), g.UnitType("pyre_wagon"), api.Vec2{X: 300, Y: 300}, api.Deg(0))
	foot := g.CreateUnit(g.Player(1), g.UnitType("footman"), api.Vec2{X: 350, Y: 300}, api.Deg(0)) // 50 < 200: in range
	if !wagon.Valid() || !foot.Valid() {
		t.Fatal("fixtures invalid")
	}
	loadFlame(t, g)

	// 1) Friendly in range gets the aura; flame tracks the wagon.
	g.Advance(5)
	if !foot.HasBuff(aura) {
		t.Fatal("footman in flame radius has no aura")
	}
	if foot.BuffCount() != 1 {
		t.Fatalf("aura stacked: BuffCount=%d, want 1 (non-stacking)", foot.BuffCount())
	}
	if flameInt(g, "lit") != 1 || flameInt(g, "x") != 300 || flameInt(g, "y") != 300 {
		t.Fatalf("flame not tracking at start: lit=%d x=%d y=%d, want 1/300/300", flameInt(g, "lit"), flameInt(g, "x"), flameInt(g, "y"))
	}
	t.Logf("FSV #171 aura+track: footman aura'd (count=1), flame lit at wagon (300,300)")

	// 2) Move the wagon far away — flame center follows; the now-distant footman drops the aura.
	wagon.SetPosition(api.Vec2{X: 1200, Y: 300})
	g.Advance(5)
	if flameInt(g, "x") != 1200 {
		t.Fatalf("flame did not follow wagon: x=%d, want 1200", flameInt(g, "x"))
	}
	if foot.HasBuff(aura) {
		t.Fatal("footman still aura'd after wagon left its radius")
	}
	t.Logf("FSV #171 mobile: wagon→(1200,300), flame center followed (x=1200), distant footman lost aura")

	// 3) Bring the wagon back — aura re-applies (flame is genuinely mobile both ways).
	wagon.SetPosition(api.Vec2{X: 350, Y: 300})
	g.Advance(5)
	if !foot.HasBuff(aura) {
		t.Fatal("footman did not re-gain aura when wagon returned")
	}
	t.Logf("FSV #171 mobile: wagon returned → footman re-aura'd")

	// 4) Kill the wagon → flame extinguished next tick: aura gone, nothing lit.
	wagon.Kill()
	g.Advance(2)
	if foot.HasBuff(aura) {
		t.Fatal("aura lingered after wagon death (ghost light)")
	}
	if flameInt(g, "lit") != 0 || flameInt(g, "wagons") != 0 {
		t.Fatalf("flame not extinguished on death: lit=%d wagons=%d, want 0/0", flameInt(g, "lit"), flameInt(g, "wagons"))
	}
	t.Logf("FSV #171 death: wagon killed → aura removed, flame lit=0, wagons=0 (no ghost light)")

	// 5) Edge — two overlapping wagons: the aura stays a single instance (non-stacking).
	g2 := flameGame(t, 7)
	aura2 := g2.BuffType("flame_aura")
	g2.CreateUnit(g2.Player(1), g2.UnitType("pyre_wagon"), api.Vec2{X: 500, Y: 500}, api.Deg(0))
	g2.CreateUnit(g2.Player(1), g2.UnitType("pyre_wagon"), api.Vec2{X: 520, Y: 500}, api.Deg(0)) // overlapping radii
	f2 := g2.CreateUnit(g2.Player(1), g2.UnitType("footman"), api.Vec2{X: 510, Y: 500}, api.Deg(0))
	loadFlame(t, g2)
	g2.Advance(5)
	if !f2.HasBuff(aura2) || f2.BuffCount() != 1 {
		t.Fatalf("two-wagon overlap: HasBuff=%v count=%d, want true/1 (single instance)", f2.HasBuff(aura2), f2.BuffCount())
	}
	t.Logf("FSV #171 non-stacking: footman under two overlapping flames carries exactly 1 aura instance")
}

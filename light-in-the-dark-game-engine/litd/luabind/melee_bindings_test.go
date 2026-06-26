package luabind

// Full State Verification for the melee setup Lua bindings (#637, ultimate-test-
// plan Phase 1, G3). Every verb is driven THROUGH Lua; the Source of Truth is
// the post-call sim state read back through the public api (Player.Gold,
// AllUnits owner count, IsAIPlayer/AIDifficulty, Unit.IsHero/HeroLevel,
// Player.Result) — never the Lua return value. Happy path + the three mandated
// fail-closed edges, each printing state BEFORE and AFTER.

import (
	"strings"
	"testing"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	lua "github.com/yuin/gopher-lua"
)

// meleeBindGame builds a 4-type game (town hall, worker, soldier, hero) with two
// players, Lua registered, and p0/p1 preset as globals.
func meleeBindGame(t *testing.T) (*api.Game, *lua.LState) {
	t.Helper()
	g, err := api.NewGame(api.GameOptions{MaxUnits: 64, Seed: 1})
	if err != nil {
		t.Fatalf("NewGame: %v", err)
	}
	// index 0 thal (town hall), 1 wrkr (worker), 2 sold (soldier), 3 champ (hero).
	if err := g.DefineUnits([]data.Unit{
		{ID: "thal", Life: 1500, MoveSpeedPerTick: 0, TurnRatePerTick: 65535, CollisionSize: 48},
		{ID: "wrkr", Life: 220, MoveSpeedPerTick: 8 * fixed.One, TurnRatePerTick: 65535, CollisionSize: 16},
		{ID: "sold", Life: 420, MoveSpeedPerTick: 8 * fixed.One, TurnRatePerTick: 65535, CollisionSize: 16},
		{ID: "champ", Life: 600, MoveSpeedPerTick: 8 * fixed.One, TurnRatePerTick: 65535, CollisionSize: 16},
	}); err != nil {
		t.Fatalf("DefineUnits: %v", err)
	}
	// gold(0)/lumber(1) ledger — without it SetGold/SetLumber are no-ops (#388).
	if err := g.DefineEconomy(2); err != nil {
		t.Fatalf("DefineEconomy: %v", err)
	}
	// champ (unit index 3) is a hero; one bounty/curve entry per type.
	if err := g.DefineHeroes(&data.HeroTables{
		Curve:  []int64{0, 240, 580, 1000},
		Bounty: []int64{0, 0, 0, 0},
		Heroes: []data.HeroDef{{Unit: 3}},
	}); err != nil {
		t.Fatalf("DefineHeroes: %v", err)
	}
	g.Player(0).SetStartLocation(api.Vec2{X: 100, Y: 100})
	g.Player(1).SetStartLocation(api.Vec2{X: 900, Y: 900})

	L := lua.NewState()
	if err := Register(L, g); err != nil {
		t.Fatalf("Register: %v", err)
	}
	t.Cleanup(func() { L.Close() })
	set := func(name string, v any) {
		ud := L.NewUserData()
		ud.Value = v
		L.SetGlobal(name, ud)
	}
	set("p0", g.Player(0))
	set("p1", g.Player(1))
	return g, L
}

// ownedUnits counts live units owned by slot via the public all-units query.
func ownedUnits(g *api.Game, slot int) int {
	n := 0
	for range g.AllUnits(func(v api.UnitView) bool { return v.OwnerPlayer() == slot }) {
		n++
	}
	return n
}

func TestMeleeBindingsFSV(t *testing.T) {
	g, L := meleeBindGame(t)

	// --- Happy path: melee_StartingResources ---------------------------------
	const factionTOML = `
name = "testfac"
gold = 500
lumber = 100
food_cap = 20
town_hall = "thal"
[workers]
code = "wrkr"
count = 3
`
	t.Logf("FSV StartingResources BEFORE: p0 gold=%d lumber=%d foodcap=%d",
		g.Player(0).Gold(), g.Player(0).Lumber(), g.Player(0).FoodCap())
	if err := L.DoString(`melee_StartingResources(p0, [==[` + factionTOML + `]==])`); err != nil {
		t.Fatalf("melee_StartingResources raised: %v", err)
	}
	t.Logf("FSV StartingResources AFTER:  p0 gold=%d lumber=%d foodcap=%d",
		g.Player(0).Gold(), g.Player(0).Lumber(), g.Player(0).FoodCap())
	if g.Player(0).Gold() != 500 || g.Player(0).Lumber() != 100 || g.Player(0).FoodCap() != 20 {
		t.Fatalf("StartingResources SoT mismatch: gold=%d lumber=%d foodcap=%d, want 500/100/20",
			g.Player(0).Gold(), g.Player(0).Lumber(), g.Player(0).FoodCap())
	}

	// --- Happy path: melee_StartingUnits (1 town hall + 3 workers = 4) --------
	before := ownedUnits(g, 0)
	t.Logf("FSV StartingUnits BEFORE: p0 owns %d units", before)
	if err := L.DoString(`u = melee_StartingUnits(p0, [==[` + factionTOML + `]==])`); err != nil {
		t.Fatalf("melee_StartingUnits raised: %v", err)
	}
	after := ownedUnits(g, 0)
	t.Logf("FSV StartingUnits AFTER:  p0 owns %d units (delta=%d)", after, after-before)
	if after-before != 4 {
		t.Fatalf("StartingUnits spawned %d units, want 4 (1 hall + 3 workers)", after-before)
	}
	// The Lua return is a 4-element array of unit handles.
	if n := int(lua.LVAsNumber(L.GetGlobal("u").(*lua.LTable).RawGetInt(0))); n != 0 {
		_ = n // (length checked below)
	}
	if tbl, ok := L.GetGlobal("u").(*lua.LTable); !ok || tbl.Len() != 4 {
		t.Fatalf("StartingUnits Lua return not a 4-element array")
	}

	// --- Happy path: melee_SpawnHero -----------------------------------------
	heroesBefore := ownedUnits(g, 0)
	t.Logf("FSV SpawnHero BEFORE: p0 owns %d units", heroesBefore)
	if err := L.DoString(`h = melee_SpawnHero(p0, "champ", {x=120, y=120}, 270)`); err != nil {
		t.Fatalf("melee_SpawnHero raised: %v", err)
	}
	hud, ok := L.GetGlobal("h").(*lua.LUserData)
	if !ok {
		t.Fatal("SpawnHero did not return a unit handle")
	}
	hero := hud.Value.(api.Unit)
	t.Logf("FSV SpawnHero AFTER:  IsHero=%v HeroLevel=%d (p0 owns %d units)",
		hero.IsHero(), hero.HeroLevel(), ownedUnits(g, 0))
	if !hero.IsHero() || hero.HeroLevel() != 1 {
		t.Fatalf("SpawnHero SoT: IsHero=%v HeroLevel=%d, want true/1", hero.IsHero(), hero.HeroLevel())
	}

	// --- Happy path: Game_AttachMeleeAI --------------------------------------
	const strategyTOML = `
name = "teststrat"
[army]
soldier_type = 2
maintain = 8
[waves]
size = 6
[[build]]
type = 0
count = 1
`
	t.Logf("FSV AttachMeleeAI BEFORE: IsAIPlayer(p1)=%v", g.IsAIPlayer(g.Player(1)))
	// difficulty 2 == DifficultyInsane.
	if err := L.DoString(`Game_AttachMeleeAI(p1, [==[` + strategyTOML + `]==], {gold_id=0, wood_id=1}, 2)`); err != nil {
		t.Fatalf("Game_AttachMeleeAI raised: %v", err)
	}
	t.Logf("FSV AttachMeleeAI AFTER:  IsAIPlayer(p1)=%v AIDifficulty=%d",
		g.IsAIPlayer(g.Player(1)), int(g.AIDifficulty(g.Player(1))))
	if !g.IsAIPlayer(g.Player(1)) {
		t.Fatal("AttachMeleeAI SoT: p1 not marked AI-controlled in sim")
	}
	if g.AIDifficulty(g.Player(1)) != api.DifficultyInsane {
		t.Fatalf("AttachMeleeAI difficulty=%d, want Insane(%d)", int(g.AIDifficulty(g.Player(1))), int(api.DifficultyInsane))
	}

	// --- Happy path: melee_VictoryDefeatConditions ---------------------------
	// p0 has units (hall+workers+hero); p1 has none → installing the rule defeats
	// p1 at t=0 and crowns p0 the lone survivor.
	t.Logf("FSV VictoryDefeat BEFORE: p0.Result=%d p1.Result=%d", int(g.Player(0).Result()), int(g.Player(1).Result()))
	if err := L.DoString(`melee_VictoryDefeatConditions({p0, p1})`); err != nil {
		t.Fatalf("melee_VictoryDefeatConditions raised: %v", err)
	}
	g.Advance(2)
	t.Logf("FSV VictoryDefeat AFTER:  p0.Result=%d p1.Result=%d", int(g.Player(0).Result()), int(g.Player(1).Result()))
	if g.Player(0).Result() != api.ResultWon {
		t.Fatalf("VictoryDefeat: p0.Result=%d, want Won(%d)", int(g.Player(0).Result()), int(api.ResultWon))
	}
	if g.Player(1).Result() != api.ResultLost {
		t.Fatalf("VictoryDefeat: p1.Result=%d, want Lost(%d)", int(g.Player(1).Result()), int(api.ResultLost))
	}
}

// TestMeleeBindingsFailClosedEdges covers the three mandated fail-closed edges:
// each must raise a Lua error and leave the sim Source of Truth unchanged.
func TestMeleeBindingsFailClosedEdges(t *testing.T) {
	// Edge 1a: SpawnHero with an unknown code → Lua error, zero units spawned.
	g, L := meleeBindGame(t)
	before := ownedUnits(g, 0)
	t.Logf("EDGE SpawnHero(bad code) BEFORE: p0 owns %d units", before)
	err := L.DoString(`melee_SpawnHero(p0, "zzzz", {x=0,y=0}, 0)`)
	after := ownedUnits(g, 0)
	t.Logf("EDGE SpawnHero(bad code) AFTER:  err=%v; p0 owns %d units", err != nil, after)
	if err == nil {
		t.Fatal("SpawnHero(unknown code) did NOT raise — fail-closed violated")
	}
	if !strings.Contains(err.Error(), "not bound") {
		t.Fatalf("SpawnHero error %q does not name the missing code", err.Error())
	}
	if after != before {
		t.Fatalf("SpawnHero(bad code) spawned %d units — must be 0", after-before)
	}

	// Edge 1b: SpawnHero with a real-but-non-hero code → Lua error, zero units.
	err = L.DoString(`melee_SpawnHero(p0, "wrkr", {x=0,y=0}, 0)`)
	t.Logf("EDGE SpawnHero(non-hero) AFTER: err=%v; p0 owns %d units", err != nil, ownedUnits(g, 0))
	if err == nil || !strings.Contains(err.Error(), "not a hero") {
		t.Fatalf("SpawnHero(non-hero code) want 'not a hero' error, got %v", err)
	}
	if ownedUnits(g, 0) != before {
		t.Fatal("SpawnHero(non-hero) spawned a unit — must be 0")
	}

	// Edge 2: AttachMeleeAI on an invalid player → Lua error, no controller.
	g2, L2 := meleeBindGame(t)
	const strat = `
name = "s"
[army]
soldier_type = 2
maintain = 1
[waves]
size = 1
[[build]]
type = 0
count = 1
`
	// Game_Player(999) is out of range → the zero (invalid) Player.
	t.Logf("EDGE AttachMeleeAI(invalid slot) BEFORE: any AI? p0=%v p1=%v",
		g2.IsAIPlayer(g2.Player(0)), g2.IsAIPlayer(g2.Player(1)))
	err = L2.DoString(`Game_AttachMeleeAI(Game_Player(999), [==[` + strat + `]==], {}, 1)`)
	t.Logf("EDGE AttachMeleeAI(invalid slot) AFTER: err=%v", err != nil)
	if err == nil || !strings.Contains(err.Error(), "invalid") {
		t.Fatalf("AttachMeleeAI(invalid player) want 'invalid' error, got %v", err)
	}
	// No player became AI-controlled.
	if g2.IsAIPlayer(g2.Player(0)) || g2.IsAIPlayer(g2.Player(1)) {
		t.Fatal("AttachMeleeAI(invalid) installed a controller somewhere — must be a no-op")
	}

	// Edge 3: StartingResources with negative gold → loader rejects, no mutation.
	g3, L3 := meleeBindGame(t)
	goldBefore := g3.Player(0).Gold()
	t.Logf("EDGE StartingResources(neg gold) BEFORE: p0 gold=%d", goldBefore)
	const badFaction = `
name = "bad"
gold = -50
lumber = 0
town_hall = "thal"
`
	err = L3.DoString(`melee_StartingResources(p0, [==[` + badFaction + `]==])`)
	t.Logf("EDGE StartingResources(neg gold) AFTER: err=%v; p0 gold=%d", err != nil, g3.Player(0).Gold())
	if err == nil || !strings.Contains(err.Error(), "negative") {
		t.Fatalf("StartingResources(neg gold) want 'negative' error, got %v", err)
	}
	if g3.Player(0).Gold() != goldBefore {
		t.Fatalf("StartingResources(neg gold) mutated gold to %d — must stay %d", g3.Player(0).Gold(), goldBefore)
	}
}

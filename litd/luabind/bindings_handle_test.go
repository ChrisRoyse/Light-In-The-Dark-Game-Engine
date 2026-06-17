package luabind

// FSV for the generated HANDLE-verb dispatch (#267). SoT = live sim state read
// back through the api after a Lua call: a getter (Unit_Position) returns the
// unit's real position bit-identically to the Go call, and a setter
// (Unit_SetLife) called from Lua actually mutates the sim (read back via Go).
// Plus the fail-closed edge: a wrong-typed userdata raises, never mis-casts.

import (
	"strings"
	"testing"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	lua "github.com/yuin/gopher-lua"
)

func TestGeneratedHandleBindingsFSV(t *testing.T) {
	g, err := api.NewGame(api.GameOptions{MaxUnits: 16, Seed: 3})
	if err != nil {
		t.Fatalf("NewGame: %v", err)
	}
	if err := g.DefineUnits([]data.Unit{
		{ID: "hfoo", Life: 100, MoveSpeedPerTick: 8 * fixed.One, TurnRatePerTick: 65535, CollisionSize: 16},
	}); err != nil {
		t.Fatalf("DefineUnits: %v", err)
	}
	u := g.CreateUnit(g.Player(1), g.UnitType("hfoo"), api.Vec2{X: 100, Y: 200}, api.Deg(0))
	if !u.Valid() {
		t.Fatal("CreateUnit returned invalid unit")
	}

	L := lua.NewState()
	defer L.Close()
	if err := Register(L, g); err != nil {
		t.Fatalf("Register: %v", err)
	}
	ud := L.NewUserData()
	ud.Value = u // a live api.Unit handle (self-carries its game)
	L.SetGlobal("hero", ud)

	// Getter: Unit_Position(hero) == the unit's live sim position, Go==Lua.
	if err := L.DoString(`p = Unit_Position(hero)`); err != nil {
		t.Fatalf("Unit_Position script: %v", err)
	}
	pt := L.GetGlobal("p").(*lua.LTable)
	px := float64(pt.RawGetString("x").(lua.LNumber))
	py := float64(pt.RawGetString("y").(lua.LNumber))
	want := u.Position()
	t.Logf("FSV Unit_Position: sim=%+v Lua={%v,%v}", want, px, py)
	if px != want.X || py != want.Y || px != 100 || py != 200 {
		t.Fatalf("Unit_Position: Lua={%v,%v} sim=%+v want {100,200}", px, py, want)
	}

	// Setter: Unit_SetLife(hero, 50) must mutate the sim — read back via Go.
	before := u.Life()
	if err := L.DoString(`Unit_SetLife(hero, 50)`); err != nil {
		t.Fatalf("Unit_SetLife script: %v", err)
	}
	after := u.Life()
	t.Logf("FSV Unit_SetLife via Lua: sim Life before=%v after=%v (SoT = sim health store)", before, after)
	if after != 50 {
		t.Fatalf("Unit_SetLife(50): sim Life=%v, want 50 (Lua did not mutate sim)", after)
	}

	// Fail-closed: a non-Unit userdata raises, never mis-casts to a zero Unit.
	bad := L.NewUserData()
	bad.Value = 42
	L.SetGlobal("notunit", bad)
	if err := L.DoString(`Unit_Position(notunit)`); err == nil || !strings.Contains(err.Error(), "expected Unit") {
		t.Fatalf("wrong-type userdata must raise 'expected Unit', got %v", err)
	}
	t.Logf("FSV fail-closed: non-Unit userdata rejected")
}

func TestGeneratedPlayerBindingsFSV(t *testing.T) {
	g, err := api.NewGame(api.GameOptions{MaxUnits: 16, Seed: 5})
	if err != nil {
		t.Fatalf("NewGame: %v", err)
	}
	if err := g.DefineUnits([]data.Unit{
		{ID: "hfoo", Life: 100, MoveSpeedPerTick: 8 * fixed.One, TurnRatePerTick: 65535, CollisionSize: 16},
	}); err != nil {
		t.Fatalf("DefineUnits: %v", err)
	}
	u := g.CreateUnit(g.Player(1), g.UnitType("hfoo"), api.Vec2{X: 10, Y: 10}, api.Deg(0))

	L := lua.NewState()
	defer L.Close()
	if err := Register(L, g); err != nil {
		t.Fatalf("Register: %v", err)
	}
	hero := L.NewUserData()
	hero.Value = u
	L.SetGlobal("hero", hero)
	p2ud := L.NewUserData()
	p2ud.Value = g.Player(2) // a Player handle (self-carries game)
	L.SetGlobal("p2", p2ud)

	// Player getter parity: Player_Gold(p2) via Lua == the Go call (the
	// binding's contract is faithful forwarding; econ init in a bare game is a
	// separate api concern, gap #388).
	if err := L.DoString(`gold = Player_Gold(p2)`); err != nil {
		t.Fatalf("Player_Gold: %v", err)
	}
	if got, want := luaNum(t, L, "gold"), float64(g.Player(2).Gold()); got != want {
		t.Fatalf("Player_Gold parity: Lua=%v Go=%v", got, want)
	}
	t.Logf("FSV Player_Gold parity: Lua==Go==%d", g.Player(2).Gold())

	// Player-as-param: Unit_SetOwner(hero, p2, false) reassigns ownership in sim.
	if u.OwnedBy(g.Player(2)) {
		t.Fatal("precondition: unit should not start owned by player 2")
	}
	if err := L.DoString(`Unit_SetOwner(hero, p2, false)`); err != nil {
		t.Fatalf("Unit_SetOwner: %v", err)
	}
	if !u.OwnedBy(g.Player(2)) {
		t.Fatal("Unit_SetOwner(p2) did not reassign ownership in the sim")
	}
	// And the Lua-side ownership check agrees.
	if err := L.DoString(`owned = Unit_OwnedBy(hero, p2)`); err != nil {
		t.Fatalf("Unit_OwnedBy: %v", err)
	}
	if L.GetGlobal("owned") != lua.LTrue {
		t.Fatalf("Unit_OwnedBy(hero, p2) = %v, want true", L.GetGlobal("owned"))
	}
	t.Logf("FSV Player param: Unit_SetOwner via Lua -> sim ownership reassigned, Lua OwnedBy=true")
}

func TestGeneratedGameBoundBindingsFSV(t *testing.T) {
	g, err := api.NewGame(api.GameOptions{MaxUnits: 16, Seed: 1})
	if err != nil {
		t.Fatalf("NewGame: %v", err)
	}
	L := lua.NewState()
	defer L.Close()
	if err := Register(L, g); err != nil {
		t.Fatalf("Register: %v", err)
	}

	// A Game-receiver verb is bound to g (no game arg from Lua). Game_NeutralHostile()
	// returns a Player; its slot must equal the Go call's (the binder used b.g).
	if err := L.DoString(`np = Game_NeutralHostile()`); err != nil {
		t.Fatalf("Game_NeutralHostile script: %v", err)
	}
	npud, ok := L.GetGlobal("np").(*lua.LUserData)
	if !ok {
		t.Fatalf("Game_NeutralHostile returned %s, want Player userdata", L.GetGlobal("np").Type())
	}
	p, ok := npud.Value.(api.Player)
	if !ok {
		t.Fatalf("returned userdata holds %T, not api.Player", npud.Value)
	}
	t.Logf("FSV Game_NeutralHostile: Lua player slot=%d, Go slot=%d", p.Slot(), g.NeutralHostile().Slot())
	if p.Slot() != g.NeutralHostile().Slot() {
		t.Fatalf("Game_NeutralHostile slot via Lua=%d != Go=%d", p.Slot(), g.NeutralHostile().Slot())
	}

	// Slice return: Game_Allies(p) yields a Lua array table of Player userdata
	// whose length equals the Go call's slice length (parity), and element 1 is
	// a Player userdata.
	p1 := L.NewUserData()
	p1.Value = g.Player(1)
	L.SetGlobal("p1", p1)
	if err := L.DoString(`al = Game_Allies(p1); n = #al`); err != nil {
		t.Fatalf("Game_Allies script: %v", err)
	}
	goAllies := g.Allies(g.Player(1))
	if got := int(luaNum(t, L, "n")); got != len(goAllies) {
		t.Fatalf("Game_Allies length via Lua=%d != Go=%d", got, len(goAllies))
	}
	altbl := L.GetGlobal("al").(*lua.LTable)
	if len(goAllies) > 0 {
		if _, ok := altbl.RawGetInt(1).(*lua.LUserData); !ok {
			t.Fatalf("Game_Allies[1] is %s, want Player userdata", altbl.RawGetInt(1).Type())
		}
	}
	t.Logf("FSV Game_Allies slice: Lua len=%d == Go len=%d", int(luaNum(t, L, "n")), len(goAllies))

	// Game-receiver verbs are gated on a game: Register(L2, nil) installs none.
	L2 := lua.NewState()
	defer L2.Close()
	if err := Register(L2, nil); err != nil {
		t.Fatalf("Register(nil): %v", err)
	}
	if L2.GetGlobal("Game_NeutralHostile") != lua.LNil {
		t.Fatal("Game_NeutralHostile installed without a game (should be gated on g != nil)")
	}
	// ...but no-game value verbs ARE installed even without a game.
	if L2.GetGlobal("Vec2_Add") == lua.LNil {
		t.Fatal("Vec2_Add should be installed even without a game")
	}
	t.Logf("FSV game-gating: Game_* absent without a game; value/handle verbs present")
}

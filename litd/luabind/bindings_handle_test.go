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

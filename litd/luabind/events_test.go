package luabind

// FSV for the OnEvent Lua handler bridge (#269). SoT = whether a Lua handler
// registered through OnEvent actually fires when the sim emits the event, and
// stops firing after Cancel. The trigger is a real sim event (a unit death
// emitted during a Step), observed via Lua-side state the handler mutates.

import (
	"strconv"
	"strings"
	"testing"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	lua "github.com/yuin/gopher-lua"
)

const eventUnitDeath = 1 // api.EventUnitDeath

func eventGame(t *testing.T) (*api.Game, *lua.LState) {
	t.Helper()
	g, err := api.NewGame(api.GameOptions{MaxUnits: 16, Seed: 37})
	if err != nil {
		t.Fatalf("NewGame: %v", err)
	}
	if err := g.DefineUnits([]data.Unit{
		{ID: "hfoo", Life: 100, MoveSpeedPerTick: 8 * fixed.One, TurnRatePerTick: 65535, CollisionSize: 16},
	}); err != nil {
		t.Fatalf("DefineUnits: %v", err)
	}
	L := lua.NewState()
	if err := Register(L, g); err != nil {
		t.Fatalf("Register: %v", err)
	}
	return g, L
}

func TestLuaOnEventFiresFSV(t *testing.T) {
	g, L := eventGame(t)
	defer L.Close()

	// A Lua handler counts unit-death events.
	if err := L.DoString(`deaths = 0
		OnEvent(` + strconv.Itoa(eventUnitDeath) + `, function() deaths = deaths + 1 end)`); err != nil {
		t.Fatalf("OnEvent register: %v", err)
	}
	if got := luaNum(t, L, "deaths"); got != 0 {
		t.Fatalf("deaths should start at 0, got %v", got)
	}

	// Trigger: kill a unit, then step the sim so the death event emits.
	u := g.CreateUnit(g.Player(1), g.UnitType("hfoo"), api.Vec2{X: 0, Y: 0}, api.Deg(0))
	u.Kill()
	g.Advance(1)

	if got := luaNum(t, L, "deaths"); got != 1 {
		t.Fatalf("after kill+Advance: Lua deaths=%v, want 1 (handler did not fire)", got)
	}
	t.Logf("FSV OnEvent: sim unit death fired the Lua handler (deaths=1)")
}

func TestLuaOnEventCancelFSV(t *testing.T) {
	g, L := eventGame(t)
	defer L.Close()

	if err := L.DoString(`deaths = 0
		sub = OnEvent(` + strconv.Itoa(eventUnitDeath) + `, function() deaths = deaths + 1 end)`); err != nil {
		t.Fatalf("OnEvent register: %v", err)
	}

	// First death: handler fires.
	u1 := g.CreateUnit(g.Player(1), g.UnitType("hfoo"), api.Vec2{X: 0, Y: 0}, api.Deg(0))
	u1.Kill()
	g.Advance(1)
	if got := luaNum(t, L, "deaths"); got != 1 {
		t.Fatalf("first death: deaths=%v, want 1", got)
	}

	// Cancel from Lua, then a second death must NOT fire the handler.
	if err := L.DoString(`Cancel(sub)`); err != nil {
		t.Fatalf("Cancel: %v", err)
	}
	u2 := g.CreateUnit(g.Player(1), g.UnitType("hfoo"), api.Vec2{X: 1, Y: 0}, api.Deg(0))
	u2.Kill()
	g.Advance(1)
	if got := luaNum(t, L, "deaths"); got != 1 {
		t.Fatalf("after Cancel: deaths=%v, want still 1 (cancelled handler fired)", got)
	}
	t.Logf("FSV OnEvent Cancel: handler fired once, silent after Cancel (deaths=1)")
}

func TestLuaOnEventReadsEventFSV(t *testing.T) {
	// The handler must be able to INSPECT the event: Event_Unit(ev) returns the
	// dying unit, and reading its position through Unit_Position from Lua yields
	// the unit's real sim position. SoT = the spawn position the handler reads
	// back during dispatch.
	g, L := eventGame(t)
	defer L.Close()

	if err := L.DoString(`vx = -1; vy = -1
		OnEvent(` + strconv.Itoa(eventUnitDeath) + `, function(ev)
			local p = Unit_Position(Event_Unit(ev))
			vx = p.x; vy = p.y
		end)`); err != nil {
		t.Fatalf("OnEvent register: %v", err)
	}

	u := g.CreateUnit(g.Player(1), g.UnitType("hfoo"), api.Vec2{X: 64, Y: 96}, api.Deg(0))
	u.Kill()
	g.Advance(1)

	if gx, gy := luaNum(t, L, "vx"), luaNum(t, L, "vy"); gx != 64 || gy != 96 {
		t.Fatalf("handler read dying unit pos via Lua = {%v,%v}, want {64,96}", gx, gy)
	}
	t.Logf("FSV OnEvent read: handler read Event_Unit's real position {64,96} during dispatch")
}

func TestLuaGoEventOrderFSV(t *testing.T) {
	// #269 requirement: Lua OnEvent handlers join the SAME ordered subscriber
	// list as Go handlers and dispatch in global registration order. Register
	// G1 (Go), L (Lua), G2 (Go) for the same kind; the firing order must be
	// exactly G1, L, G2.
	g, L := eventGame(t)
	defer L.Close()

	var trace []string
	L.SetGlobal("mark", L.NewFunction(func(L *lua.LState) int {
		trace = append(trace, L.CheckString(1))
		return 0
	}))

	g.OnEvent(api.EventKind(eventUnitDeath), func(api.Event) { trace = append(trace, "G1") })
	if err := L.DoString(`OnEvent(` + strconv.Itoa(eventUnitDeath) + `, function() mark("L") end)`); err != nil {
		t.Fatalf("Lua OnEvent: %v", err)
	}
	g.OnEvent(api.EventKind(eventUnitDeath), func(api.Event) { trace = append(trace, "G2") })

	u := g.CreateUnit(g.Player(1), g.UnitType("hfoo"), api.Vec2{X: 0, Y: 0}, api.Deg(0))
	u.Kill()
	g.Advance(1)

	got := strings.Join(trace, ",")
	t.Logf("FSV mixed Go/Lua event order: [%s], want [G1,L,G2]", got)
	if got != "G1,L,G2" {
		t.Fatalf("Go/Lua handler dispatch order = [%s], want [G1,L,G2]", got)
	}
}

func TestLuaOnEventHandlerErrorSurfacedFSV(t *testing.T) {
	g, L := eventGame(t)
	defer L.Close()
	var errs []string
	OnScriptError(L, func(e error) { errs = append(errs, e.Error()) })

	if err := L.DoString(`OnEvent(` + strconv.Itoa(eventUnitDeath) + `, function() error("handler-boom") end)`); err != nil {
		t.Fatalf("OnEvent register: %v", err)
	}
	u := g.CreateUnit(g.Player(1), g.UnitType("hfoo"), api.Vec2{X: 0, Y: 0}, api.Deg(0))
	u.Kill()
	g.Advance(1)
	if len(errs) != 1 {
		t.Fatalf("handler error not surfaced: handler errors=%d", len(errs))
	}
	t.Logf("FSV OnEvent error surfaced: %q", errs[0])
}

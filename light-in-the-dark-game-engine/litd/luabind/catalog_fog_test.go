package luabind

// Fog bindings (#267): Game_SetFogState / Game_NewFogModifier + FogModifier
// Start/Stop/Destroy — the write side of fog-of-war, which the generated
// dispatch defers wholesale (no api.Area arg marshaler, FogModifier not in the
// pushHandle set). SoT = the sim fog grid, read via the Go api Game.FogStateAt
// (and cross-checked through the already-generated Lua Game_FogStateAt).

import (
	"testing"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	lua "github.com/yuin/gopher-lua"
)

func TestFogBindingsFSV(t *testing.T) {
	g, _ := confGame(t, 17)
	p1 := g.Player(1)
	L := lua.NewState()
	defer L.Close()
	if err := Register(L, g); err != nil {
		t.Fatalf("Register: %v", err)
	}
	ud := L.NewUserData()
	ud.Value = p1
	L.SetGlobal("p1", ud)

	inside := api.Vec2{X: 200, Y: 200} // inside the stamped rect
	const visible = int(api.FogVisible) // enum: Masked=0, Fogged=1, Visible=2

	// BEFORE: capture the default fog state at the target point (do not assume).
	before := g.FogStateAt(p1, inside)
	t.Logf("BEFORE SetFogState: FogStateAt(p1,{200,200}) = %d", int(before))

	// SetFogState stamps immediately (no tick). Stamp Visible over a rect
	// containing the point, then read the grid back.
	if err := L.DoString(`Game_SetFogState(p1, 2, {minx=0, miny=0, maxx=400, maxy=400}, false)`); err != nil {
		t.Fatalf("Game_SetFogState: %v", err)
	}
	after := g.FogStateAt(p1, inside)
	if int(after) != visible {
		t.Fatalf("SetFogState SoT: FogStateAt(p1,{200,200}) = %d after stamping Visible, want %d", int(after), visible)
	}
	// Cross-check: the generated Lua read agrees with the Go read.
	if err := L.DoString(`_fs = Game_FogStateAt(p1, {x=200, y=200})`); err != nil {
		t.Fatalf("Game_FogStateAt: %v", err)
	}
	if luaFS := int(lua.LVAsNumber(L.GetGlobal("_fs"))); luaFS != int(after) {
		t.Fatalf("Lua Game_FogStateAt = %d, Go = %d — disagree", luaFS, int(after))
	}
	t.Logf("AFTER SetFogState: Go=%d Lua=%d (Visible) — match", int(after), int(after))

	// Persistent modifier. Fog modifiers are overlaid by the visibility system on
	// its recompute cadence (tick % Interval()==0, Interval=5), so the FSV
	// advances past a cadence boundary to observe each lifecycle transition.
	pt := api.Vec2{X: 1000, Y: 1000}
	if err := L.DoString(`_m = Game_NewFogModifier(p1, 2, {cx=1000, cy=1000, radius=200})
		_mvalid = Valid(_m)`); err != nil {
		t.Fatalf("Game_NewFogModifier: %v", err)
	}
	if !lua.LVAsBool(L.GetGlobal("_mvalid")) {
		t.Fatal("NewFogModifier returned an invalid handle")
	}
	// Created stopped: advancing the cadence must NOT reveal the point (teeth).
	g.Advance(10)
	if st := g.FogStateAt(p1, pt); int(st) == visible {
		t.Fatalf("created-stopped modifier revealed the point: %d", int(st))
	}
	if err := L.DoString(`FogModifier_Start(_m)`); err != nil {
		t.Fatalf("FogModifier_Start: %v", err)
	}
	g.Advance(10) // cross a recompute boundary so the started modifier overlays
	afterStart := g.FogStateAt(p1, pt)
	if int(afterStart) != visible {
		t.Fatalf("modifier SoT: FogStateAt(p1,{1000,1000}) = %d after Start, want %d", int(afterStart), visible)
	}
	t.Logf("modifier: created-stopped masked, after Start+cadence=%d (Visible)", int(afterStart))

	// Stop reverts (the point is now explored → Fogged, not Visible).
	if err := L.DoString(`FogModifier_Stop(_m)`); err != nil {
		t.Fatalf("FogModifier_Stop: %v", err)
	}
	g.Advance(10)
	afterStop := g.FogStateAt(p1, pt)
	if int(afterStop) == visible {
		t.Fatalf("modifier SoT: FogStateAt still Visible (%d) after Stop — Stop did not revert", int(afterStop))
	}
	t.Logf("modifier: after Stop+cadence=%d (reverted from Visible)", int(afterStop))

	// Destroy invalidates the handle.
	if err := L.DoString(`FogModifier_Destroy(_m)
		_mvalid2 = Valid(_m)`); err != nil {
		t.Fatalf("FogModifier_Destroy: %v", err)
	}
	if lua.LVAsBool(L.GetGlobal("_mvalid2")) {
		t.Fatal("FogModifier still Valid after Destroy")
	}

	// Fail-closed: an Area table with neither rect nor circle keys raises.
	if err := L.DoString(`Game_SetFogState(p1, 2, {foo=1}, false)`); err == nil {
		t.Fatal("SetFogState with a malformed Area must raise")
	}
	t.Logf("FSV #267 fog: SetFogState stamps Visible (Go+Lua agree); modifier Start applies / Stop reverts / Destroy invalidates; malformed Area raised")
}

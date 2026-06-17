package luabind

// FSV for the binding runtime ABI + value-math reference bindings (#267). SoT =
// the value a Lua script gets back from each binding, asserted to be (a) the
// known X+X=Y result and (b) BIT-IDENTICAL to calling the same api verb in Go
// (the Go-vs-Lua conformance property the full suite, step 4, generalizes).
// Plus the fail-closed edge: a malformed argument raises, never coerces.

import (
	"testing"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	lua "github.com/yuin/gopher-lua"
)

func luaNum(t *testing.T, L *lua.LState, global string) float64 {
	t.Helper()
	v := L.GetGlobal(global)
	n, ok := v.(lua.LNumber)
	if !ok {
		t.Fatalf("global %q is %s, want number", global, v.Type())
	}
	return float64(n)
}

func TestRegisterValueMathFSV(t *testing.T) {
	L := lua.NewState()
	defer L.Close()
	if err := Register(L, nil); err != nil {
		t.Fatalf("Register: %v", err)
	}

	// Vec2.DistanceTo — known 3-4-5, and Go==Lua.
	wantD := api.Vec2{X: 3, Y: 4}.DistanceTo(api.Vec2{X: 0, Y: 0})
	if err := L.DoString(`result = Vec2_DistanceTo({x=3,y=4},{x=0,y=0})`); err != nil {
		t.Fatalf("DistanceTo script: %v", err)
	}
	gotD := luaNum(t, L, "result")
	t.Logf("FSV DistanceTo: Go=%v Lua=%v (want 5)", wantD, gotD)
	if gotD != wantD || gotD != 5 {
		t.Fatalf("DistanceTo: Lua=%v Go=%v want 5", gotD, wantD)
	}

	// Vec2.Add — Go==Lua, known {4,6}.
	wantAdd := api.Vec2{X: 1, Y: 2}.Add(api.Vec2{X: 3, Y: 4})
	if err := L.DoString(`r = Vec2_Add({x=1,y=2},{x=3,y=4})`); err != nil {
		t.Fatalf("Add script: %v", err)
	}
	rt, ok := L.GetGlobal("r").(*lua.LTable)
	if !ok {
		t.Fatalf("Add result is not a table")
	}
	gx := float64(rt.RawGetString("x").(lua.LNumber))
	gy := float64(rt.RawGetString("y").(lua.LNumber))
	t.Logf("FSV Add: Go=%+v Lua={%v,%v} (want {4,6})", wantAdd, gx, gy)
	if gx != wantAdd.X || gy != wantAdd.Y || gx != 4 || gy != 6 {
		t.Fatalf("Add: Lua={%v,%v} Go=%+v want {4,6}", gx, gy, wantAdd)
	}

	// Vec2.AngleTo — Go==Lua (both via fixed-point), east heading.
	wantAng := api.Vec2{X: 0, Y: 0}.AngleTo(api.Vec2{X: 1, Y: 0}).Degrees()
	if err := L.DoString(`result = Vec2_AngleTo({x=0,y=0},{x=1,y=0})`); err != nil {
		t.Fatalf("AngleTo script: %v", err)
	}
	gotAng := luaNum(t, L, "result")
	t.Logf("FSV AngleTo(east): Go=%v Lua=%v", wantAng, gotAng)
	if gotAng != wantAng {
		t.Fatalf("AngleTo: Lua=%v != Go=%v", gotAng, wantAng)
	}

	// Vec2.Polar — Go==Lua (fixed-point sin/cos), heading 90° dist 10.
	wantPolar := api.Vec2{X: 0, Y: 0}.Polar(api.Deg(90), 10)
	if err := L.DoString(`p = Vec2_Polar({x=0,y=0}, 90, 10)`); err != nil {
		t.Fatalf("Polar script: %v", err)
	}
	pt := L.GetGlobal("p").(*lua.LTable)
	px := float64(pt.RawGetString("x").(lua.LNumber))
	py := float64(pt.RawGetString("y").(lua.LNumber))
	t.Logf("FSV Polar(90°,10): Go=%+v Lua={%v,%v}", wantPolar, px, py)
	if px != wantPolar.X || py != wantPolar.Y {
		t.Fatalf("Polar: Lua={%v,%v} != Go=%+v", px, py, wantPolar)
	}

	// Angle.Degrees — round-trip 90 -> 90.
	if err := L.DoString(`result = Angle_Degrees(90)`); err != nil {
		t.Fatalf("Degrees script: %v", err)
	}
	gotDeg := luaNum(t, L, "result")
	t.Logf("FSV Angle_Degrees(90): Lua=%v", gotDeg)
	if gotDeg != 90 {
		t.Fatalf("Angle_Degrees(90) = %v, want 90", gotDeg)
	}

	// Edge (fail-closed): a malformed Vec2 argument raises, never coerces y=0.
	if err := L.DoString(`Vec2_DistanceTo({x=3}, {x=0,y=0})`); err == nil {
		t.Fatal("malformed Vec2 arg must raise a Lua error, got nil")
	} else {
		t.Logf("FSV fail-closed: malformed arg -> %v", err)
	}
}

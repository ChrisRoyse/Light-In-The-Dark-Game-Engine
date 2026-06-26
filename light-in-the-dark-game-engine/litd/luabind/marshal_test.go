package luabind

// FSV for value-type marshaling (#267 step 2). SoT = the actual *lua.LTable
// field contents (Go->Lua) and the decoded Go value (Lua->Go), under the
// X+X=Y discipline (known input -> known output), plus the fail-closed edges:
// a malformed Lua value must error, never coerce to a zero value.

import (
	"math"
	"strings"
	"testing"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	lua "github.com/yuin/gopher-lua"
)

func TestMarshalVec2RoundTrip(t *testing.T) {
	L := lua.NewState()
	defer L.Close()

	// Go -> Lua. SoT = the table's x/y fields.
	in := api.Vec2{X: 3, Y: 4}
	tbl := vec2ToLua(L, in)
	gx, gy := tbl.RawGetString("x"), tbl.RawGetString("y")
	t.Logf("FSV Vec2->Lua: table x=%v y=%v", gx, gy)
	if gx != lua.LNumber(3) || gy != lua.LNumber(4) {
		t.Fatalf("vec2ToLua = {x:%v y:%v}, want {3,4}", gx, gy)
	}

	// Lua -> Go. SoT = the decoded value.
	out, err := luaToVec2(tbl)
	if err != nil {
		t.Fatalf("luaToVec2: %v", err)
	}
	if out != in {
		t.Fatalf("round-trip %v != %v", out, in)
	}

	// Edge: missing component (off-by-one bait — y absent).
	bad := L.NewTable()
	bad.RawSetString("x", lua.LNumber(1))
	if _, err := luaToVec2(bad); err == nil || !strings.Contains(err.Error(), `"y"`) {
		t.Fatalf("missing y must fail naming y, got %v", err)
	}
	// Edge: not a table.
	if _, err := luaToVec2(lua.LString("nope")); err == nil {
		t.Fatal("non-table must fail")
	}
	// Edge: wrong field type (string where number expected).
	bad2 := L.NewTable()
	bad2.RawSetString("x", lua.LString("a"))
	bad2.RawSetString("y", lua.LNumber(2))
	if _, err := luaToVec2(bad2); err == nil || !strings.Contains(err.Error(), "want number") {
		t.Fatalf("non-numeric x must fail, got %v", err)
	}
	t.Logf("FSV Vec2 edges: missing field / non-table / wrong field type all fail closed")
}

func TestMarshalRectRoundTrip(t *testing.T) {
	L := lua.NewState()
	defer L.Close()

	in := api.Rect{MinX: -1, MinY: -2, MaxX: 3, MaxY: 4}
	tbl := rectToLua(L, in)
	t.Logf("FSV Rect->Lua: minx=%v miny=%v maxx=%v maxy=%v",
		tbl.RawGetString("minx"), tbl.RawGetString("miny"),
		tbl.RawGetString("maxx"), tbl.RawGetString("maxy"))
	if tbl.RawGetString("minx") != lua.LNumber(-1) || tbl.RawGetString("maxy") != lua.LNumber(4) {
		t.Fatalf("rectToLua bounds wrong: %v", tbl)
	}
	out, err := luaToRect(tbl)
	if err != nil {
		t.Fatalf("luaToRect: %v", err)
	}
	if out != in {
		t.Fatalf("round-trip %v != %v", out, in)
	}
	// Edge: one bound missing (maxy).
	bad := L.NewTable()
	for _, k := range []string{"minx", "miny", "maxx"} {
		bad.RawSetString(k, lua.LNumber(0))
	}
	if _, err := luaToRect(bad); err == nil || !strings.Contains(err.Error(), `"maxy"`) {
		t.Fatalf("missing maxy must fail naming maxy, got %v", err)
	}
	t.Logf("FSV Rect edges: missing bound fails closed")
}

func TestMarshalAngleRoundTrip(t *testing.T) {
	// X+X=Y: 90 degrees -> pi/2 radians, and back to 90.
	for _, deg := range []float64{0, 90, 180, 360, -45} {
		n := angleToLua(api.Deg(deg))
		if math.Abs(float64(n)-deg) > 1e-9 {
			t.Fatalf("angleToLua(Deg(%v)) = %v, want %v deg", deg, n, deg)
		}
		got, err := luaToAngle(n)
		if err != nil {
			t.Fatalf("luaToAngle(%v): %v", n, err)
		}
		wantRad := deg * math.Pi / 180
		if math.Abs(got.Radians()-wantRad) > 1e-9 {
			t.Fatalf("luaToAngle(%v) = %v rad, want %v rad", n, got.Radians(), wantRad)
		}
		t.Logf("FSV Angle: %v deg <-> %v rad round-trip", deg, got.Radians())
	}
	// Edge: non-number must fail.
	if _, err := luaToAngle(lua.LString("90")); err == nil || !strings.Contains(err.Error(), "want number") {
		t.Fatalf("string angle must fail, got %v", err)
	}
	if _, err := luaToAngle(lua.LNil); err == nil {
		t.Fatal("nil angle must fail")
	}
	t.Logf("FSV Angle edges: non-number / nil fail closed")
}

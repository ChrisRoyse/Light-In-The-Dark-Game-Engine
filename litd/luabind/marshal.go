package luabind

// Value-type marshaling for the Lua binding layer (#267 step 2). The generated
// bindings move api value types across the Go<->Lua boundary through these
// converters. Per R-API-1..6 the public API exposes no G3N/sim types, so the
// only things that cross are: value types (Vec2/Rect/Angle — mapped to Lua
// tables/numbers here), opaque handles (userdata, via the HandleMarshaler seam
// — see persist_thread.go; the game-backed impl is gated on api.NewGame, #386),
// and primitives (number/string/bool, handled inline by the generator).
//
// Marshaling is fail-closed: a malformed or wrong-typed Lua value is a loud
// error, never a zero-value coercion. A script passing {x=1} where a Vec2 is
// required must fail at the boundary, not silently read y as 0.

import (
	"fmt"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	lua "github.com/yuin/gopher-lua"
)

// tableNumber reads a required numeric field from a Lua table, failing loudly if
// the field is absent or not a number (fail-closed: no silent default to 0).
func tableNumber(t *lua.LTable, key string) (float64, error) {
	v := t.RawGetString(key)
	switch n := v.(type) {
	case lua.LNumber:
		return float64(n), nil
	case *lua.LNilType:
		return 0, fmt.Errorf("missing required numeric field %q", key)
	default:
		return 0, fmt.Errorf("field %q is %s, want number", key, v.Type())
	}
}

// vec2ToLua encodes a Vec2 as the Lua table {x=, y=}.
func vec2ToLua(L *lua.LState, v api.Vec2) *lua.LTable {
	t := L.NewTable()
	t.RawSetString("x", lua.LNumber(v.X))
	t.RawSetString("y", lua.LNumber(v.Y))
	return t
}

// luaToVec2 decodes {x=, y=} into a Vec2, failing loudly on a non-table or a
// missing/non-numeric component.
func luaToVec2(lv lua.LValue) (api.Vec2, error) {
	t, ok := lv.(*lua.LTable)
	if !ok {
		return api.Vec2{}, fmt.Errorf("Vec2: want table, got %s", lv.Type())
	}
	x, err := tableNumber(t, "x")
	if err != nil {
		return api.Vec2{}, fmt.Errorf("Vec2: %w", err)
	}
	y, err := tableNumber(t, "y")
	if err != nil {
		return api.Vec2{}, fmt.Errorf("Vec2: %w", err)
	}
	return api.Vec2{X: x, Y: y}, nil
}

// rectToLua encodes a Rect as {minx=, miny=, maxx=, maxy=}.
func rectToLua(L *lua.LState, r api.Rect) *lua.LTable {
	t := L.NewTable()
	t.RawSetString("minx", lua.LNumber(r.MinX))
	t.RawSetString("miny", lua.LNumber(r.MinY))
	t.RawSetString("maxx", lua.LNumber(r.MaxX))
	t.RawSetString("maxy", lua.LNumber(r.MaxY))
	return t
}

// luaToRect decodes {minx=, miny=, maxx=, maxy=} into a Rect, failing loudly on
// a non-table or any missing/non-numeric bound.
func luaToRect(lv lua.LValue) (api.Rect, error) {
	t, ok := lv.(*lua.LTable)
	if !ok {
		return api.Rect{}, fmt.Errorf("Rect: want table, got %s", lv.Type())
	}
	var out api.Rect
	for _, f := range []struct {
		key string
		dst *float64
	}{
		{"minx", &out.MinX}, {"miny", &out.MinY},
		{"maxx", &out.MaxX}, {"maxy", &out.MaxY},
	} {
		n, err := tableNumber(t, f.key)
		if err != nil {
			return api.Rect{}, fmt.Errorf("Rect: %w", err)
		}
		*f.dst = n
	}
	return out, nil
}

// angleToLua encodes an Angle as a Lua number in DEGREES — JASS facing reals are
// degrees, and degrees is the unit a script author expects (the radians/degrees
// confusion ends at this boundary, mirroring api.Deg/Degrees).
func angleToLua(a api.Angle) lua.LNumber {
	return lua.LNumber(a.Degrees())
}

// luaToAngle decodes a Lua number (degrees) into an Angle, failing loudly on a
// non-number.
func luaToAngle(lv lua.LValue) (api.Angle, error) {
	n, ok := lv.(lua.LNumber)
	if !ok {
		return api.Angle{}, fmt.Errorf("Angle: want number (degrees), got %s", lv.Type())
	}
	return api.Deg(float64(n)), nil
}

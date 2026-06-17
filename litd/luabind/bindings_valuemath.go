package luabind

// bindings_valuemath.go — hand-written REFERENCE bindings for the Vec2/Angle
// value-math verbs (#267). These follow the register.go ABI exactly and are the
// code shape the jassgen -luabind generator will emit; until then they make the
// runtime real and end-to-end testable. Each binding marshals through
// marshal.go and fails closed on a bad argument (L.ArgError), never coercing.

import (
	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	lua "github.com/yuin/gopher-lua"
)

// registerValueMath installs the value-math globals. Names follow the canonical
// "Type_Method" convention (luaBindingName in the generator).
func registerValueMath(L *lua.LState) {
	L.SetGlobal("Vec2_Add", L.NewFunction(bindVec2Add))
	L.SetGlobal("Vec2_AngleTo", L.NewFunction(bindVec2AngleTo))
	L.SetGlobal("Vec2_DistanceTo", L.NewFunction(bindVec2DistanceTo))
	L.SetGlobal("Vec2_Polar", L.NewFunction(bindVec2Polar))
	L.SetGlobal("Angle_Degrees", L.NewFunction(bindAngleDegrees))
}

// argVec2 reads argument i as a Vec2, raising a Lua arg error on a malformed
// value (fail-closed).
func argVec2(L *lua.LState, i int) api.Vec2 {
	v, err := luaToVec2(L.Get(i))
	if err != nil {
		L.ArgError(i, err.Error())
	}
	return v
}

// argAngle reads argument i as an Angle (degrees), raising on a non-number.
func argAngle(L *lua.LState, i int) api.Angle {
	a, err := luaToAngle(L.Get(i))
	if err != nil {
		L.ArgError(i, err.Error())
	}
	return a
}

// Vec2.Add(o Vec2) Vec2 — Lua: Vec2_Add(self, o) -> {x,y}
func bindVec2Add(L *lua.LState) int {
	self := argVec2(L, 1)
	o := argVec2(L, 2)
	L.Push(vec2ToLua(L, self.Add(o)))
	return 1
}

// Vec2.AngleTo(o Vec2) Angle — Lua: Vec2_AngleTo(self, o) -> degrees
func bindVec2AngleTo(L *lua.LState) int {
	self := argVec2(L, 1)
	o := argVec2(L, 2)
	L.Push(angleToLua(self.AngleTo(o)))
	return 1
}

// Vec2.DistanceTo(o Vec2) float64 — Lua: Vec2_DistanceTo(self, o) -> number
func bindVec2DistanceTo(L *lua.LState) int {
	self := argVec2(L, 1)
	o := argVec2(L, 2)
	L.Push(lua.LNumber(self.DistanceTo(o)))
	return 1
}

// Vec2.Polar(a Angle, dist float64) Vec2 — Lua: Vec2_Polar(self, degrees, dist) -> {x,y}
func bindVec2Polar(L *lua.LState) int {
	self := argVec2(L, 1)
	a := argAngle(L, 2)
	dist := float64(L.CheckNumber(3))
	L.Push(vec2ToLua(L, self.Polar(a, dist)))
	return 1
}

// Angle.Degrees() float64 — Lua: Angle_Degrees(self) -> number
func bindAngleDegrees(L *lua.LState) int {
	self := argAngle(L, 1)
	L.Push(lua.LNumber(self.Degrees()))
	return 1
}

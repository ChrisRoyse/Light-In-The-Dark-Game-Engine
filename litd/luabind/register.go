package luabind

// register.go is the Lua binding runtime ABI (#267): the seam that installs the
// api surface as Lua globals on an LState, and the conventions the generated
// dispatch follows.
//
// Binding ABI (what every binding function obeys):
//   - A binding is a gopher-lua `func(*lua.LState) int`: it reads its arguments
//     from the Lua stack (1-based), calls exactly one api verb, pushes its
//     results, and returns the result count.
//   - Argument 1 of a METHOD binding is the receiver (a value-type table for
//     Vec2/Angle, or a handle userdata for noun methods); free-function bindings
//     start their args at 1.
//   - Marshaling goes through marshal.go (value types) and GameHandles (handle
//     userdata) — never ad-hoc. A malformed argument raises a Lua arg error
//     (fail-closed), it is never coerced to a zero value.
//   - The bound *api.Game is captured by closure where a verb needs it (the
//     handle-bearing and free-function verbs); pure value-math verbs need none.
//
// bindings_valuemath.go is the hand-written REFERENCE implementation of this
// ABI for the Vec2/Angle value-math verbs — the exact code shape the
// jassgen -luabind generator will emit for the rest of the surface. It is kept
// small and explicit on purpose: it is the spec the generator is checked
// against.

import (
	"fmt"
	"time"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	lua "github.com/yuin/gopher-lua"
)

// Register installs the api binding surface as globals on L, bound to game g.
// g may be nil for the pure value-math verbs (they need no game); handle and
// free-function verbs require a non-nil g and are registered against it.
func Register(L *lua.LState, g *api.Game) error {
	registerGenerated(L) // value/handle/Player verbs (no game needed)
	if g != nil {
		registerGameBound(L, gameBinder{g: g}) // Game-receiver verbs, bound to g
		registerScriptThreads(L, g)            // Run/PolledWait cooperative threads (#269)
		registerScriptEvents(L, g)             // OnEvent/Cancel handler bridge (#269)
	}
	return nil
}

// gameBinder carries the bound game for the generated Game-receiver verb
// methods (bindings_dispatch_gen.go emits methods on this type using b.g). It
// lives here so the generated file imports only gopher-lua, never api.
type gameBinder struct{ g *api.Game }

// --- stable ABI argument readers (used by the generated dispatch) ---

// argRect reads argument i as a Rect, raising on a malformed value.
func argRect(L *lua.LState, i int) api.Rect {
	r, err := luaToRect(L.Get(i))
	if err != nil {
		L.ArgError(i, err.Error())
	}
	return r
}

// handleToLua wraps an api handle or Player in a fresh userdata carrying the
// value (which self-carries its *Game). The script receives an opaque handle it
// can pass to other verbs; GameHandles persists noun handles across a save
// (#264/#267).
func handleToLua(L *lua.LState, h any) *lua.LUserData {
	ud := L.NewUserData()
	ud.Value = h
	return ud
}

// handleSliceToLua marshals a slice of handles/Players to a 1-based Lua array
// table, each element wrapped as a userdata (the generated dispatch uses this
// for []Unit/[]Player/... returns).
func handleSliceToLua[T any](L *lua.LState, s []T) *lua.LTable {
	t := L.NewTable()
	for i, v := range s {
		ud := L.NewUserData()
		ud.Value = v
		t.RawSetInt(i+1, ud)
	}
	return t
}

// intSliceToLua / stringSliceToLua marshal primitive slices to Lua array tables.
func intSliceToLua[T ~int | ~int32 | ~int64 | ~uint32 | ~uint8](L *lua.LState, s []T) *lua.LTable {
	t := L.NewTable()
	for i, v := range s {
		t.RawSetInt(i+1, lua.LNumber(v))
	}
	return t
}

func stringSliceToLua(L *lua.LState, s []string) *lua.LTable {
	t := L.NewTable()
	for i, v := range s {
		t.RawSetInt(i+1, lua.LString(v))
	}
	return t
}

// Integer enum readers: the Lua surface passes the underlying integer constant
// (WC3-style: bj_RACE_*, etc.); the binding converts it to the typed enum. A
// non-number raises. Enum RETURNS go back as plain numbers (lua.LNumber), so no
// reader is needed for those.
func argRace(L *lua.LState, i int) api.Race                   { return api.Race(L.CheckInt(i)) }
func argDifficulty(L *lua.LState, i int) api.Difficulty       { return api.Difficulty(L.CheckInt(i)) }
func argFogState(L *lua.LState, i int) api.FogState           { return api.FogState(L.CheckInt(i)) }
func argController(L *lua.LState, i int) api.Controller       { return api.Controller(L.CheckInt(i)) }
func argAllianceFlags(L *lua.LState, i int) api.AllianceFlags { return api.AllianceFlags(L.CheckInt(i)) }
func argAbilityField(L *lua.LState, i int) api.AbilityField   { return api.AbilityField(L.CheckInt(i)) }
func argCameraField(L *lua.LState, i int) api.CameraField     { return api.CameraField(L.CheckInt(i)) }
func argAbilityRef(L *lua.LState, i int) api.AbilityRef       { return api.AbilityRef(L.CheckInt(i)) }

// argDuration reads argument i as a time.Duration expressed in SECONDS (the Lua
// surface uses seconds: PolledWait(0.5)), raising on a non-number.
func argDuration(L *lua.LState, i int) time.Duration {
	return time.Duration(float64(L.CheckNumber(i)) * float64(time.Second))
}

// argPlayer reads argument i as a Player userdata (self-carries its game).
func argPlayer(L *lua.LState, i int) api.Player {
	p, ok := handleArg(L, i).(api.Player)
	if !ok {
		L.ArgError(i, fmt.Sprintf("expected Player userdata, got %T", handleArg(L, i)))
	}
	return p
}

// handleArg reads argument i as a userdata and returns its payload, raising if i
// is not a userdata at all. The generated typed readers assert the concrete
// noun type on top of this.
func handleArg(L *lua.LState, i int) any {
	return L.CheckUserData(i).Value
}

func argUnit(L *lua.LState, i int) api.Unit {
	u, ok := handleArg(L, i).(api.Unit)
	if !ok {
		L.ArgError(i, fmt.Sprintf("expected Unit userdata, got %T", handleArg(L, i)))
	}
	return u
}

func argItem(L *lua.LState, i int) api.Item {
	v, ok := handleArg(L, i).(api.Item)
	if !ok {
		L.ArgError(i, fmt.Sprintf("expected Item userdata, got %T", handleArg(L, i)))
	}
	return v
}

func argDestructable(L *lua.LState, i int) api.Destructable {
	v, ok := handleArg(L, i).(api.Destructable)
	if !ok {
		L.ArgError(i, fmt.Sprintf("expected Destructable userdata, got %T", handleArg(L, i)))
	}
	return v
}

func argMissile(L *lua.LState, i int) api.Missile {
	v, ok := handleArg(L, i).(api.Missile)
	if !ok {
		L.ArgError(i, fmt.Sprintf("expected Missile userdata, got %T", handleArg(L, i)))
	}
	return v
}

func argEffect(L *lua.LState, i int) api.Effect {
	v, ok := handleArg(L, i).(api.Effect)
	if !ok {
		L.ArgError(i, fmt.Sprintf("expected Effect userdata, got %T", handleArg(L, i)))
	}
	return v
}

func argTimer(L *lua.LState, i int) api.Timer {
	v, ok := handleArg(L, i).(api.Timer)
	if !ok {
		L.ArgError(i, fmt.Sprintf("expected Timer userdata, got %T", handleArg(L, i)))
	}
	return v
}

// Id-ref value types (UnitType/ItemType/BuffType/Order) are tiny self-contained
// structs; they ride the same opaque-userdata seam as the noun handles.
func argUnitType(L *lua.LState, i int) api.UnitType {
	v, ok := handleArg(L, i).(api.UnitType)
	if !ok {
		L.ArgError(i, fmt.Sprintf("expected UnitType userdata, got %T", handleArg(L, i)))
	}
	return v
}

func argItemType(L *lua.LState, i int) api.ItemType {
	v, ok := handleArg(L, i).(api.ItemType)
	if !ok {
		L.ArgError(i, fmt.Sprintf("expected ItemType userdata, got %T", handleArg(L, i)))
	}
	return v
}

func argBuffType(L *lua.LState, i int) api.BuffType {
	v, ok := handleArg(L, i).(api.BuffType)
	if !ok {
		L.ArgError(i, fmt.Sprintf("expected BuffType userdata, got %T", handleArg(L, i)))
	}
	return v
}

func argOrder(L *lua.LState, i int) api.Order {
	v, ok := handleArg(L, i).(api.Order)
	if !ok {
		L.ArgError(i, fmt.Sprintf("expected Order userdata, got %T", handleArg(L, i)))
	}
	return v
}

// --- stable ABI argument readers (used by the generated dispatch) ---

// argVec2 reads argument i as a Vec2, raising a Lua arg error on a malformed
// value (fail-closed — never coerced to a zero vector).
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

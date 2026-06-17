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

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	lua "github.com/yuin/gopher-lua"
)

// Register installs the api binding surface as globals on L, bound to game g.
// g may be nil for the pure value-math verbs (they need no game); handle and
// free-function verbs require a non-nil g and are registered against it.
func Register(L *lua.LState, g *api.Game) error {
	registerGenerated(L) // generated: bindings_dispatch_gen.go
	// Game-bound free functions and Player/enum verbs install here once the
	// generator supports those type shapes (they need g threaded; the handle
	// and value verbs above do not — a handle userdata self-carries its game).
	return nil
}

// --- stable ABI argument readers (used by the generated dispatch) ---

// argRect reads argument i as a Rect, raising on a malformed value.
func argRect(L *lua.LState, i int) api.Rect {
	r, err := luaToRect(L.Get(i))
	if err != nil {
		L.ArgError(i, err.Error())
	}
	return r
}

// handleToLua wraps an api handle in a fresh userdata carrying the handle value
// (which self-carries its *Game). The script receives an opaque handle it can
// pass to other verbs; GameHandles persists it across a save (#264/#267).
func handleToLua(L *lua.LState, h api.Handle) *lua.LUserData {
	ud := L.NewUserData()
	ud.Value = h
	return ud
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

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
	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	lua "github.com/yuin/gopher-lua"
)

// Register installs the api binding surface as globals on L, bound to game g.
// g may be nil for the pure value-math verbs (they need no game); handle and
// free-function verbs require a non-nil g and are registered against it.
func Register(L *lua.LState, g *api.Game) error {
	registerValueMath(L)
	// Generated handle/free-function bindings (against GameHandles{g}) install
	// here once the jassgen -luabind generator lands.
	return nil
}

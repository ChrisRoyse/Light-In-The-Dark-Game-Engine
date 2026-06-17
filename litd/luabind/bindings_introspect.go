package luabind

// bindings_introspect.go — handle introspection bindings (#392). Valid(h) and
// IsZero(h) expose the universal predicates every api noun handle carries
// (R-API-5), so a script can honor modder-contract rule 2 — "if you wait, you
// resume on a later tick; everything may have changed, re-check your handles
// (Valid())" (execution-model.md §8). Before this, Valid()/IsZero() were
// unreachable from Lua and that contract was unfollowable from a script.
//
// They are polymorphic over every handle type via the Valid()/IsZero()
// interfaces, rather than 16+ per-type Type_Valid functions: the predicate is
// universal, and 8 of the handle types have no typed arg reader yet. A payload
// that does not implement the predicate (e.g. a value-type id-ref handed to
// Valid) is a loud Lua arg error, never a silent answer (fail-closed).
// Per-type aliases (Unit_Valid, …) can be added non-breakingly later if the ABI
// wants them. Hand-written (Go-idiom methods, no JASS origin, absent from the
// manifest); sibling to the generated dispatch, so regen stays byte-identical.

import (
	"fmt"

	lua "github.com/yuin/gopher-lua"
)

// bindValid implements Valid(h) -> bool for any handle userdata.
func bindValid(L *lua.LState) int {
	ud := L.CheckUserData(1)
	h, ok := ud.Value.(interface{ Valid() bool })
	if !ok {
		L.ArgError(1, fmt.Sprintf("Valid: %T has no Valid() predicate", ud.Value))
	}
	L.Push(lua.LBool(h.Valid()))
	return 1
}

// bindIsZero implements IsZero(h) -> bool for any handle userdata.
func bindIsZero(L *lua.LState) int {
	ud := L.CheckUserData(1)
	h, ok := ud.Value.(interface{ IsZero() bool })
	if !ok {
		L.ArgError(1, fmt.Sprintf("IsZero: %T has no IsZero() predicate", ud.Value))
	}
	L.Push(lua.LBool(h.IsZero()))
	return 1
}

// registerIntrospection installs the universal handle predicates. They need no
// game (a handle userdata self-carries its *Game), so they register alongside
// the generated value/handle verbs.
func registerIntrospection(L *lua.LState) {
	L.SetGlobal("Valid", L.NewFunction(bindValid))
	L.SetGlobal("IsZero", L.NewFunction(bindIsZero))
}

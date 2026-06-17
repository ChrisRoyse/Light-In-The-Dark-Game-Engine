// Package luabind is the embedding seam for the vendored, deterministic Lua
// runtime (architecture.md §6, execution-model.md §7). It is the ONLY package
// in the tree that imports the gopher-lua fork (repoes/gopher-lua, wired by a
// go.mod replace) — every other package reaches Lua through luabind, so the
// determinism patches (#262-265), the sandbox (#266), the generated bindings
// (#267), and the scheduler integration (#269) all have a single chokepoint.
//
// This file (#261) is the bring-up smoke surface only: a constructor and a
// one-shot eval used to prove the vendored fork builds and runs. The real
// binding layer is generated from api-manifest.json under #267.
package luabind

import (
	"fmt"

	lua "github.com/yuin/gopher-lua"
)

// NewState returns a fresh Lua interpreter. At #261 it is a stock gopher-lua
// state; the sandbox (#266) and quota patches (#262) will replace this
// constructor with the locked-down one. Callers must Close it.
func NewState() *lua.LState {
	return lua.NewState()
}

// Eval runs src as a Lua chunk on a fresh state and returns its first return
// value rendered as a string (Lua tostring semantics). It is the smoke-test
// primitive — `Eval("return 1+1")` yields "2". Any Lua compile/runtime error
// is returned (fail-closed: never a swallowed default).
func Eval(src string) (string, error) {
	L := NewState()
	defer L.Close()
	if err := L.DoString(src); err != nil {
		return "", fmt.Errorf("luabind: eval: %w", err)
	}
	if L.GetTop() == 0 {
		return "", nil // chunk returned nothing
	}
	v := L.Get(-1)
	return lua.LVAsString(L.ToStringMeta(v)), nil
}

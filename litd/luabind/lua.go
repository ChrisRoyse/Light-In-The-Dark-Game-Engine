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
	i := New()
	defer i.Close()
	return i.Eval(src)
}

// Interp is a luabind-owned, reusable Lua interpreter. It lets a host (or a
// test) bind a deterministic random source and drive several evals against ONE
// persistent state — without importing the gopher-lua fork directly (luabind
// is the sole importer). The real binding layer (#267) builds on this seam.
type Interp struct{ L *lua.LState }

// New returns a fresh interpreter. Callers must Close it.
func New() *Interp { return &Interp{L: NewState()} }

// Close releases the interpreter.
func (i *Interp) Close() { i.L.Close() }

// SetRandomSource binds the deterministic source math.random draws from
// (#263). fn must return a value in [0, 1); the host wires it to the sim PRNG
// (R-SIM-2). With none bound, math.random raises a loud error.
func (i *Interp) SetRandomSource(fn func() float64) { i.L.SetRandomSource(fn) }

// SetInstructionBudget arms the per-eval VM instruction budget (#262); n<=0
// disables it.
func (i *Interp) SetInstructionBudget(n int64) { i.L.SetInstructionBudget(n) }

// Eval runs src and returns its first return value as a string.
func (i *Interp) Eval(src string) (string, error) {
	if err := i.L.DoString(src); err != nil {
		return "", fmt.Errorf("luabind: eval: %w", err)
	}
	if i.L.GetTop() == 0 {
		return "", nil
	}
	v := i.L.Get(-1)
	s := lua.LVAsString(i.L.ToStringMeta(v))
	i.L.SetTop(0) // clear results so successive evals don't accumulate stack
	return s, nil
}

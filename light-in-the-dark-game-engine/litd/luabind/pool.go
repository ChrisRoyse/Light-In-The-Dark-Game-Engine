package luabind

// LState pooling (#265, LITD-PATCH 4 — the cross-world half; D-2026-06-11-25).
// Building a sandboxed interpreter is not free: NewSandbox runs lua.NewState,
// opens four libraries, performs the string-lock metatable surgery, and arms the
// budgets. A host that loads world after world (match restart, the hub trying
// many worlds, headless batch sims) pays that every time and throws the whole
// interpreter away. Pool recycles the interpreter SHELL — the LState, its value
// stack, its registry, its callframe stack — across worlds, while a
// reset-to-pristine guarantees ZERO state leaks between worlds.
//
// Why this is safe (the determinism contract, R-SIM-6 / the #271 G5.7 gate):
// resetSandbox returns a used LState to the exact state a fresh NewSandbox would
// produce, by construction — it swaps in a brand-new globals table and re-runs
// the identical installSandboxEnv that NewSandbox runs. So whatever a prior
// world wrote (globals, reassigned builtins, poisoned math/table tables, parked
// scheduler threads, registered handlers) is unreferenced and gone; the next
// world drives the sim identically to one driven by a never-pooled interpreter.
// pool_test.go proves it: the same scenario over a recycled interpreter produces
// a BIT-IDENTICAL sim StateHash to a fresh one (and a negative control shows the
// reset is load-bearing, not vacuous).
//
// Scope note: this is a world-load cost lever. The steady-state per-tick handler
// path is ALREADY zero-alloc (patch 4a, #407) and is unaffected by pooling.

import (
	lua "github.com/yuin/gopher-lua"
)

// Pool recycles pristine sandboxed interpreters. Get hands out an interpreter in
// the exact state NewSandbox would return (recycled if one is free, else newly
// built); the caller then Register(g)s its world onto it. Put resets the
// interpreter and returns it for reuse. A Pool is NOT safe for concurrent use —
// the sim runs single-threaded on one goroutine (R-EXEC-1), matching that.
type Pool struct {
	opts SandboxOptions
	free []*Interp
}

// NewSandboxPool returns a pool that builds/resets interpreters with opts (the
// same SandboxOptions NewSandbox takes — instruction/memory budgets, random
// source). The opts are re-applied on every reset, so a recycled interpreter is
// armed identically to a fresh one.
func NewSandboxPool(opts SandboxOptions) *Pool { return &Pool{opts: opts} }

// Get returns a pristine sandboxed interpreter. Recycled interpreters were reset
// on Put, so a Get'd interpreter is indistinguishable from NewSandbox(opts).
func (p *Pool) Get() *Interp {
	if n := len(p.free); n > 0 {
		i := p.free[n-1]
		p.free[n-1] = nil
		p.free = p.free[:n-1]
		return i
	}
	return NewSandbox(p.opts)
}

// Put resets i to pristine and returns it to the pool. The caller must not touch
// i after Put. resetSandbox drops the world's scheduler (handlers, parked
// threads, the game binding) and rebuilds a clean global environment.
func (p *Pool) Put(i *Interp) {
	if i == nil {
		return
	}
	resetSandbox(i.L, p.opts)
	p.free = append(p.free, i)
}

// Close closes every pooled interpreter and empties the pool. Interpreters
// handed out via Get and not yet Put are the caller's to Close.
func (p *Pool) Close() {
	for idx, i := range p.free {
		i.Close()
		p.free[idx] = nil
	}
	p.free = p.free[:0]
}

// resetSandbox returns a used LState to the state a fresh NewSandbox produced,
// so it can drive a new world with no leakage from the old one. The steps, in
// order:
//
//  1. Drop the scheduler entry. The per-LState scriptScheduler (Register
//     installs it into the package `schedulers` map) holds the old world's
//     game binding, OnEvent handlers, periodic actions, and parked coroutine
//     threads. Deleting it makes them all unreferenced — and is ALSO the fix
//     for the map otherwise retaining every LState forever (a slow leak in any
//     host that builds many interpreters, pooled or not).
//  2. Unwind to the main thread and clear the value stack, so a reset done
//     while a coroutine is the current thread (it never is in normal host use,
//     but be defensive) cannot strand a frame.
//  3. Swap _G for a fresh empty table AND re-point L.Env at it. The globals a
//     chunk sees are resolved through the main thread's environment register
//     L.Env, which NewState initializes to L.G.Global (gopher-lua state.go:688)
//     and which the compiler captures into every loaded function (state.go:1628
//     newLFunctionL(proto, ls.Env, …)). Swapping only L.G.Global without
//     re-pointing L.Env splits the brain: SetGlobal/installSandboxEnv/Register
//     write the NEW table while freshly-loaded scripts still read the OLD one —
//     so the next world misses every freshly-installed binding AND inherits the
//     prior world's leaked globals. Restoring the NewState invariant
//     (L.Env == L.G.Global) is what actually makes the reset reset.
//  4. Re-run installSandboxEnv on the fresh _G — the IDENTICAL install
//     NewSandbox runs — so the libraries, string lock, budgets, and random
//     source are rebuilt exactly as for a new interpreter.
func resetSandbox(L *lua.LState, opts SandboxOptions) {
	schedulers.Delete(L)

	L.G.CurrentThread = L.G.MainThread
	L.SetTop(0)

	L.G.Global = L.NewTable() // fresh, empty _G; orphans old globals + lib tables
	L.Env = L.G.Global        // re-point the env register (NewState's invariant)
	installSandboxEnv(L, opts)
}

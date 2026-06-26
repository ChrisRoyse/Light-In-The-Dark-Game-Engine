package luabind

// The R-SEC-1 hard sandbox (#266, execution-model.md §7, decisions.md
// D-2026-06-11-20). Script code is untrusted: a world author — human or AI —
// can ship arbitrary Lua, so the embedded VM must not be able to touch the
// filesystem, the OS, the network, the wall clock, the host's Go internals, or
// nondeterministic state, and must run under hard CPU (instruction) and memory
// budgets. Everything here is fail-closed: the default state of a capability is
// OFF, and a capability is present only because it appears on a closed
// whitelist that a census test pins.

import (
	"sort"

	lua "github.com/yuin/gopher-lua"
)

// SandboxOptions configures a locked-down interpreter.
type SandboxOptions struct {
	// InstructionBudget caps VM instructions per Eval (patch 1, #262). <=0
	// leaves it unarmed — only acceptable in trusted bring-up, never for
	// untrusted script. The host re-arms it each tick via the Interp.
	InstructionBudget int64
	// MemoryBudget caps bytes requested from unbounded allocators over the
	// LState lifetime (patch 4 mem accountant, #266). <=0 leaves it unarmed.
	MemoryBudget int64
	// RandomSource is the deterministic [0,1) source math.random draws from
	// (#263). nil leaves math.random fail-closed (raises if called).
	RandomSource func() float64
}

// sandboxLibs is the CLOSED whitelist of libraries opened in the sandbox.
// Deliberately absent: package/require (code loading + filesystem), io (files),
// os (filesystem, processes, AND the wall clock — os.time/os.clock are
// determinism leaks), debug (reflection / bytecode / upvalue escalation),
// channel (goroutine concurrency — breaks the single-threaded sim model),
// coroutine (a coroutine is a child LState that would NOT inherit the budgets,
// i.e. the literal quota-dodge vector; enabling it safely is #269's job).
var sandboxLibs = []struct {
	name string
	open lua.LGFunction
}{
	{"", lua.OpenBase},        // base globals — then the dangerous ones are stripped below
	{lua.TabLibName, lua.OpenTable},
	{lua.StringLibName, lua.OpenString},
	{lua.MathLibName, lua.OpenMath}, // deterministic mathlib (#263)
}

// strippedBaseGlobals are base-library globals removed after OpenBase. Each is
// a capability that breaks the sandbox: code loading (load/loadstring/dofile/
// loadfile/require/module — filesystem + arbitrary bytecode), environment
// reflection (getfenv/setfenv — sandbox-env escape), the GC side channel
// (collectgarbage — nondeterministic timing + memory probing), the userdata
// proxy escalation (newproxy), and a debug register dump (_printregs).
var strippedBaseGlobals = []string{
	"load", "loadstring", "dofile", "loadfile", "require", "module",
	"getfenv", "setfenv", "collectgarbage", "newproxy", "_printregs",
	"_GOPHER_LUA_VERSION", // impl-detail version string; not a capability the script needs
}

// SandboxGlobalWhitelist is the EXACT set of top-level _G names a freshly built
// sandbox exposes. The census test (census_test.go) compares a live snapshot of
// _G against this list and fails on any addition or removal — so a future lib
// bump that smuggles in a new global cannot pass silently. Keep sorted.
var SandboxGlobalWhitelist = []string{
	"_G", "_VERSION",
	"assert", "error", "getmetatable", "ipairs", "math", "next", "pairs",
	"pcall", "print", "rawequal", "rawget", "rawset", "select", "setmetatable",
	"string", "table", "tonumber", "tostring", "type", "unpack", "xpcall",
}

// NewSandbox builds a locked-down interpreter per R-SEC-1. Callers must Close it.
func NewSandbox(opts SandboxOptions) *Interp {
	L := lua.NewState(lua.Options{SkipOpenLibs: true})
	installSandboxEnv(L, opts)
	return &Interp{L: L}
}

// installSandboxEnv installs the entire R-SEC-1 sandbox environment onto L's
// CURRENT globals table: the whitelisted libraries, the deterministic math
// backend, the stripped dangerous globals, the read-only string lock, and the
// armed budgets/random source. It is shared by NewSandbox (on a brand-new
// LState) and resetSandbox (#265: after swapping in a fresh _G on a recycled
// LState) — running the SAME install on both paths is what makes a pooled,
// reset interpreter byte-for-byte equivalent to a freshly built one. It assumes
// L's globals table is empty (a new LState, or a freshly swapped _G).
func installSandboxEnv(L *lua.LState, opts SandboxOptions) {
	// Open only the whitelisted libraries. Base must be opened first (it
	// installs _G), matching OpenLibs' own ordering note.
	for _, lib := range sandboxLibs {
		L.Push(L.NewFunction(lib.open))
		L.Push(lua.LString(lib.name))
		L.Call(1, 0)
	}
	L.SetMathBackend(detMathBackend) // deterministic math.* (#391, D-2026-06-19-1)

	// Strip the dangerous base globals. Setting them to nil removes the name
	// from _G entirely (so the census sees them gone, not merely shadowed).
	for _, name := range strippedBaseGlobals {
		L.SetGlobal(name, lua.LNil)
	}

	// Lock the string library against escalation. Out of the box gopher-lua
	// uses the string library table as BOTH the global `string` AND the shared
	// metatable for every string value, with `__index` pointing back at
	// itself. That self-reference means a script can reach the mutable library
	// table through plain string indexing — `("").__index.upper = evil` — and
	// globally redefine string methods for the whole LState (poisoning later
	// scripts that share the persistent sandbox). Close it by separating the
	// three roles:
	//   real  — the functions table (kept unreferenced by any script-reachable
	//           name; reachable only as an __index target, which yields values
	//           FROM it, never the table itself);
	//   smt   — the string-value metatable: __index = real, __metatable locked
	//           (so getmetatable("") cannot hand back a mutable table, and
	//           ("").__index is nil);
	//   proxy — the new global `string`: a read-only view whose __index reads
	//           from real and whose __newindex raises (so `string.upper = x`
	//           and `string.x = y` both fail loudly).
	// Method dispatch (("x"):upper()) resolves through smt.__index = real and
	// is unaffected.
	if real, ok := L.GetGlobal("string").(*lua.LTable); ok {
		// OpenString sets real.__index = real (the self-reference that powers
		// method dispatch when the lib table doubles as the metatable). Now
		// that `real` is purely an __index *target*, that key would let
		// ("").__index resolve back to the mutable table — clear it.
		real.RawSetString("__index", lua.LNil)

		readonly := L.NewFunction(func(l *lua.LState) int {
			l.RaiseError("litd: string library is read-only (sandbox)")
			return 0
		})

		smt := L.NewTable()
		smt.RawSetString("__index", real)
		smt.RawSetString("__metatable", lua.LString("locked"))
		L.SetMetatable(lua.LString(""), smt) // builtinMts[LTString] = smt

		proxy := L.NewTable()
		pmt := L.NewTable()
		pmt.RawSetString("__index", real)
		pmt.RawSetString("__newindex", readonly)
		pmt.RawSetString("__metatable", lua.LString("locked"))
		L.SetMetatable(proxy, pmt)
		L.SetGlobal("string", proxy)
	}

	// Arm the budgets and bind the deterministic random source.
	if opts.InstructionBudget > 0 {
		L.SetInstructionBudget(opts.InstructionBudget)
	}
	if opts.MemoryBudget > 0 {
		L.SetMemoryBudget(opts.MemoryBudget)
	}
	if opts.RandomSource != nil {
		L.SetRandomSource(opts.RandomSource)
	}
}

// GlobalNames returns the sorted top-level names currently in _G. It is the
// Source of Truth the census test reads — a live snapshot of the global
// environment, not a claim about it.
func (i *Interp) GlobalNames() []string {
	g := i.L.Get(lua.GlobalsIndex).(*lua.LTable)
	var names []string
	g.ForEach(func(k, _ lua.LValue) {
		if ks, ok := k.(lua.LString); ok {
			names = append(names, string(ks))
		}
	})
	sort.Strings(names)
	return names
}

// RemainingMemory exposes the unspent sandbox memory budget (bytes).
func (i *Interp) RemainingMemory() int64 { return i.L.RemainingMemory() }

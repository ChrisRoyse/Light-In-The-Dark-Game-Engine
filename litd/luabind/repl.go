package luabind

// repl.go is the backend half of the in-game debug console (#399, R-OBS-5): the
// eval engine + the obs.* module. The rendered backquote overlay is blocked on the
// render/UI epic, but the engine it sits on is headless and lands independently. A
// console line goes through EvalLine, which runs it against the live script VM the
// way a REPL does (expression OR statement), captures print output and return
// values, and fails closed — a syntax or runtime error becomes loud error text, not
// a panic or a crashed VM. RegisterObs installs the obs table the console exposes;
// its reads are inert and loglevel(n) only changes logging verbosity, never sim
// state, so the console can never perturb the hash.

import (
	"strings"

	lua "github.com/yuin/gopher-lua"
)

// ReplResult is the outcome of one evaluated console line. Output holds captured
// print output followed by any formatted return values; Err is non-empty (and Ok
// false) when the line failed to compile or raised at runtime. The VM survives
// either way.
type ReplResult struct {
	Output string
	Err    string
	Ok     bool
}

// CounterLine is one counter row obs.counters() reports.
type CounterLine struct {
	Name  string
	Unit  string
	Value int64
}

// ReplObs is the read surface the obs.* console module exposes. The host backs it
// with its live *obs.Logger / *obs.Counters; tests use a fake. Every method is an
// inert read except SetLogLevel, which adjusts logging verbosity only (presentation,
// never sim state).
type ReplObs interface {
	DumpLog() string            // obs.dump()    -> formatted log ring
	Counters() []CounterLine    // obs.counters()-> current counter rows
	LogLevel() int              // obs.loglevel()-> current level
	SetLogLevel(level int) bool // obs.loglevel(n) -> accepted?
}

// EvalLine evaluates one console line against L and returns the captured result. It
// first tries the line as an expression ("return <line>", so typing `1+1` or
// `obs.counters()` prints a value), and falls back to running it as a statement.
// print output is captured for the duration of the call and the global is restored
// afterwards. A compile or runtime error is returned as Err with Ok=false; the call
// never panics and leaves the VM usable.
func EvalLine(L *lua.LState, line string) ReplResult {
	if strings.TrimSpace(line) == "" {
		return ReplResult{Ok: true}
	}

	var out strings.Builder
	restore := capturePrint(L, &out)
	defer restore()

	fn, err := L.LoadString("return " + line)
	if err != nil {
		// Not a valid expression — try it as a statement.
		fn, err = L.LoadString(line)
		if err != nil {
			return ReplResult{Err: cleanLuaErr(err)}
		}
	}

	base := L.GetTop()
	L.Push(fn)
	if err := L.PCall(0, lua.MultRet, nil); err != nil {
		L.SetTop(base)
		return ReplResult{Output: out.String(), Err: cleanLuaErr(err)}
	}

	// Append any return values left on the stack (positions base+1..top).
	top := L.GetTop()
	for i := base + 1; i <= top; i++ {
		if out.Len() > 0 && !strings.HasSuffix(out.String(), "\n") {
			out.WriteByte('\t')
		}
		out.WriteString(L.ToStringMeta(L.Get(i)).String())
	}
	L.SetTop(base)
	return ReplResult{Output: out.String(), Ok: true}
}

// capturePrint swaps the global print for one that appends to out, returning a
// closure that restores the original. Captured args are tab-separated, newline
// terminated, matching Lua's print.
func capturePrint(L *lua.LState, out *strings.Builder) func() {
	old := L.GetGlobal("print")
	L.SetGlobal("print", L.NewFunction(func(L *lua.LState) int {
		n := L.GetTop()
		for i := 1; i <= n; i++ {
			if i > 1 {
				out.WriteByte('\t')
			}
			out.WriteString(L.ToStringMeta(L.Get(i)).String())
		}
		out.WriteByte('\n')
		return 0
	}))
	return func() { L.SetGlobal("print", old) }
}

// cleanLuaErr trims the noisy "<string>:N:" prefix gopher-lua puts on eval errors so
// the console shows the human message; the full error is still recoverable upstream.
func cleanLuaErr(err error) string {
	msg := err.Error()
	if i := strings.Index(msg, ": "); i >= 0 && strings.HasPrefix(msg, "<string>") {
		return msg[i+2:]
	}
	return msg
}

// RegisterObs installs the obs console module on L: obs.dump(), obs.counters(),
// obs.loglevel([n]). src supplies the data. With nil src the module is omitted (a
// headless game with no observability simply has no obs table).
func RegisterObs(L *lua.LState, src ReplObs) {
	if src == nil {
		return
	}
	mod := L.NewTable()

	L.SetField(mod, "dump", L.NewFunction(func(L *lua.LState) int {
		L.Push(lua.LString(src.DumpLog()))
		return 1
	}))

	L.SetField(mod, "counters", L.NewFunction(func(L *lua.LState) int {
		t := L.NewTable()
		for _, c := range src.Counters() {
			row := L.NewTable()
			L.SetField(row, "name", lua.LString(c.Name))
			L.SetField(row, "unit", lua.LString(c.Unit))
			L.SetField(row, "value", lua.LNumber(float64(c.Value)))
			t.Append(row)
		}
		L.Push(t)
		return 1
	}))

	L.SetField(mod, "loglevel", L.NewFunction(func(L *lua.LState) int {
		if L.GetTop() >= 1 {
			ok := src.SetLogLevel(int(L.CheckNumber(1)))
			L.Push(lua.LBool(ok))
			return 1
		}
		L.Push(lua.LNumber(float64(src.LogLevel())))
		return 1
	}))

	L.SetGlobal("obs", mod)
}

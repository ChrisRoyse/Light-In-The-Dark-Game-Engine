package luabind

// #399 REPL-backend FSV. SoT = the captured ReplResult and the fake ReplObs's
// recorded state — the actual bytes the eval engine produced / the mutation it
// performed, not a return code. X+X=Y: `return 2+2` must yield Output "4"; the
// obs.* module must surface the fake's data verbatim; and every malformed line must
// fail closed (loud Err, Ok=false, VM still alive on the next line).

import (
	"strings"
	"testing"

	lua "github.com/yuin/gopher-lua"
)

// fakeObs is a deterministic ReplObs for verification: known dump text, known
// counter rows, and a recorded log level we can read back after SetLogLevel.
type fakeObs struct {
	dump     string
	counters []CounterLine
	level    int
	setCalls []int
}

func (f *fakeObs) DumpLog() string         { return f.dump }
func (f *fakeObs) Counters() []CounterLine { return f.counters }
func (f *fakeObs) LogLevel() int           { return f.level }
func (f *fakeObs) SetLogLevel(l int) bool {
	f.setCalls = append(f.setCalls, l)
	if l < 0 || l > 5 {
		return false
	}
	f.level = l
	return true
}

func newReplState(src ReplObs) *lua.LState {
	L := lua.NewState()
	RegisterObs(L, src)
	return L
}

func TestEvalLineExpressionAndPrintFSV(t *testing.T) {
	L := newReplState(nil)
	defer L.Close()

	// X+X=Y: a pure expression returns its value as text.
	if r := EvalLine(L, "return 2+2"); !r.Ok || r.Output != "4" {
		t.Fatalf("`return 2+2` -> %+v, want Ok output \"4\"", r)
	}
	// Bare expression form (no `return`) also works via the REPL retry.
	if r := EvalLine(L, "3*14"); !r.Ok || r.Output != "42" {
		t.Fatalf("`3*14` -> %+v, want Ok output \"42\"", r)
	}
	// print output is captured verbatim (tab-joined, newline-terminated).
	r := EvalLine(L, "print('hi', 7)")
	t.Logf("FSV print capture: %q", r.Output)
	if !r.Ok || r.Output != "hi\t7\n" {
		t.Fatalf("print capture -> %+v, want \"hi\\t7\\n\"", r)
	}
	// A blank line is a no-op success.
	if r := EvalLine(L, "   "); !r.Ok || r.Output != "" {
		t.Fatalf("blank line -> %+v, want Ok empty", r)
	}
}

func TestEvalLineFailClosedFSV(t *testing.T) {
	L := newReplState(nil)
	defer L.Close()

	// (1) Syntax error: loud Err, Ok=false, no panic.
	r := EvalLine(L, "return (((")
	t.Logf("FSV syntax err: %q", r.Err)
	if r.Ok || r.Err == "" {
		t.Fatalf("syntax error not reported: %+v", r)
	}
	// (2) Runtime error surfaces the message.
	r = EvalLine(L, "error('boom')")
	t.Logf("FSV runtime err: %q", r.Err)
	if r.Ok || !strings.Contains(r.Err, "boom") {
		t.Fatalf("runtime error -> %+v, want Err containing boom", r)
	}
	// (3) Calling a nil global is a runtime error, not a crash.
	r = EvalLine(L, "nope_not_defined()")
	if r.Ok || r.Err == "" {
		t.Fatalf("nil-call -> %+v, want failure", r)
	}
	// SoT: the VM is still alive after three failures — a good line still runs.
	if r := EvalLine(L, "return 1+1"); !r.Ok || r.Output != "2" {
		t.Fatalf("VM dead after errors: %+v", r)
	}
}

func TestObsModuleFSV(t *testing.T) {
	fake := &fakeObs{
		dump:     "LOG-DUMP-MARKER-7",
		counters: []CounterLine{{Name: "frames", Unit: "f", Value: 60}, {Name: "draws", Unit: "n", Value: 128}},
		level:    1,
	}
	L := newReplState(fake)
	defer L.Close()

	// obs.dump() returns the fake's text verbatim.
	if r := EvalLine(L, "return obs.dump()"); !r.Ok || r.Output != "LOG-DUMP-MARKER-7" {
		t.Fatalf("obs.dump() -> %+v, want the marker", r)
	}
	// obs.counters() surfaces the rows; index + field access proves the table shape.
	if r := EvalLine(L, "local c=obs.counters(); return c[1].name..'='..c[1].value"); !r.Ok || r.Output != "frames=60" {
		t.Fatalf("obs.counters()[1] -> %+v, want frames=60", r)
	}
	if r := EvalLine(L, "return #obs.counters()"); !r.Ok || r.Output != "2" {
		t.Fatalf("counter count -> %+v, want 2", r)
	}
	// obs.loglevel() reads current level...
	if r := EvalLine(L, "return obs.loglevel()"); !r.Ok || r.Output != "1" {
		t.Fatalf("obs.loglevel() -> %+v, want 1", r)
	}
	// ...obs.loglevel(n) sets it; SoT = the fake's recorded state after the call.
	if r := EvalLine(L, "return obs.loglevel(3)"); !r.Ok || r.Output != "true" {
		t.Fatalf("obs.loglevel(3) -> %+v, want true", r)
	}
	t.Logf("FSV fake after set: level=%d setCalls=%v", fake.level, fake.setCalls)
	if fake.level != 3 || len(fake.setCalls) != 1 || fake.setCalls[0] != 3 {
		t.Fatalf("SetLogLevel not applied: level=%d calls=%v", fake.level, fake.setCalls)
	}
	// Out-of-range level is rejected (fail-closed) and leaves the level unchanged.
	if r := EvalLine(L, "return obs.loglevel(99)"); !r.Ok || r.Output != "false" {
		t.Fatalf("obs.loglevel(99) -> %+v, want false", r)
	}
	if fake.level != 3 {
		t.Fatalf("rejected level still mutated state: %d", fake.level)
	}
}

func TestRegisterObsNilOmitsModuleFSV(t *testing.T) {
	// A headless game with no observability has no obs table — accessing it is a
	// normal Lua nil-index error, not a crash.
	L := newReplState(nil)
	defer L.Close()
	if r := EvalLine(L, "return obs"); !r.Ok || r.Output != "nil" {
		t.Fatalf("obs with nil src -> %+v, want nil", r)
	}
}

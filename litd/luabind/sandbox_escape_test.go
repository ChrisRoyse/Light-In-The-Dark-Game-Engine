package luabind_test

// #266 adversarial sandbox-escape suite. SoT = the loud failure each attack
// produces (or, for the positive controls, the value legit code computes).
// Every vector prints its attempt and the verdict so the proof is in the test
// log, not in the exit code. No mocks; no fallbacks that mask a failure.

import (
	"runtime"
	"strings"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/luabind"
)

func newTestSandbox() *luabind.Interp {
	return luabind.NewSandbox(luabind.SandboxOptions{
		InstructionBudget: 5_000_000,
		MemoryBudget:      4 << 20, // 4 MiB
		RandomSource:      newLCG(1),
	})
}

// blockedVectors must each raise — the capability is gone, so touching it is a
// loud runtime error ("attempt to index/call a nil value"), never a value.
var blockedVectors = []struct {
	name string
	src  string
}{
	{"io.open (filesystem)", `io.open("/etc/passwd", "r")`},
	{"io.read", `return io.read()`},
	{"os.execute (process spawn)", `os.execute("echo pwned")`},
	{"os.time (wall-clock determinism leak)", `return os.time()`},
	{"os.clock (wall-clock determinism leak)", `return os.clock()`},
	{"os.getenv", `return os.getenv("HOME")`},
	{"os.exit (kill host)", `os.exit(1)`},
	{"require (code loading)", `require("os")`},
	{"package.loaded", `return package.loaded`},
	{"dofile (filesystem)", `dofile("/etc/passwd")`},
	{"loadfile (filesystem)", `loadfile("/etc/passwd")`},
	{"load (arbitrary chunk + env smuggle)", `return load("return os")()`},
	{"loadstring", `return loadstring("return 1")()`},
	{"debug.getinfo (reflection)", `return debug.getinfo(1)`},
	{"debug.getupvalue (upvalue escalation)", `return debug.getupvalue(print, 1)`},
	{"collectgarbage (GC side channel)", `return collectgarbage("count")`},
	{"getfenv (env reflection)", `return getfenv(1)`},
	{"setfenv (env swap escape)", `setfenv(1, {})`},
	{"newproxy (userdata escalation)", `return newproxy(true)`},
	{"coroutine.create (budget-dodge via child LState)", `return coroutine.create(function() end)`},
	{"channel.make (goroutine concurrency)", `return channel.make()`},
}

func TestSandboxEscapeBlockedVectors(t *testing.T) {
	for _, v := range blockedVectors {
		v := v
		t.Run(v.name, func(t *testing.T) {
			i := newTestSandbox()
			defer i.Close()
			_, err := i.Eval(v.src)
			if err == nil {
				t.Fatalf("VECTOR NOT BLOCKED: %s — %q returned without error", v.name, v.src)
			}
			t.Logf("BLOCKED %-52s -> %v", v.name, oneLine(err))
		})
	}
}

// TestSandboxStringMetatableLocked — getmetatable("") escalation. The string
// metatable is locked, so getmetatable("") yields the sentinel, not the
// mutable string library, and a script cannot reach .__index to redefine string
// methods. The legit method-dispatch path (("x"):upper()) still works.
func TestSandboxStringMetatableLocked(t *testing.T) {
	i := newTestSandbox()
	defer i.Close()

	mt, err := i.Eval(`return tostring(getmetatable(""))`)
	if err != nil {
		t.Fatalf("eval: %v", err)
	}
	t.Logf("FSV getmetatable(\"\") = %q (want \"locked\" sentinel, not the string table)", mt)
	if mt != "locked" {
		t.Fatalf("string metatable not locked: getmetatable(\"\") = %q", mt)
	}

	// Indexing a string for __index must NOT yield the mutable string table —
	// the classic ("").__index.upper = evil escalation path.
	probe, err := i.Eval(`return type(("").__index)`)
	t.Logf("FSV type((\"\").__index) -> val=%q err=%v", probe, oneLine(err))
	if err == nil && probe == "table" {
		t.Fatalf("escalation possible: (\"\").__index is the mutable string table")
	}

	// Writing through the string library must fail loudly: neither redefining
	// an existing method nor adding a new field is allowed.
	_, err = i.Eval(`string.upper = function() return "PWNED" end`)
	t.Logf("FSV redefine string.upper -> err=%v", oneLine(err))
	if err == nil {
		t.Fatalf("escalation possible: string.upper is writable")
	}
	_, err = i.Eval(`("").x = 1`)
	t.Logf("FSV write to string value -> err=%v", oneLine(err))
	if err == nil {
		t.Fatalf("escalation possible: string values are writable")
	}
	// And after those attempts, upper still behaves.
	chk, err := i.Eval(`return ("ab"):upper()`)
	if err != nil || chk != "AB" {
		t.Fatalf("string.upper corrupted after write attempts: %q err=%v", chk, err)
	}

	// Positive control: method dispatch is unaffected by the lock.
	up, err := i.Eval(`return ("hello"):upper()`)
	if err != nil {
		t.Fatalf("legit string method broke under lock: %v", err)
	}
	t.Logf("FSV legit ('hello'):upper() = %q (want \"HELLO\")", up)
	if up != "HELLO" {
		t.Fatalf("('hello'):upper() = %q, want HELLO", up)
	}
}

// TestSandboxMemoryBomb — string.rep memory bomb. The memory budget rejects the
// allocation BEFORE it happens: the error is loud and located, and the host
// process RSS does not balloon (we print it before and after to prove the bomb
// never materialized).
func TestSandboxMemoryBomb(t *testing.T) {
	i := luabind.NewSandbox(luabind.SandboxOptions{
		InstructionBudget: 5_000_000,
		MemoryBudget:      1 << 20, // 1 MiB ceiling
	})
	defer i.Close()

	var before, after runtime.MemStats
	runtime.GC()
	runtime.ReadMemStats(&before)

	// Ask for ~1 GiB — 200× the budget. Must be refused without allocating.
	_, err := i.Eval(`return ("A"):rep(1024*1024*1024)`)

	runtime.ReadMemStats(&after)
	t.Logf("FSV mem bomb: HeapAlloc before=%d KiB after=%d KiB (delta=%d KiB)",
		before.HeapAlloc/1024, after.HeapAlloc/1024, (int64(after.HeapAlloc)-int64(before.HeapAlloc))/1024)
	t.Logf("FSV mem bomb: budget remaining=%d bytes, err=%v", i.RemainingMemory(), oneLine(err))

	if err == nil {
		t.Fatalf("MEMORY BOMB NOT BLOCKED: string.rep(1 GiB) succeeded")
	}
	if !strings.Contains(err.Error(), "memory budget exceeded") {
		t.Fatalf("wrong error for mem bomb: %v", err)
	}
	// The 1 GiB string must never have been allocated: heap growth stays far
	// below the requested gigabyte (allow generous slack for test noise).
	if grew := int64(after.HeapAlloc) - int64(before.HeapAlloc); grew > 64<<20 {
		t.Fatalf("heap grew %d bytes — the bomb appears to have allocated", grew)
	}
}

// TestSandboxMemoryBudgetUnderLimit — an allocation UNDER budget succeeds and
// debits the budget by exactly the requested bytes (deterministic accounting).
func TestSandboxMemoryBudgetUnderLimit(t *testing.T) {
	i := luabind.NewSandbox(luabind.SandboxOptions{
		InstructionBudget: 1_000_000,
		MemoryBudget:      1000,
	})
	defer i.Close()

	t.Logf("FSV before: budget=%d", i.RemainingMemory())
	got, err := i.Eval(`return #("AB"):rep(100)`) // 200 bytes
	if err != nil {
		t.Fatalf("under-budget rep failed: %v", err)
	}
	t.Logf("FSV after: len=%q budget=%d (want 1000-200=800)", got, i.RemainingMemory())
	if got != "200" {
		t.Fatalf("rep length = %q, want 200", got)
	}
	if rem := i.RemainingMemory(); rem != 800 {
		t.Fatalf("budget after 200-byte rep = %d, want 800 (deterministic charge)", rem)
	}
}

// TestSandboxInstructionBudget — CPU quota. An infinite loop trips the
// instruction budget loudly rather than hanging the host.
func TestSandboxInstructionBudget(t *testing.T) {
	i := luabind.NewSandbox(luabind.SandboxOptions{InstructionBudget: 100_000})
	defer i.Close()
	_, err := i.Eval(`while true do end`)
	t.Logf("FSV infinite loop under budget -> err=%v", oneLine(err))
	if err == nil || !strings.Contains(err.Error(), "instruction budget exceeded") {
		t.Fatalf("infinite loop not bounded by instruction budget: %v", err)
	}
}

// TestSandboxLegitCodeWorks — positive control: the whitelisted libraries do
// their job, so a real script is not collateral damage of the lockdown.
func TestSandboxLegitCodeWorks(t *testing.T) {
	i := newTestSandbox()
	defer i.Close()
	cases := []struct{ src, want string }{
		{`return math.floor(3.7)`, "3"},
		{`return string.format("%d-%s", 42, "x")`, "42-x"},
		{`local t={}; for k=1,5 do t[k]=k*k end; return table.concat(t, ",")`, "1,4,9,16,25"},
		{`return tostring(1+1)`, "2"},
		{`local n=0; for _ in pairs({a=1,b=2,c=3}) do n=n+1 end; return n`, "3"},
		{`return math.random(1,1)`, "1"}, // bound deterministic source
	}
	for _, c := range cases {
		got, err := i.Eval(c.src)
		if err != nil {
			t.Fatalf("legit %q errored: %v", c.src, err)
		}
		t.Logf("FSV legit %-50s = %q (want %q)", c.src, got, c.want)
		if got != c.want {
			t.Fatalf("%q = %q, want %q", c.src, got, c.want)
		}
	}
}

func oneLine(err error) string {
	if err == nil {
		return "<nil>"
	}
	s := err.Error()
	if idx := strings.IndexByte(s, '\n'); idx >= 0 {
		s = s[:idx]
	}
	return s
}

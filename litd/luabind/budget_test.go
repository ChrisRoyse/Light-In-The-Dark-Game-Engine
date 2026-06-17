package luabind_test

// #262 instruction-budget FSV. SoT = the VM's own verdict read back after a
// run: the located breach error and the remaining-budget counter — not a
// wall-clock timeout or a return code.

import (
	"strings"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/luabind"
)

// TestInstructionBudgetInfiniteLoopBreaches — edge (1): `while true do end` under
// a finite budget raises a LOCATED budget error (never hangs), and the host run
// returns so the sim tick can proceed.
func TestInstructionBudgetInfiniteLoopBreaches(t *testing.T) {
	r := luabind.RunBudgeted("while true do end", 10000)
	t.Logf("FSV infinite-loop breach: exceeded=%v remaining=%d err=%q", r.BudgetExceeded, r.Remaining, errStr(r.Err))
	if !r.BudgetExceeded {
		t.Fatalf("infinite loop did not trip the budget: err=%v", r.Err)
	}
	if r.Remaining != 0 {
		t.Fatalf("remaining at breach = %d, want 0", r.Remaining)
	}
	// Located: the error names a source position (the "<string>:N:" prefix).
	if !strings.Contains(r.Err.Error(), ":") || !strings.Contains(r.Err.Error(), "instruction budget exceeded") {
		t.Fatalf("breach error missing location/message: %q", r.Err)
	}
}

// TestInstructionBudgetExactBoundary — edge (2): a script run at exactly the
// instruction count it needs succeeds; one fewer breaches. Measured, not
// guessed: first count the instructions, then pin the boundary.
func TestInstructionBudgetExactBoundary(t *testing.T) {
	const src = `local s = 0; for i = 1, 50 do s = s + i end; return s`
	used, val, err := luabind.InstructionsUsed(src, 1_000_000)
	if err != nil {
		t.Fatalf("measure run: %v", err)
	}
	t.Logf("FSV measured: src used %d instructions, value=%q", used, val)
	if val != "1275" {
		t.Fatalf("sum 1..50 = %q, want 1275", val)
	}

	// Budget = used+1: success, and exactly 1 instruction of headroom remains.
	ok := luabind.RunBudgeted(src, used+1)
	t.Logf("FSV budget=used+1 (%d): exceeded=%v remaining=%d value=%q", used+1, ok.BudgetExceeded, ok.Remaining, ok.Value)
	if ok.BudgetExceeded || ok.Value != "1275" {
		t.Fatalf("budget=used+1 should succeed: %+v", ok)
	}
	if ok.Remaining != 1 {
		t.Fatalf("RemainingBudget at budget-1 instructions = %d, want 1", ok.Remaining)
	}

	// Budget = used (exact): still succeeds, 0 remaining.
	exact := luabind.RunBudgeted(src, used)
	if exact.BudgetExceeded || exact.Value != "1275" || exact.Remaining != 0 {
		t.Fatalf("budget=used should succeed with 0 remaining: %+v", exact)
	}

	// Budget = used-1: breaches (one instruction short of finishing).
	short := luabind.RunBudgeted(src, used-1)
	t.Logf("FSV budget=used-1 (%d): exceeded=%v", used-1, short.BudgetExceeded)
	if !short.BudgetExceeded {
		t.Fatalf("budget=used-1 should breach: %+v", short)
	}
}

// TestInstructionBudgetNotSwallowableByPcall — edge (3): the script wraps an
// infinite loop in pcall and tries to continue; it MUST NOT reach the line past
// the loop, because the budget stays exhausted and the next instruction
// re-raises. The host sees the breach, not "survived".
func TestInstructionBudgetNotSwallowableByPcall(t *testing.T) {
	const src = `
		pcall(function() while true do end end)
		return "survived"
	`
	r := luabind.RunBudgeted(src, 10000)
	t.Logf("FSV pcall-swallow attempt: exceeded=%v value=%q err=%q", r.BudgetExceeded, r.Value, errStr(r.Err))
	if r.Value == "survived" {
		t.Fatal("script swallowed the budget breach via pcall and continued past the tick")
	}
	if !r.BudgetExceeded {
		t.Fatalf("expected an unswallowed budget breach, got: %+v", r)
	}
}

// TestInstructionBudgetDeterministic — edge (4): two runs of the same script
// under the same budget fail at the identical instruction count (remaining and
// located error identical).
func TestInstructionBudgetDeterministic(t *testing.T) {
	run := func() luabind.BudgetResult { return luabind.RunBudgeted("local n=0; while true do n=n+1 end", 25000) }
	a, b := run(), run()
	t.Logf("FSV determinism: a(exceeded=%v rem=%d) b(exceeded=%v rem=%d); errEqual=%v",
		a.BudgetExceeded, a.Remaining, b.BudgetExceeded, b.Remaining, errStr(a.Err) == errStr(b.Err))
	if !a.BudgetExceeded || !b.BudgetExceeded {
		t.Fatalf("both runs must breach: a=%+v b=%+v", a, b)
	}
	if a.Remaining != b.Remaining || errStr(a.Err) != errStr(b.Err) {
		t.Fatalf("nondeterministic breach: a(rem=%d err=%q) b(rem=%d err=%q)", a.Remaining, errStr(a.Err), b.Remaining, errStr(b.Err))
	}
}

// TestInstructionBudgetDisabled — n<=0 disables the check: a long-but-finite
// loop runs to completion, and RemainingBudget reports the disabled state.
func TestInstructionBudgetDisabled(t *testing.T) {
	r := luabind.RunBudgeted(`local s=0; for i=1,100000 do s=s+1 end; return s`, 0)
	t.Logf("FSV disabled: exceeded=%v value=%q", r.BudgetExceeded, r.Value)
	if r.BudgetExceeded || r.Value != "100000" {
		t.Fatalf("disabled budget should run to completion: %+v", r)
	}
}

func errStr(e error) string {
	if e == nil {
		return ""
	}
	return e.Error()
}

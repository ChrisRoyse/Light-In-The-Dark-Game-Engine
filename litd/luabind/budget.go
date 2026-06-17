package luabind

import (
	"strings"

	lua "github.com/yuin/gopher-lua"
)

// budgetExceededMsg is the substring the VM's budget error carries (the fork's
// litdBudgetExceeded message, prefixed by the source:line where it tripped).
const budgetExceededMsg = "instruction budget exceeded"

// BudgetResult is the outcome of a budgeted Lua run (#262, R-SEC-1).
type BudgetResult struct {
	Value          string // first return value (Lua tostring) on success
	Err            error  // nil on success; budget breach carries "source:line: …"
	BudgetExceeded bool   // true iff Err is the instruction-budget breach
	Remaining      int64  // RemainingBudget() after the run (0 at a breach)
}

// RunBudgeted compiles and runs src on a fresh state armed with an instruction
// budget of n (n<=0 = unlimited). It reports the VM's verdict read straight off
// the state: the breach error (with location) or the return value, plus the
// remaining budget. The budget breach is NOT swallowable by the script — see
// the fork's litd_budget.go — so a script that loops forever returns
// BudgetExceeded even if it wraps the loop in pcall.
func RunBudgeted(src string, n int64) BudgetResult {
	L := NewState()
	defer L.Close()
	L.SetInstructionBudget(n)
	err := L.DoString(src)
	r := BudgetResult{Remaining: L.RemainingBudget()}
	if err != nil {
		r.Err = err
		r.BudgetExceeded = strings.Contains(err.Error(), budgetExceededMsg)
		return r
	}
	if L.GetTop() > 0 {
		r.Value = lua.LVAsString(L.ToStringMeta(L.Get(-1)))
	}
	return r
}

// InstructionsUsed runs src under a generous budget and returns how many VM
// instructions it actually dispatched (budget − remaining). Used by tests to
// pin the exact-budget boundary without guessing opcode counts.
func InstructionsUsed(src string, ceiling int64) (used int64, value string, err error) {
	L := NewState()
	defer L.Close()
	L.SetInstructionBudget(ceiling)
	if e := L.DoString(src); e != nil {
		return 0, "", e
	}
	used = ceiling - L.RemainingBudget()
	if L.GetTop() > 0 {
		value = lua.LVAsString(L.ToStringMeta(L.Get(-1)))
	}
	return used, value, nil
}

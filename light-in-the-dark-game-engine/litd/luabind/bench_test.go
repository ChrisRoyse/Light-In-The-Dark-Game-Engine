package luabind_test

import (
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/luabind"
)

// benchScript is a fixed compute workload (arithmetic over a tight loop) — a
// stable instruction stream for measuring the mainLoop dispatch cost.
const benchScript = `local s = 0; for i = 1, 10000 do s = s + i * 2 - 1 end; return s`

// BenchmarkMainLoopBudgetOff is the as-shipped cost when no quota is armed —
// the regression imposed on every script even when budgets are disabled (one
// predictable, not-taken branch per dispatched instruction).
func BenchmarkMainLoopBudgetOff(b *testing.B) {
	for i := 0; i < b.N; i++ {
		if _, _, err := luabind.InstructionsUsed(benchScript, 0); err != nil {
			b.Fatal(err)
		}
	}
}

// BenchmarkMainLoopBudgetOn is the cost with the quota armed (the branch taken,
// plus the decrement) — what a sandboxed world script pays.
func BenchmarkMainLoopBudgetOn(b *testing.B) {
	for i := 0; i < b.N; i++ {
		r := luabind.RunBudgeted(benchScript, 1_000_000)
		if r.BudgetExceeded || r.Value != "100000000" {
			b.Fatalf("unexpected: %+v", r)
		}
	}
}

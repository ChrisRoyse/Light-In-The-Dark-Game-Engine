package sim

import "testing"

// BenchmarkWorldTick measures one tick of the determinism workload
// (256 entities + scheduler scripts + command stream). The 10k-tick
// wall time is 10,000 × this number; the budget harness compares
// ns/op against tick_ms_max and allocs/op against allocs_per_tick
// from benchmarks/budgets.toml (R-GC-5).
func BenchmarkWorldTick(b *testing.B) {
	w := NewDetWorld(katSeed, katN, ScriptedCommands(katSeed, 300))
	for i := 0; i < 256; i++ { // warm pools and heap capacities
		w.Step()
	}
	b.ReportAllocs()
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		w.Step()
	}
}

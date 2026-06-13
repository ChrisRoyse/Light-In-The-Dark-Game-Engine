package obs

import (
	"math"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestStandardCounterSet(t *testing.T) {
	c := NewDefaultCounters()
	RegisterStandardCounters(c)
	if c.CounterCount() != StandardCounterCount {
		t.Fatalf("standard counter count=%d want %d", c.CounterCount(), StandardCounterCount)
	}
	want := []string{
		"sim.tick", "sim.phase.input", "sim.phase.scripts", "sim.phase.orders",
		"sim.phase.movement", "sim.phase.combat", "sim.phase.events", "sim.phase.cleanup",
		"render.frame", "render.fps", "render.draw_calls", "render.batches",
		"render.instances", "render.allocs.frame", "sim.allocs.tick", "heap",
		"sim.path.expansions", "sim.path.queue_depth", "sim.entities.units.active",
		"sim.entities.missiles.active", "sim.entities.buffs.active", "audio.voices.active",
		"net.turn_rtt", "net.input_delay", "net.hash_lag", "lua.instructions.tick",
		"lua.mem",
	}
	for i, name := range want {
		if got := c.Def(CounterID(i)).Name; got != name {
			t.Fatalf("counter %d name=%q want %q", i, got, name)
		}
	}
	t.Logf("standard counters=%d history=%d samples (%ds @ %dHz)", c.CounterCount(), c.HistoryCap(), DefaultHistorySeconds, DefaultHistoryHz)
}

func TestCounterHistoryExportFSV(t *testing.T) {
	c := NewCounters(8, 4)
	tick := c.Register("sim.tick", "ns/op", CounterDuration)
	units := c.Register("sim.entities.units.active", "count/op", CounterGauge)

	t.Logf("BEFORE: samples=%d tick=%d units=%d", c.Len(), c.Value(tick), c.Value(units))
	c.Set(tick, 1_250_000)
	c.Set(units, 2)
	c.Sample(10, 0)
	c.Set(tick, 1_500_000)
	c.Set(units, 3)
	c.Sample(11, 0)

	path := filepath.Join(t.TempDir(), "counters.bench")
	if err := c.ExportBenchFile(path); err != nil {
		t.Fatal(err)
	}
	raw, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	text := string(raw)
	t.Logf("AFTER file %s:\n%s", path, text)
	for _, want := range []string{
		"# litd/obs counter history counters=2 samples=2 total=2 historyCap=4 overflow=int64-wraparound",
		"BenchmarkLITDPerf/sim.tick/tick_00000010-1 1 1250000 ns/op",
		"BenchmarkLITDPerf/sim.entities.units.active/tick_00000011-1 1 3 count/op",
	} {
		if !strings.Contains(text, want) {
			t.Fatalf("export missing %q", want)
		}
	}
}

func TestCounterOverflowWrapPolicyFSV(t *testing.T) {
	c := NewCounters(2, 2)
	dropped := c.Register("sim.events.dropped", "count/op", CounterTotal)
	before := int64(math.MaxInt64 - 1)
	c.Set(dropped, before)
	t.Logf("BEFORE overflow: value=%d policy=%s", c.Value(dropped), OverflowPolicy())
	c.Add(dropped, 3)
	after := c.Value(dropped)
	c.Sample(1, 0)
	t.Logf("AFTER overflow: value=%d policy=%s", after, OverflowPolicy())
	if after != math.MinInt64+1 {
		t.Fatalf("overflow=%d want %d", after, int64(math.MinInt64+1))
	}

	path := filepath.Join(t.TempDir(), "overflow.bench")
	if err := c.ExportBenchFile(path); err != nil {
		t.Fatal(err)
	}
	raw, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	text := string(raw)
	t.Logf("SoT overflow file:\n%s", text)
	if !strings.Contains(text, "overflow=int64-wraparound") || !strings.Contains(text, "-9223372036854775807 count/op") {
		t.Fatalf("overflow policy/value missing from export")
	}
}

func TestCounterZeroEntityIdleFSV(t *testing.T) {
	c := NewCounters(DefaultCounterCap, 2)
	std := RegisterStandardCounters(c)
	t.Logf("BEFORE idle: samples=%d units=%d missiles=%d buffs=%d",
		c.Len(), c.Value(std.SimEntitiesUnitsActive), c.Value(std.SimEntitiesMissiles), c.Value(std.SimEntitiesBuffs))
	c.Set(std.SimEntitiesUnitsActive, 0)
	c.Set(std.SimEntitiesMissiles, 0)
	c.Set(std.SimEntitiesBuffs, 0)
	c.Sample(42, 0)
	t.Logf("AFTER idle: samples=%d units=%d missiles=%d buffs=%d",
		c.Len(), c.Value(std.SimEntitiesUnitsActive), c.Value(std.SimEntitiesMissiles), c.Value(std.SimEntitiesBuffs))

	path := filepath.Join(t.TempDir(), "idle.bench")
	if err := c.ExportBenchFile(path); err != nil {
		t.Fatal(err)
	}
	raw, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	text := string(raw)
	t.Logf("SoT idle excerpt:\n%s", firstLines(text, 10))
	for _, want := range []string{
		"BenchmarkLITDPerf/sim.entities.units.active/tick_00000042-1 1 0 count/op",
		"BenchmarkLITDPerf/sim.entities.missiles.active/tick_00000042-1 1 0 count/op",
		"BenchmarkLITDPerf/sim.entities.buffs.active/tick_00000042-1 1 0 count/op",
	} {
		if !strings.Contains(text, want) {
			t.Fatalf("idle export missing %q", want)
		}
	}
}

func TestCounterHistoryRingWrapFSV(t *testing.T) {
	c := NewCounters(2, 3)
	synthetic := c.Register("synthetic.count", "count/op", CounterGauge)
	for i := 0; i < 3; i++ {
		c.Set(synthetic, int64(i))
		c.Sample(uint32(i+1), 0)
	}
	meta, _ := c.SampleMeta(0)
	v, _ := c.HistoryValue(0, synthetic)
	t.Logf("BEFORE wrap: len=%d total=%d oldestTick=%d oldestValue=%d", c.Len(), c.TotalSamples(), meta.Tick, v)

	for i := 3; i < 5; i++ {
		c.Set(synthetic, int64(i))
		c.Sample(uint32(i+1), 0)
	}
	meta0, _ := c.SampleMeta(0)
	v0, _ := c.HistoryValue(0, synthetic)
	meta2, _ := c.SampleMeta(2)
	v2, _ := c.HistoryValue(2, synthetic)
	t.Logf("AFTER wrap: len=%d total=%d oldestTick=%d oldestValue=%d newestTick=%d newestValue=%d",
		c.Len(), c.TotalSamples(), meta0.Tick, v0, meta2.Tick, v2)
	if c.Len() != 3 || c.TotalSamples() != 5 || meta0.Tick != 3 || v0 != 2 || meta2.Tick != 5 || v2 != 4 {
		t.Fatalf("ring wrap state wrong")
	}

	path := filepath.Join(t.TempDir(), "wrap.bench")
	if err := c.ExportBenchFile(path); err != nil {
		t.Fatal(err)
	}
	raw, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	text := string(raw)
	t.Logf("SoT wrap file:\n%s", text)
	if strings.Contains(text, "tick_00000001") || strings.Contains(text, "tick_00000002") {
		t.Fatal("evicted samples still present in export")
	}
	for _, want := range []string{"tick_00000003", "tick_00000004", "tick_00000005"} {
		if !strings.Contains(text, want) {
			t.Fatalf("retained sample %s missing", want)
		}
	}
}

func TestCounterHotPathZeroAlloc(t *testing.T) {
	c := NewCounters(DefaultCounterCap, 8)
	std := RegisterStandardCounters(c)
	var tick uint32
	if n := testing.AllocsPerRun(10000, func() {
		tick++
		c.Set(std.SimTickNS, int64(tick))
		c.Add(std.SimPathExpansionsTick, 2)
		c.Set(std.SimEntitiesUnitsActive, 500)
		c.Sample(tick, 0)
	}); n != 0 {
		t.Fatalf("counter hot path allocates %v/op; want 0", n)
	}
	t.Logf("AllocsPerRun=0 samples=%d retained=%d", c.TotalSamples(), c.Len())
}

func firstLines(s string, n int) string {
	lines := strings.Split(strings.TrimRight(s, "\n"), "\n")
	if len(lines) < n {
		n = len(lines)
	}
	return strings.Join(lines[:n], "\n")
}

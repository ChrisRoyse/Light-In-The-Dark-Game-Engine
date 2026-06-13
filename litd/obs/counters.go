package obs

import (
	"bufio"
	"fmt"
	"io"
	"os"
)

// CounterKind classifies a counter for overlays and telemetry. The hot
// path treats all values as int64 slots.
type CounterKind uint8

const (
	CounterGauge CounterKind = iota
	CounterTotal
	CounterDuration
)

func (k CounterKind) String() string {
	switch k {
	case CounterGauge:
		return "gauge"
	case CounterTotal:
		return "total"
	case CounterDuration:
		return "duration"
	}
	return fmt.Sprintf("kind(%d)", uint8(k))
}

// CounterID is the index of one registered counter slot.
type CounterID uint16

// CounterDef is immutable metadata for one counter. Name and Unit are
// benchstat tokens: no whitespace, no formatting on the hot path.
type CounterDef struct {
	Name string
	Unit string
	Kind CounterKind
}

// HistorySample identifies one sampled row.
type HistorySample struct {
	Seq   uint64
	Tick  uint32
	Frame uint32
}

const (
	// DefaultCounterCap leaves headroom for later subsystems while
	// keeping the sample copy bounded and predictable.
	DefaultCounterCap = 64
	// DefaultHistoryHz covers a 60 FPS frame counter; sim-only users at
	// 20 Hz still retain the full 10 minutes.
	DefaultHistoryHz      = 60
	DefaultHistorySeconds = 10 * 60
	DefaultHistorySamples = DefaultHistoryHz * DefaultHistorySeconds
)

// StandardCounters are the R-OBS-2 counters every subsystem publishes
// into as that subsystem lands. Duration counters store nanoseconds.
type StandardCounters struct {
	SimTickNS              CounterID
	SimPhaseInputNS        CounterID
	SimPhaseScriptsNS      CounterID
	SimPhaseOrdersNS       CounterID
	SimPhaseMovementNS     CounterID
	SimPhaseCombatNS       CounterID
	SimPhaseEventsNS       CounterID
	SimPhaseCleanupNS      CounterID
	RenderFrameNS          CounterID
	RenderFPS              CounterID
	RenderDrawCalls        CounterID
	RenderBatches          CounterID
	RenderInstances        CounterID
	RenderAllocsFrame      CounterID
	SimAllocsTick          CounterID
	HeapBytes              CounterID
	SimPathExpansionsTick  CounterID
	SimPathQueueDepth      CounterID
	SimEntitiesUnitsActive CounterID
	SimEntitiesMissiles    CounterID
	SimEntitiesBuffs       CounterID
	AudioVoicesActive      CounterID
	NetTurnRTTNS           CounterID
	NetInputDelayTurns     CounterID
	NetHashLagTurns        CounterID
	LuaInstructionsTick    CounterID
	LuaMemoryBytes         CounterID
}

const StandardCounterCount = 27

// Counters is a single-writer, fixed-capacity perf counter registry
// plus a fixed ring of sampled history. Register at startup; Set/Add
// and Sample are allocation-free hot-path calls.
type Counters struct {
	defs []CounterDef
	vals []int64

	meta []HistorySample
	hist []int64 // row-major: slot*counterCap + counterID

	counterCap int
	historyCap int
	total      uint64
}

// NewCounters preallocates counter slots and history rows. Registration
// may not continue after the first Sample because older rows would not
// have defined values for late counters.
func NewCounters(counterCap, historyCap int) *Counters {
	if counterCap <= 0 || counterCap > 1<<16 || historyCap <= 0 {
		panic(fmt.Sprintf("obs: bad counter/history capacity %d/%d", counterCap, historyCap))
	}
	return &Counters{
		defs:       make([]CounterDef, 0, counterCap),
		vals:       make([]int64, counterCap),
		meta:       make([]HistorySample, historyCap),
		hist:       make([]int64, counterCap*historyCap),
		counterCap: counterCap,
		historyCap: historyCap,
	}
}

// NewDefaultCounters returns the standard 10-minute/60 Hz history.
func NewDefaultCounters() *Counters { return NewCounters(DefaultCounterCap, DefaultHistorySamples) }

// Register adds one startup counter slot.
func (c *Counters) Register(name, unit string, kind CounterKind) CounterID {
	if c.total != 0 {
		panic("obs: cannot register counters after sampling starts")
	}
	if !benchToken(name) || !benchToken(unit) {
		panic("obs: counter name/unit must be non-empty bench tokens")
	}
	for i := range c.defs {
		if c.defs[i].Name == name {
			panic("obs: duplicate counter " + name)
		}
	}
	if len(c.defs) == c.counterCap {
		panic("obs: counter registry full")
	}
	id := CounterID(len(c.defs))
	c.defs = append(c.defs, CounterDef{Name: name, Unit: unit, Kind: kind})
	return id
}

func benchToken(s string) bool {
	if s == "" {
		return false
	}
	for i := 0; i < len(s); i++ {
		if s[i] <= ' ' || s[i] >= 0x7f {
			return false
		}
	}
	return true
}

// RegisterStandardCounters registers the R-OBS-2 counter table. Duration
// values are nanoseconds so sub-millisecond phases remain visible.
func RegisterStandardCounters(c *Counters) StandardCounters {
	return StandardCounters{
		SimTickNS:              c.Register("sim.tick", "ns/op", CounterDuration),
		SimPhaseInputNS:        c.Register("sim.phase.input", "ns/op", CounterDuration),
		SimPhaseScriptsNS:      c.Register("sim.phase.scripts", "ns/op", CounterDuration),
		SimPhaseOrdersNS:       c.Register("sim.phase.orders", "ns/op", CounterDuration),
		SimPhaseMovementNS:     c.Register("sim.phase.movement", "ns/op", CounterDuration),
		SimPhaseCombatNS:       c.Register("sim.phase.combat", "ns/op", CounterDuration),
		SimPhaseEventsNS:       c.Register("sim.phase.events", "ns/op", CounterDuration),
		SimPhaseCleanupNS:      c.Register("sim.phase.cleanup", "ns/op", CounterDuration),
		RenderFrameNS:          c.Register("render.frame", "ns/op", CounterDuration),
		RenderFPS:              c.Register("render.fps", "fps/op", CounterGauge),
		RenderDrawCalls:        c.Register("render.draw_calls", "count/op", CounterGauge),
		RenderBatches:          c.Register("render.batches", "count/op", CounterGauge),
		RenderInstances:        c.Register("render.instances", "count/op", CounterGauge),
		RenderAllocsFrame:      c.Register("render.allocs.frame", "allocs/op", CounterGauge),
		SimAllocsTick:          c.Register("sim.allocs.tick", "allocs/op", CounterGauge),
		HeapBytes:              c.Register("heap", "B/op", CounterGauge),
		SimPathExpansionsTick:  c.Register("sim.path.expansions", "count/op", CounterGauge),
		SimPathQueueDepth:      c.Register("sim.path.queue_depth", "count/op", CounterGauge),
		SimEntitiesUnitsActive: c.Register("sim.entities.units.active", "count/op", CounterGauge),
		SimEntitiesMissiles:    c.Register("sim.entities.missiles.active", "count/op", CounterGauge),
		SimEntitiesBuffs:       c.Register("sim.entities.buffs.active", "count/op", CounterGauge),
		AudioVoicesActive:      c.Register("audio.voices.active", "count/op", CounterGauge),
		NetTurnRTTNS:           c.Register("net.turn_rtt", "ns/op", CounterDuration),
		NetInputDelayTurns:     c.Register("net.input_delay", "turns/op", CounterGauge),
		NetHashLagTurns:        c.Register("net.hash_lag", "turns/op", CounterGauge),
		LuaInstructionsTick:    c.Register("lua.instructions.tick", "instr/op", CounterGauge),
		LuaMemoryBytes:         c.Register("lua.mem", "B/op", CounterGauge),
	}
}

// CounterCount returns the number of registered counters.
func (c *Counters) CounterCount() int { return len(c.defs) }

// HistoryCap returns the fixed sample-ring capacity.
func (c *Counters) HistoryCap() int { return c.historyCap }

// TotalSamples returns all samples ever written, including evicted rows.
func (c *Counters) TotalSamples() uint64 { return c.total }

// Len reports samples currently retained in the history ring.
func (c *Counters) Len() int {
	if c.total > uint64(c.historyCap) {
		return c.historyCap
	}
	return int(c.total)
}

// Def returns immutable counter metadata.
func (c *Counters) Def(id CounterID) CounterDef {
	c.mustID(id)
	return c.defs[id]
}

// Set assigns one counter value.
func (c *Counters) Set(id CounterID, value int64) {
	c.mustID(id)
	c.vals[id] = value
}

// Add increments one counter. Overflow intentionally follows Go's
// int64 wraparound semantics; ExportBench prints the policy.
func (c *Counters) Add(id CounterID, delta int64) {
	c.mustID(id)
	c.vals[id] += delta
}

// Value reads the current value of one counter.
func (c *Counters) Value(id CounterID) int64 {
	c.mustID(id)
	return c.vals[id]
}

func (c *Counters) mustID(id CounterID) {
	if int(id) >= len(c.defs) {
		panic(fmt.Sprintf("obs: invalid counter id %d", id))
	}
}

// Sample copies the current counter slots into the history ring.
func (c *Counters) Sample(tick, frame uint32) {
	seq := c.total
	slot := int(seq % uint64(c.historyCap))
	c.meta[slot] = HistorySample{Seq: seq, Tick: tick, Frame: frame}
	base := slot * c.counterCap
	for i := range c.defs {
		c.hist[base+i] = c.vals[i]
	}
	c.total = seq + 1
}

// SampleMeta returns the i'th retained sample, oldest first.
func (c *Counters) SampleMeta(i int) (HistorySample, bool) {
	slot, ok := c.slot(i)
	if !ok {
		return HistorySample{}, false
	}
	return c.meta[slot], true
}

// HistoryValue returns one value from the i'th retained sample, oldest
// first.
func (c *Counters) HistoryValue(i int, id CounterID) (int64, bool) {
	c.mustID(id)
	slot, ok := c.slot(i)
	if !ok {
		return 0, false
	}
	return c.hist[slot*c.counterCap+int(id)], true
}

func (c *Counters) slot(i int) (int, bool) {
	count := c.Len()
	if i < 0 || i >= count {
		return 0, false
	}
	start := c.total - uint64(count)
	return int((start + uint64(i)) % uint64(c.historyCap)), true
}

// OverflowPolicy is printed in exported artifacts for FSV and tool
// consumers.
func OverflowPolicy() string { return "int64-wraparound" }

// ExportBench writes the retained history as Go benchmark-format lines
// accepted by benchstat, one line per sample/counter.
func (c *Counters) ExportBench(w io.Writer) error {
	bw := bufio.NewWriter(w)
	if _, err := fmt.Fprintf(bw, "# litd/obs counter history counters=%d samples=%d total=%d historyCap=%d overflow=%s\n",
		len(c.defs), c.Len(), c.total, c.historyCap, OverflowPolicy()); err != nil {
		return err
	}
	for i := range c.defs {
		d := c.defs[i]
		if _, err := fmt.Fprintf(bw, "# counter id=%d name=%s unit=%s kind=%s\n", i, d.Name, d.Unit, d.Kind); err != nil {
			return err
		}
	}
	count := c.Len()
	for i := 0; i < count; i++ {
		meta, _ := c.SampleMeta(i)
		if _, err := fmt.Fprintf(bw, "# sample seq=%d tick=%d frame=%d\n", meta.Seq, meta.Tick, meta.Frame); err != nil {
			return err
		}
		slot, _ := c.slot(i)
		base := slot * c.counterCap
		for j := range c.defs {
			d := c.defs[j]
			if _, err := fmt.Fprintf(bw, "BenchmarkLITDPerf/%s/tick_%08d-1 1 %d %s\n",
				d.Name, meta.Tick, c.hist[base+j], d.Unit); err != nil {
				return err
			}
		}
	}
	return bw.Flush()
}

// ExportBenchFile writes the benchstat-compatible history to path.
func (c *Counters) ExportBenchFile(path string) error {
	f, err := os.Create(path)
	if err != nil {
		return err
	}
	defer f.Close()
	return c.ExportBench(f)
}

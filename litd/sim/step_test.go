package sim

import (
	"fmt"
	"strings"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
)

// Edge: empty world — Step advances tick 0→1 and every phase runs in
// order 1..7 exactly once.
func TestStepPhaseOrder(t *testing.T) {
	w := NewWorld(Caps{Units: 16})
	var trace []string
	w.PhaseTrace = func(tick uint32, phase int, name string) {
		trace = append(trace, fmt.Sprintf("t%d p%d:%s", tick, phase, name))
	}
	if w.Tick() != 0 {
		t.Fatalf("fresh world at tick %d", w.Tick())
	}
	w.Step()
	t.Logf("phase trace: %s", strings.Join(trace, " "))
	want := "t1 p1:input t1 p2:scripts t1 p3:orders t1 p4:movement t1 p5:combat t1 p6:events t1 p7:cleanup"
	if strings.Join(trace, " ") != want {
		t.Fatalf("phase order wrong:\ngot  %s\nwant %s", strings.Join(trace, " "), want)
	}
	if w.Tick() != 1 {
		t.Fatalf("tick %d after one Step, want 1", w.Tick())
	}
}

// Edge: 1,000 Steps — tick == 1,000 and no phase ever skipped.
func TestStepThousandTicksNoPhaseSkipped(t *testing.T) {
	w := NewWorld(Caps{Units: 16})
	counts := [8]int{}
	w.PhaseTrace = func(_ uint32, phase int, _ string) { counts[phase]++ }
	for i := 0; i < 1000; i++ {
		w.Step()
	}
	t.Logf("tick=%d phase counts 1..7: %v", w.Tick(), counts[1:])
	if w.Tick() != 1000 {
		t.Fatalf("tick %d want 1000", w.Tick())
	}
	for p := 1; p <= 7; p++ {
		if counts[p] != 1000 {
			t.Fatalf("phase %d ran %d times, want 1000", p, counts[p])
		}
	}
}

// Edge: an entity killed in phase 5 still receives its phase-6 death
// event (alive at dispatch), and is removed only in phase 7.
func TestStepKillDeferredAcrossPhases(t *testing.T) {
	w := NewWorld(Caps{Units: 16})
	victim, _ := w.CreateUnit(fixed.Vec2{X: fixed.One}, 0)

	var log []string
	w.OnCombatPhase = func(tick uint32) {
		if tick == 1 {
			w.KillUnit(victim)
			log = append(log, fmt.Sprintf("p5: killed marked, alive=%v", w.Ents.Alive(victim)))
		}
	}
	w.OnDeathEvent = func(tick uint32, id EntityID) {
		log = append(log, fmt.Sprintf("p6: death event for idx=%d, alive=%v, transform row=%d",
			id.Index(), w.Ents.Alive(id), w.Transforms.Row(id)))
	}
	w.OnSnapshot = func(tick uint32) {
		log = append(log, fmt.Sprintf("p7(end): alive=%v units=%d", w.Ents.Alive(victim), w.UnitCount()))
	}
	w.Step()
	for _, l := range log {
		t.Logf("%s", l)
	}
	want := []string{
		"p5: killed marked, alive=true",
		"p6: death event for idx=0, alive=true, transform row=0",
		"p7(end): alive=false units=0",
	}
	if fmt.Sprint(log) != fmt.Sprint(want) {
		t.Fatalf("liveness sequence wrong:\ngot  %v\nwant %v", log, want)
	}
}

// Edge: a command enqueued MID-tick (during phase 2) applies on the
// next tick's phase 1, never the current tick.
func TestStepMidTickCommandLandsNextTick(t *testing.T) {
	w := NewWorld(Caps{Units: 16})
	var applied []string
	w.OnCommand = func(tick uint32, c WorldCommand) {
		applied = append(applied, fmt.Sprintf("applied kind=%d at tick %d", c.Kind, tick))
	}
	w.OnScriptPhase = func(tick uint32) {
		if tick == 1 {
			w.EnqueueCommand(WorldCommand{Kind: 42}) // mid-tick, after phase 1 ran
		}
	}
	w.EnqueueCommand(WorldCommand{Kind: 7}) // pre-tick: applies tick 1
	w.Step()                                // tick 1: applies kind=7; kind=42 staged mid-tick
	w.Step()                                // tick 2: applies kind=42
	for _, l := range applied {
		t.Logf("%s", l)
	}
	want := []string{"applied kind=7 at tick 1", "applied kind=42 at tick 2"}
	if fmt.Sprint(applied) != fmt.Sprint(want) {
		t.Fatalf("command timing wrong:\ngot  %v\nwant %v", applied, want)
	}
}

// Hash hook fires on cadence only.
func TestStepHashCadence(t *testing.T) {
	w := NewWorld(Caps{Units: 16})
	var hashes []uint32
	w.HashEvery = 100
	w.OnHash = func(tick uint32) { hashes = append(hashes, tick) }
	for i := 0; i < 250; i++ {
		w.Step()
	}
	t.Logf("hash ticks: %v", hashes)
	if fmt.Sprint(hashes) != fmt.Sprint([]uint32{100, 200}) {
		t.Fatalf("cadence wrong: %v", hashes)
	}
}

func TestStepZeroAllocEmptyWorld(t *testing.T) {
	w := NewWorld(Caps{Units: 16})
	for i := 0; i < 8; i++ {
		w.Step()
	}
	if n := testing.AllocsPerRun(5000, func() { w.Step() }); n != 0 {
		t.Fatalf("Step allocates %v/op on empty world; R-GC-1 requires 0", n)
	}
	t.Log("AllocsPerRun = 0 for Step (empty world)")
}

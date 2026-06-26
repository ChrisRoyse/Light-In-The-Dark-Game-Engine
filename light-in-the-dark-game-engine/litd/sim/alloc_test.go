package sim

import (
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim/sched"
)

// Zero-alloc CI gates on the tick path (R-GC-1, enforced per R-GC-5
// from M3 onward; ecs-architecture.md §7-§8). Steady state means
// post-warmup: the first ticks after load may fill pools to capacity
// (load-time work, explicitly excluded); after the warmup boundary a
// tick performs zero heap allocations, measured by
// testing.AllocsPerRun and failed hard on any nonzero value.

// allocGateSink is a package-level escape sink: anything stored here
// provably escapes, so escape analysis cannot elide a deliberately
// injected allocation when proving the gate trips (the #46 lesson —
// `_ = make(...)` is elided and the gate stays green).
var allocGateSink []byte

const allocWarmupTicks = 16

// steadyStateWorld builds a populated world exercising every phase
// each tick: commands drain in phase 1, a perpetual continuation
// re-arms on the scheduler in phase 2, combat kills one unit and
// creates a replacement in phase 5, the kill's EvUnitDeath plus a
// burst of script events dispatch in phase 6, and phase 7 destroys
// the corpse. Churn without growth: every buffer reaches its
// steady-state capacity during warmup.
func steadyStateWorld(units int) (*World, *int64) {
	w := NewWorld(Caps{Units: units + 8})
	sink := new(int64)

	const contPerp sched.ContID = 1
	w.Sched.Register(contPerp, func(s *sched.Scheduler, st sched.State) {
		*sink += st[0]
		s.After(1, contPerp, st)
	})
	w.Sched.After(1, contPerp, sched.State{1})

	const evLoad uint16 = 200
	w.RegisterHandler(hA, func(w *World, e Event) { *sink += e.Arg })
	w.Subscribe(evLoad, hA)
	w.RegisterHandler(hB, func(w *World, e Event) { *sink++ })
	w.Subscribe(EvUnitDeath, hB)

	var victim EntityID
	for i := 0; i < units; i++ {
		id, ok := w.CreateUnit(fixed.Vec2{}, 0)
		if !ok {
			panic("alloc_test: unit cap hit during setup")
		}
		victim = id
	}
	w.OnScriptPhase = func(tick uint32) {
		for i := 0; i < 8; i++ {
			w.Emit(Event{Kind: evLoad, Arg: int64(i)})
		}
		w.EnqueueCommand(WorldCommand{Kind: 1, Unit: victim})
	}
	w.OnCombatPhase = func(tick uint32) {
		w.KillUnit(victim)
		if id, ok := w.CreateUnit(fixed.Vec2{}, 0); ok {
			victim = id
		}
	}
	return w, sink
}

func measureTick(t *testing.T, name string, w *World) float64 {
	t.Helper()
	tickBefore := w.Tick()
	for i := 0; i < allocWarmupTicks; i++ {
		w.Step()
	}
	allocs := testing.AllocsPerRun(100, func() { w.Step() })
	t.Logf("%s: ticks %d..%d warmup (excluded), then AllocsPerRun(100, Step) = %v",
		name, tickBefore+1, tickBefore+allocWarmupTicks, allocs)
	return allocs
}

// Gate: a steady-state tick of an EMPTY world allocates nothing.
func TestZeroAllocTickEmptyWorld(t *testing.T) {
	w := NewWorld(Caps{Units: 16})
	if a := measureTick(t, "empty world", w); a != 0 {
		t.Fatalf("R-GC-1 violated: empty-world tick allocates %v/op", a)
	}
}

// Gate: a steady-state tick of a POPULATED world (1,000 units, full
// per-tick churn across all 7 phases) allocates nothing.
func TestZeroAllocTickPopulated(t *testing.T) {
	w, sink := steadyStateWorld(1000)
	a := measureTick(t, "populated world (1000 units, churn)", w)
	t.Logf("liveness sink after run: %d (proves handlers/continuations executed)", *sink)
	if *sink == 0 {
		t.Fatalf("workload never ran — measurement is vacuous")
	}
	if a != 0 {
		t.Fatalf("R-GC-1 violated: populated tick allocates %v/op", a)
	}
}

// Diagnosis: when the whole-tick gate trips, this names the phase.
// Each phase function is measured in isolation at steady state.
func TestZeroAllocPerPhase(t *testing.T) {
	w, _ := steadyStateWorld(1000)
	for i := 0; i < allocWarmupTicks; i++ {
		w.Step()
	}
	phases := []struct {
		name string
		f    func(*World)
	}{
		{"1-input", (*World).phaseInput},
		{"2-scripts", (*World).phaseScripts},
		{"3-orders", (*World).phaseOrders},
		{"4-movement", (*World).phaseMovement},
		{"5-combat", (*World).phaseCombat},
		{"6-events", (*World).phaseEvents},
		{"7-cleanup", (*World).phaseCleanup},
	}
	for _, p := range phases {
		allocs := testing.AllocsPerRun(100, func() {
			w.tick++
			p.f(w)
		})
		t.Logf("phase %s: AllocsPerRun = %v", p.name, allocs)
		if allocs != 0 {
			t.Errorf("R-GC-1 violated in phase %s: %v allocs/op", p.name, allocs)
		}
	}
}

// Boundary proof: the gate measures only ticks AFTER the explicit
// allocWarmupTicks boundary. Today even the cold first tick is 0
// (NewWorld preallocates every pool, R-GC-2); the boundary exists so
// future load-time pool growth never silently leaks into the gate.
// Both sides are printed so the boundary stays observable.
func TestZeroAllocWarmupBoundary(t *testing.T) {
	w, _ := steadyStateWorld(1000)
	cold := testing.AllocsPerRun(1, func() { w.Step() })
	for i := 0; i < allocWarmupTicks; i++ {
		w.Step()
	}
	warm := testing.AllocsPerRun(100, func() { w.Step() })
	t.Logf("cold first tick: %v allocs/op; post-warmup tick: %v allocs/op", cold, warm)
	if warm != 0 {
		t.Fatalf("steady state allocates after explicit warmup: %v", warm)
	}
}

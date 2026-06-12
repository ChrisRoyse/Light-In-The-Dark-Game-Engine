package sim

import (
	"fmt"
	"strings"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim/sched"
)

const (
	hA HandlerID = 1
	hB HandlerID = 2
	hC HandlerID = 3

	evTest    uint16 = 100
	evCascade uint16 = 101
)

// Edge: handlers fire in registration order; re-registering in the
// opposite order flips the trace.
func TestEventDispatchRegistrationOrder(t *testing.T) {
	run := func(order []HandlerID) string {
		var trace []string
		w := NewWorld(Caps{Units: 16})
		w.RegisterHandler(hA, func(w *World, e Event) { trace = append(trace, "A") })
		w.RegisterHandler(hB, func(w *World, e Event) { trace = append(trace, "B") })
		w.RegisterHandler(hC, func(w *World, e Event) { trace = append(trace, "C") })
		for _, id := range order {
			w.Subscribe(evTest, id)
		}
		w.Emit(Event{Kind: evTest})
		w.Step() // flush happens in phase 6
		return strings.Join(trace, ",")
	}
	abc := run([]HandlerID{hA, hB, hC})
	cba := run([]HandlerID{hC, hB, hA})
	t.Logf("subscribed A,B,C -> dispatch %s", abc)
	t.Logf("subscribed C,B,A -> dispatch %s", cba)
	if abc != "A,B,C" || cba != "C,B,A" {
		t.Fatalf("registration order not honored: %q / %q", abc, cba)
	}
}

// Edge: a handler that waits mid-dispatch suspends a continuation;
// the REMAINING handlers still run in this flush and the suspended
// half resumes on a later tick.
func TestEventDispatchHandlerWaits(t *testing.T) {
	var trace []string
	w := NewWorld(Caps{Units: 16})
	const contLater sched.ContID = 7
	w.Sched.Register(contLater, func(s *sched.Scheduler, st sched.State) {
		trace = append(trace, fmt.Sprintf("t%d B-post-wait", w.Tick()))
	})
	w.RegisterHandler(hA, func(w *World, e Event) {
		trace = append(trace, fmt.Sprintf("t%d A", w.Tick()))
	})
	w.RegisterHandler(hB, func(w *World, e Event) {
		trace = append(trace, fmt.Sprintf("t%d B-pre-wait", w.Tick()))
		w.AfterMS(100, contLater, sched.State{}) // "wait": suspend the rest
	})
	w.RegisterHandler(hC, func(w *World, e Event) {
		trace = append(trace, fmt.Sprintf("t%d C", w.Tick()))
	})
	w.Subscribe(evTest, hA)
	w.Subscribe(evTest, hB)
	w.Subscribe(evTest, hC)
	w.Emit(Event{Kind: evTest})
	for i := 0; i < 4; i++ {
		w.Step()
	}
	t.Logf("trace: %v", trace)
	want := []string{"t1 A", "t1 B-pre-wait", "t1 C", "t3 B-post-wait"}
	if fmt.Sprint(trace) != fmt.Sprint(want) {
		t.Fatalf("wait-mid-dispatch wrong:\ngot  %v\nwant %v", trace, want)
	}
}

// Edge: a handler emitting a new event during flush — it dispatches
// in THIS phase, after everything already queued, in order.
func TestEventDispatchCascadeSamePhase(t *testing.T) {
	var trace []string
	w := NewWorld(Caps{Units: 16})
	w.RegisterHandler(hA, func(w *World, e Event) {
		trace = append(trace, fmt.Sprintf("t%d first arg=%d", w.Tick(), e.Arg))
		w.Emit(Event{Kind: evCascade, Arg: e.Arg + 1})
	})
	w.RegisterHandler(hB, func(w *World, e Event) {
		trace = append(trace, fmt.Sprintf("t%d second arg=%d", w.Tick(), e.Arg))
	})
	w.RegisterHandler(hC, func(w *World, e Event) {
		trace = append(trace, fmt.Sprintf("t%d cascade arg=%d", w.Tick(), e.Arg))
	})
	w.Subscribe(evTest, hA)
	w.Subscribe(evTest, hB)
	w.Subscribe(evCascade, hC)
	w.Emit(Event{Kind: evTest, Arg: 10})
	w.Step()
	t.Logf("trace: %v", trace)
	// cascade emitted by the FIRST handler still lands after the
	// already-queued evTest finishes its handler list, same tick
	want := []string{"t1 first arg=10", "t1 second arg=10", "t1 cascade arg=11"}
	if fmt.Sprint(trace) != fmt.Sprint(want) {
		t.Fatalf("cascade order wrong:\ngot  %v\nwant %v", trace, want)
	}
}

// Edge: ring overflow — 4,096 events queue, the 4,097th drops
// deterministically with the debug assert and a counted drop.
func TestEventRingOverflowDrops(t *testing.T) {
	w := NewWorld(Caps{Units: 16})
	var dropped []Event
	w.OnEventDrop = func(tick uint32, e Event) { dropped = append(dropped, e) }
	okCount := 0
	for i := 0; i < 4096; i++ {
		if w.Emit(Event{Kind: evTest, Arg: int64(i)}) {
			okCount++
		}
	}
	overflow := w.Emit(Event{Kind: evTest, Arg: 4096})
	t.Logf("queued %d OK; event 4097 -> emit=%v; EventsDropped=%d; drop record=%+v",
		okCount, overflow, w.EventsDropped(), dropped)
	if okCount != 4096 || overflow || w.EventsDropped() != 1 {
		t.Fatalf("overflow handling wrong: ok=%d overflow=%v dropped=%d", okCount, overflow, w.EventsDropped())
	}
	if len(dropped) != 1 || dropped[0].Arg != 4096 {
		t.Fatalf("drop record wrong: %+v", dropped)
	}
}

// Built-in death event: phase-5 kill emits EvUnitDeath dispatched in
// phase 6 while the entity is still alive.
func TestEventUnitDeathBuiltin(t *testing.T) {
	var trace []string
	w := NewWorld(Caps{Units: 16})
	victim, _ := w.CreateUnit(fixed.Vec2{}, 0)
	w.RegisterHandler(hA, func(w *World, e Event) {
		trace = append(trace, fmt.Sprintf("death of idx=%d alive=%v", e.Src.Index(), w.Ents.Alive(e.Src)))
	})
	w.Subscribe(EvUnitDeath, hA)
	w.OnCombatPhase = func(tick uint32) {
		if tick == 1 {
			w.KillUnit(victim)
		}
	}
	w.Step()
	t.Logf("trace: %v; alive after tick: %v", trace, w.Ents.Alive(victim))
	if fmt.Sprint(trace) != fmt.Sprint([]string{"death of idx=1 alive=true"}) || w.Ents.Alive(victim) {
		t.Fatalf("death event wrong: %v alive=%v", trace, w.Ents.Alive(victim))
	}
}

// Fail-closed registration contracts.
func TestEventRegistryFailsClosed(t *testing.T) {
	w := NewWorld(Caps{Units: 16})
	w.RegisterHandler(hA, func(*World, Event) {})
	for name, f := range map[string]func(){
		"dup-register":           func() { w.RegisterHandler(hA, func(*World, Event) {}) },
		"subscribe-unregistered": func() { w.Subscribe(evTest, HandlerID(99)) },
	} {
		func() {
			defer func() {
				if recover() == nil {
					t.Fatalf("%s did not panic", name)
				}
			}()
			f()
		}()
	}
}

func BenchmarkEventFlush(b *testing.B) {
	w := NewWorld(Caps{Units: 16})
	sink := 0
	w.RegisterHandler(hA, func(w *World, e Event) { sink += int(e.Arg) })
	w.Subscribe(evTest, hA)
	b.ReportAllocs()
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		for j := 0; j < 100; j++ {
			w.Emit(Event{Kind: evTest, Arg: int64(j)})
		}
		w.flushEvents()
	}
	_ = sink
}

// Regression for #332: EvUnitDamaged shipped as 2, colliding with
// EvMoveDone in the single dispatch namespace — a move-done subscriber
// fired on every damage packet. Every built-in kind must be unique.
func TestBuiltinEventKindsUnique(t *testing.T) {
	kinds := map[string]uint16{
		"EvUnitDeath":    EvUnitDeath,
		"EvMoveDone":     EvMoveDone,
		"EvRepathNeeded": EvRepathNeeded,
		"EvOrderIssued":  EvOrderIssued,
		"EvOrderDone":    EvOrderDone,
		"EvOrderDropped": EvOrderDropped,
		"EvUnitDamaged":  EvUnitDamaged,
	}
	byVal := map[uint16]string{}
	for name, v := range kinds {
		if prev, dup := byVal[v]; dup {
			t.Errorf("event kind %d claimed by both %s and %s", v, prev, name)
		}
		byVal[v] = name
	}
	t.Logf("built-in kinds: %v", kinds)
}

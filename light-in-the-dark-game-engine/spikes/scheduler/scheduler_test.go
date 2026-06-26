// Spike S2 (decision D-2026-06-11-9 validation): serializable stackless
// cooperative scheduler. Proves the design that replaced the goroutine-baton
// candidate: script "threads" are descriptive suspension records, so the
// entire scheduler state serializes mid-run and resumes bit-identically.
//
// A script is a state machine: Step(env) advances until it returns a
// suspension (wait N ticks / wait for event / done). No goroutines anywhere.
package scheduler

import (
	"bytes"
	"encoding/gob"
	"fmt"
	"testing"
)

type SuspendKind uint8

const (
	SuspendDone SuspendKind = iota
	SuspendTicks
	SuspendEvent
)

type Suspension struct {
	Kind  SuspendKind
	Ticks int64  // SuspendTicks: how many
	Event string // SuspendEvent: which
}

// Script is the stackless coroutine contract: resumable pure state machine.
// PC + locals fully describe the suspension point — this is what makes it
// serializable where a goroutine stack is not.
type Script struct {
	ID     int64
	PC     int64            // resume point
	Locals map[string]int64 // script-local state
}

// program is the demo script body: deterministic, branches on locals, waits.
//
//	loop 5 times: wait 3 ticks; counter += tick; if counter odd wait for "ping"
func program(s *Script, now int64, env *World) Suspension {
	switch s.PC {
	case 0:
		s.Locals["i"] = 0
		s.PC = 1
		fallthrough
	case 1:
		if s.Locals["i"] >= 5 {
			s.PC = 4
			return Suspension{Kind: SuspendDone}
		}
		s.PC = 2
		return Suspension{Kind: SuspendTicks, Ticks: 3}
	case 2:
		s.Locals["counter"] += now
		env.Trace = append(env.Trace, fmt.Sprintf("s%d c=%d t=%d", s.ID, s.Locals["counter"], now))
		if s.Locals["counter"]%2 == 1 {
			s.PC = 3
			return Suspension{Kind: SuspendEvent, Event: "ping"}
		}
		s.Locals["i"]++
		s.PC = 1
		return program(s, now, env)
	case 3:
		s.Locals["counter"] += 1000
		s.Locals["i"]++
		s.PC = 1
		return program(s, now, env)
	}
	return Suspension{Kind: SuspendDone}
}

type sleeper struct {
	WakeTick int64
	Seq      int64 // registration sequence: deterministic tie-break
	ScriptID int64
}

type waiter struct {
	Event    string
	Seq      int64
	ScriptID int64
}

// World is the full serializable scheduler state.
type World struct {
	Tick    int64
	NextSeq int64
	Scripts map[int64]*Script
	Sleep   []sleeper // kept sorted (WakeTick, Seq)
	Waiters []waiter  // FIFO per event by Seq
	Trace   []string
}

func NewWorld(nScripts int) *World {
	w := &World{Scripts: map[int64]*Script{}}
	for i := 0; i < nScripts; i++ {
		s := &Script{ID: int64(i), Locals: map[string]int64{}}
		w.Scripts[s.ID] = s
		w.schedule(s.ID, 0)
	}
	return w
}

func (w *World) schedule(id, wake int64) {
	w.NextSeq++
	sl := sleeper{WakeTick: wake, Seq: w.NextSeq, ScriptID: id}
	// insert sorted by (WakeTick, Seq) — deterministic resume order (R-EXEC-2)
	i := len(w.Sleep)
	for i > 0 && (w.Sleep[i-1].WakeTick > sl.WakeTick) {
		i--
	}
	w.Sleep = append(w.Sleep, sleeper{})
	copy(w.Sleep[i+1:], w.Sleep[i:])
	w.Sleep[i] = sl
}

func (w *World) resume(id int64) {
	s := w.Scripts[id]
	switch susp := program(s, w.Tick, w); susp.Kind {
	case SuspendTicks:
		w.schedule(id, w.Tick+susp.Ticks)
	case SuspendEvent:
		w.NextSeq++
		w.Waiters = append(w.Waiters, waiter{Event: susp.Event, Seq: w.NextSeq, ScriptID: id})
	case SuspendDone:
		delete(w.Scripts, id)
	}
}

// Step advances one tick: wake sleepers due now, fire "ping" every 7 ticks.
func (w *World) Step() {
	w.Tick++
	for len(w.Sleep) > 0 && w.Sleep[0].WakeTick <= w.Tick {
		id := w.Sleep[0].ScriptID
		w.Sleep = w.Sleep[1:]
		if _, ok := w.Scripts[id]; ok {
			w.resume(id)
		}
	}
	if w.Tick%7 == 0 {
		fired := w.Waiters
		w.Waiters = nil
		for _, wt := range fired {
			if _, ok := w.Scripts[wt.ScriptID]; ok && wt.Event == "ping" {
				w.resume(wt.ScriptID)
			}
		}
	}
}

func (w *World) Serialize() []byte {
	var buf bytes.Buffer
	if err := gob.NewEncoder(&buf).Encode(w); err != nil {
		panic(err)
	}
	return buf.Bytes()
}

func Deserialize(b []byte) *World {
	var w World
	if err := gob.NewDecoder(bytes.NewReader(b)).Decode(&w); err != nil {
		panic(err)
	}
	return &w
}

// TestSaveLoadMidRun: run 25 ticks, save, run 25 more; vs restore the save
// and run the same 25 — traces and final state must be identical.
func TestSaveLoadMidRun(t *testing.T) {
	a := NewWorld(8)
	for i := 0; i < 25; i++ {
		a.Step()
	}
	snap := a.Serialize()

	for i := 0; i < 25; i++ {
		a.Step()
	}

	b := Deserialize(snap)
	for i := 0; i < 25; i++ {
		b.Step()
	}

	if fmt.Sprint(a.Trace) != fmt.Sprint(b.Trace) {
		t.Fatalf("trace divergence after save/load:\nA: %v\nB: %v", a.Trace, b.Trace)
	}
	ha, hb := fmt.Sprint(a.Sleep, a.Waiters, a.Tick), fmt.Sprint(b.Sleep, b.Waiters, b.Tick)
	if ha != hb {
		t.Fatalf("scheduler state divergence:\nA: %s\nB: %s", ha, hb)
	}
	t.Logf("save/load mid-run: %d trace entries identical; suspended coroutines survived serialization", len(a.Trace))
}

// TestDeterministicOrder: two fresh worlds produce identical traces.
func TestDeterministicOrder(t *testing.T) {
	a, b := NewWorld(8), NewWorld(8)
	for i := 0; i < 60; i++ {
		a.Step()
		b.Step()
	}
	if fmt.Sprint(a.Trace) != fmt.Sprint(b.Trace) {
		t.Fatal("nondeterministic resume order")
	}
}

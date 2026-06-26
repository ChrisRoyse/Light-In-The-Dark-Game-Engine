package sim

import "testing"

// #615 — custom kinds flow through the existing Emit/Subscribe/ring/
// dispatch unchanged. SoT = the handler fire log (order + payload) and
// ValidEventKind's fail-closed verdict.

func TestCustomKindEndToEndDispatch(t *testing.T) {
	w := NewWorld(Caps{Units: 8})
	wave := w.CustomEvents.RegisterEventKind("wave")
	if wave == 0 {
		t.Fatal("register failed")
	}
	// Two handlers; registration order = dispatch order, even for a custom kind.
	var log []int64
	w.RegisterHandler(HandlerID(101), func(_ *World, e Event) { log = append(log, e.Arg*10) })
	w.RegisterHandler(HandlerID(102), func(_ *World, e Event) { log = append(log, e.Arg*100) })
	w.Subscribe(wave, HandlerID(101))
	w.Subscribe(wave, HandlerID(102))

	// X+X=Y payload: Arg=2+2=4 should produce [40, 400] in registration order.
	w.Emit(Event{Kind: wave, Arg: 2 + 2})
	w.Step() // phase-6 flush

	if len(log) != 2 || log[0] != 40 || log[1] != 400 {
		t.Fatalf("custom dispatch log = %v, want [40 400] (reg order, Arg=4)", log)
	}
}

func TestCustomKindGroupPayloadViaArg(t *testing.T) {
	w := NewWorld(Caps{Units: 8, UnitGroups: 16, GroupMembers: 256})
	kind := w.CustomEvents.RegisterEventKind("squadSpawned")
	g := w.Groups.CreateGroup()
	w.Groups.GroupAdd(g, makeEntityID(7, 1))
	var seenCount int32
	w.RegisterHandler(HandlerID(200), func(w *World, e Event) {
		// Arg carries the GroupID; the handler reads the group payload.
		seenCount = w.Groups.GroupCount(GroupID(uint32(e.Arg)))
	})
	w.Subscribe(kind, HandlerID(200))
	w.Emit(Event{Kind: kind, Arg: int64(uint32(g))})
	w.Step()
	if seenCount != 1 {
		t.Fatalf("group-payload handler saw count %d, want 1", seenCount)
	}
}

func TestValidEventKindFailClosed(t *testing.T) {
	w := NewWorld(Caps{Units: 8})
	reg := w.CustomEvents.RegisterEventKind("ok")
	if !w.ValidEventKind(EvUnitDeath) {
		t.Fatal("built-in kind invalid")
	}
	if !w.ValidEventKind(reg) {
		t.Fatal("registered custom kind invalid")
	}
	if w.ValidEventKind(0) {
		t.Fatal("kind 0 valid")
	}
	// An unregistered custom kind (above the registered range) is rejected.
	if w.ValidEventKind(reg + 1) {
		t.Fatal("unregistered custom kind accepted (not fail-closed)")
	}
}

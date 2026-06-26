package litd

import "testing"

// #619 — public custom-event surface. SoT = the handler's observable
// side effect (it writes what it received) after a real dispatch tick.

func TestCustomEventGoEndToEnd(t *testing.T) {
	w, g, _ := newDriverGame(t)
	kind := g.RegisterEvent("wave")
	if kind == 0 {
		t.Fatal("RegisterEvent returned 0")
	}
	// Idempotent: same name → same kind.
	if g.RegisterEvent("wave") != kind {
		t.Fatal("RegisterEvent not idempotent")
	}
	if g.EventKindByName("wave") != kind {
		t.Fatal("EventKindByName mismatch")
	}

	var gotArg int64 = -1
	var fires int
	g.OnEvent(kind, func(e Event) { gotArg = e.Arg(); fires++ })

	// X+X=Y: emit arg 2+2=4; the handler must observe 4 after dispatch.
	if !g.Emit(kind, Unit{}, Unit{}, 2+2) {
		t.Fatal("Emit returned false")
	}
	w.Step() // phase-6 dispatch

	if fires != 1 || gotArg != 4 {
		t.Fatalf("handler fires=%d arg=%d, want 1 and 4", fires, gotArg)
	}

	// Fail-closed: emitting an unregistered custom kind is refused.
	if g.Emit(kind+1, Unit{}, Unit{}, 0) {
		t.Fatal("Emit of unregistered kind returned true")
	}
}

func TestCustomEventEmitGroup(t *testing.T) {
	w, g, _ := newDriverGame(t)
	kind := g.RegisterEvent("squad")
	grp := g.NewGroup()
	grp.Add(grpUnit(t, w, g, 0))
	grp.Add(grpUnit(t, w, g, 0))

	var seen int
	g.OnEvent(kind, func(e Event) {
		// arg carries the GroupID; reconstruct + read its count.
		seen = e.GroupArg().Count()
	})
	g.EmitGroup(kind, Unit{}, grp)
	w.Step()
	if seen != 2 {
		t.Fatalf("group-payload handler saw count %d, want 2", seen)
	}
}

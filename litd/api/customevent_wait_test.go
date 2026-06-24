package litd

import (
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
)

// #616 — WaitForEvent over custom kinds. SoT = the sim subscription list
// (IsSubscribed): SubscribeScriptEvent must resolve a registered custom
// kind to itself and subscribe the reserved dispatcher, so a coroutine
// parked on it wakes when the custom event fires; an unregistered kind
// fails closed.
func TestSubscribeScriptEventCustomKind(t *testing.T) {
	w := sim.NewWorld(sim.Caps{Units: 8})
	g := newGame(w)

	g.RegisterScriptEventDispatcher(func(uint16) {}) // VM-setup contract before Subscribe

	kind := w.CustomEvents.RegisterEventKind("wave")
	if kind == 0 {
		t.Fatal("register failed")
	}
	// BEFORE: dispatcher not subscribed to the custom kind.
	if w.IsSubscribed(kind, scriptEventHandlerID) {
		t.Fatal("dispatcher already subscribed before wait")
	}
	sk, ok := g.SubscribeScriptEvent(EventKind(kind))
	if !ok || sk != kind {
		t.Fatalf("SubscribeScriptEvent(custom) = %d,%v, want %d,true", sk, ok, kind)
	}
	// AFTER (SoT): the dispatcher is now subscribed to the custom kind.
	if !w.IsSubscribed(kind, scriptEventHandlerID) {
		t.Fatal("dispatcher not subscribed to custom kind after SubscribeScriptEvent")
	}
	// Idempotent: a second subscribe does not double-register.
	g.SubscribeScriptEvent(EventKind(kind))
	cnt := 0
	for _, s := range w.SubsSnapshot() {
		if s.Kind == kind {
			cnt = len(s.Handlers)
		}
	}
	if cnt != 1 {
		t.Fatalf("custom kind has %d handlers after double subscribe, want 1 (idempotent)", cnt)
	}

	// Fail-closed: an unregistered custom kind resolves to ok=false.
	if _, ok := g.SubscribeScriptEvent(EventKind(kind + 1)); ok {
		t.Fatal("unregistered custom kind resolved (not fail-closed)")
	}
}

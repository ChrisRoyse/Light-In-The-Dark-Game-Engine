package litd

// FSV for the #478 api surface: Trigger.BindName. SoT = the sim's name→trigger
// resolution after binding through the public Trigger handle.

import (
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
)

// TestTriggerBindNameFSV — binding a trigger to a name through the api makes the
// sim resolve that name to the trigger's id; fail-closed on a duplicate.
func TestTriggerBindNameFSV(t *testing.T) {
	w := sim.NewWorld(sim.Caps{Units: 4})
	g := newGame(w)
	tr := g.NewTrigger()
	if !tr.Valid() {
		t.Fatal("NewTrigger invalid")
	}
	if !tr.BindName("spell") {
		t.Fatal("BindName returned false")
	}
	tid, ok := w.TriggerByName("spell")
	t.Logf("FSV #478 api: BindName(spell) → sim resolves ok=%v id=%d, count=%d", ok, tid, w.NamedTriggerCount())
	if !ok || w.NamedTriggerCount() != 1 {
		t.Fatalf("sim did not resolve the bound name: ok=%v count=%d", ok, w.NamedTriggerCount())
	}
	// duplicate name → fail-closed.
	tr2 := g.NewTrigger()
	if tr2.BindName("spell") {
		t.Fatal("duplicate BindName accepted")
	}
	// invalid handle → false, no panic.
	var zero Trigger
	if zero.BindName("x") {
		t.Fatal("zero-value Trigger BindName accepted")
	}
	t.Log("FSV #478 api fail-closed: duplicate name and invalid handle both refused")
}

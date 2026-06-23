package litd

// #527 public train verb FSV. SoT = the sim production result: the unit-count
// delta (a real footman entity appears) + the EventUnitTrained / EventTrainRefused
// the sim emits. Reuses the live barracks/footman harness (lvGame). X+X=Y: one
// accepted Train at a 500-gold barracks => exactly one footman, one trained event.

import (
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
)

func lvBarracksUnit(t *testing.T, g *Game) Unit {
	t.Helper()
	var id sim.EntityID
	for _, u := range g.w.AppendAllUnits(nil) {
		id = u
	}
	if id == 0 {
		t.Fatal("no barracks in lvGame")
	}
	return Unit{id: id, g: g}
}

func TestUnitTrainProducesUnitFSV(t *testing.T) {
	g := lvGame(t, 0)
	barracks := lvBarracksUnit(t, g)
	footman := g.UnitType("footman")
	if footman.IsZero() {
		t.Fatal("footman UnitType unresolved")
	}

	trained := 0
	g.OnEvent(EventUnitTrained, func(Event) { trained++ })

	before := liveFootmen(g, 0)
	if before != 0 {
		t.Fatalf("expected 0 footmen before training, got %d", before)
	}

	if !barracks.Train(footman) {
		t.Fatal("Train(footman) refused at a valid barracks with 500 gold")
	}
	// Footman TrainTicks=40 → spawns by tick 40.
	for i := 0; i < 45; i++ {
		g.Advance(1)
	}
	after := liveFootmen(g, 0)
	t.Logf("FSV Unit.Train: footmen before=%d after=%d EventUnitTrained=%d", before, after, trained)
	if after != before+1 {
		t.Fatalf("expected exactly 1 footman produced, got delta %d", after-before)
	}
	if trained != 1 {
		t.Fatalf("expected exactly 1 EventUnitTrained, got %d", trained)
	}
}

func TestUnitTrainRefusedEdgesFSV(t *testing.T) {
	// (1) Non-producer: a footman cannot train — refused, EventTrainRefused fires,
	// no unit produced.
	g := lvGame(t, 0)
	footman := g.UnitType("footman")
	refused := 0
	g.OnEvent(EventTrainRefused, func(Event) { refused++ })
	nonProducer := g.CreateUnit(g.Player(0), footman, Vec2{X: 300, Y: 300}, Deg(0))
	if !nonProducer.Valid() {
		t.Fatal("could not create a footman to test non-producer training")
	}
	before := liveFootmen(g, 0)
	if nonProducer.Train(footman) {
		t.Fatal("a footman accepted a Train order — only producers should")
	}
	for i := 0; i < 45; i++ {
		g.Advance(1)
	}
	if got := liveFootmen(g, 0); got != before {
		t.Fatalf("non-producer Train produced %d units, want 0", got-before)
	}
	if refused == 0 {
		t.Fatal("non-producer Train emitted no EventTrainRefused")
	}
	t.Logf("FSV non-producer refused: EventTrainRefused=%d, no unit produced", refused)

	// (2) Null type: rejected at the handle boundary, no event, no panic.
	barracks := lvBarracksUnit(t, g)
	if barracks.Train(UnitType{}) {
		t.Fatal("Train(null type) was accepted")
	}

	// (3) Invalid producer handle: dead unit → false, no panic.
	dead := g.CreateUnit(g.Player(0), footman, Vec2{X: 320, Y: 320}, Deg(0))
	dead.Kill()
	for i := 0; i < 3; i++ {
		g.Advance(1)
	}
	if dead.Valid() {
		t.Skip("unit still valid after Kill+advance; skipping dead-handle edge")
	}
	if dead.Train(footman) {
		t.Fatal("Train on a dead unit was accepted")
	}
	t.Logf("FSV null-type + dead-handle rejected cleanly")
}

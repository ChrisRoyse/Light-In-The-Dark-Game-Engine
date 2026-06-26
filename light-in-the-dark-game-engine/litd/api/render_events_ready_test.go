package litd

// FSV of the api render-event accessor's "ready" mapping (#313): a sim
// RenderUnitReady cue (staged when a unit finishes training) surfaces through
// Game.RenderEvents as a value-type api.RenderUnitReady carrying the trained unit's
// UnitType. SoT = the drained RenderEvent slice after a real training completes.
// Reuses the live AI-domain harness's barracks/footman world (lvGame); trains
// directly via the sim queue so the test depends on nothing but production.

import (
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
)

func TestRenderEventsReadyMappingFSV(t *testing.T) {
	g := lvGame(t, 0)

	// The only unit at start is the barracks — find it.
	var barracks sim.EntityID
	for _, id := range g.w.AppendAllUnits(nil) {
		barracks = id
	}
	if barracks == 0 {
		t.Fatal("no barracks")
	}
	if got := g.w.EnqueueTrain(barracks, lvFootman); got != sim.TrainOK {
		t.Fatalf("EnqueueTrain: reason %d", got)
	}

	wantType := g.UnitType("footman")
	if wantType.IsZero() {
		t.Fatal("footman UnitType unresolved")
	}

	// Footman TrainTicks=40 → spawns at tick 40; drain each tick (RenderEvents only
	// holds the current tick's cues).
	var ready []RenderEvent
	var buf []RenderEvent
	for i := 0; i < 45; i++ {
		g.Advance(1)
		buf = g.RenderEvents(buf)
		for _, e := range buf {
			if e.Kind == RenderUnitReady {
				ready = append(ready, e)
			}
		}
	}
	if len(ready) != 1 {
		t.Fatalf("RenderUnitReady events drained = %d, want exactly 1", len(ready))
	}
	if ready[0].UnitType != wantType {
		t.Fatalf("ready event UnitType = %v, want footman %v (api accessor mis-mapped the type)", ready[0].UnitType, wantType)
	}
	t.Logf("FSV #313 api: trained footman → Game.RenderEvents drained 1 RenderUnitReady carrying the footman UnitType")
}

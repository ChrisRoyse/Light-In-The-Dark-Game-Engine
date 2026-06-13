package litd

import (
	"math"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
)

// rawPos reads the unit's position straight out of the sim Transform store —
// the Source of Truth behind Position().
func rawPos(w *sim.World, id sim.EntityID) (Vec2, bool) {
	r := w.Transforms.Row(id)
	if r < 0 {
		return Vec2{}, false
	}
	p := w.Transforms.Pos[r]
	return Vec2{X: toFloat(p.X), Y: toFloat(p.Y)}, true
}

// TestUnitPositionFSV: the getter agrees with the Transform store before and
// after a teleport. Known input → known output (spawn at 64,64).
func TestUnitPositionFSV(t *testing.T) {
	w := sim.NewWorld(sim.Caps{Units: 16})
	g := newGame(w)
	u, id := liveUnit(t, w, g, 0, 100)

	// BEFORE: liveUnit spawns at (64,64).
	got := u.Position()
	sot, _ := rawPos(w, id)
	t.Logf("spawn: Position()=%+v store=%+v", got, sot)
	if got != (Vec2{X: 64, Y: 64}) || got != sot {
		t.Fatalf("Position()=%+v store=%+v, want {64 64}", got, sot)
	}

	// ACTION: teleport to a known new position via the sim, re-read getter.
	if !w.TeleportUnit(id, vec(Vec2{X: 200, Y: -50})) {
		t.Fatal("TeleportUnit failed")
	}
	got = u.Position()
	sot, _ = rawPos(w, id)
	t.Logf("after teleport: Position()=%+v store=%+v", got, sot)
	if got != (Vec2{X: 200, Y: -50}) || got != sot {
		t.Fatalf("Position()=%+v store=%+v, want {200 -50}", got, sot)
	}

	// EDGE: zero-value handle is a safe no-op getter.
	if z := (Unit{}).Position(); z != (Vec2{}) {
		t.Fatalf("zero Unit Position()=%+v, want zero", z)
	}
}

// TestUnitFacingFSV: SetFacing writes the Transform store; the getter reads it
// back. 90° round-trips exactly through the brad encoding (16384/65536).
func TestUnitFacingFSV(t *testing.T) {
	w := sim.NewWorld(sim.Caps{Units: 16})
	g := newGame(w)
	u, id := liveUnit(t, w, g, 0, 100)

	const eps = 0.01 // brad quantization is ~0.0055°
	check := func(label string, setDeg, wantDeg float64) {
		u.SetFacing(Deg(setDeg))
		r := w.Transforms.Row(id)
		storeDeg := angleFromBrad(w.Transforms.Facing[r]).Degrees()
		getDeg := u.Facing().Degrees()
		t.Logf("%s: SetFacing(%.0f°) -> store=%.4f° getter=%.4f°", label, setDeg, storeDeg, getDeg)
		if math.Abs(storeDeg-wantDeg) > eps || math.Abs(getDeg-wantDeg) > eps {
			t.Fatalf("%s: store=%.4f getter=%.4f, want ~%.4f", label, storeDeg, getDeg, wantDeg)
		}
	}
	check("cardinal", 90, 90)
	// EDGE: 450° wraps to 90° in the unsigned brad encoding.
	check("wrap", 450, 90)
	// EDGE: zero-value handle SetFacing is a no-op (must not panic).
	(Unit{}).SetFacing(Deg(45))
}

// TestUnitKillFiresDeathThenDestroys: Kill marks the unit; after one Step the
// death event fires exactly once and the unit is gone from the store.
func TestUnitKillFiresDeathThenDestroys(t *testing.T) {
	w := sim.NewWorld(sim.Caps{Units: 16})
	g := newGame(w)
	u, id := liveUnit(t, w, g, 0, 100)

	deaths := 0
	var deadID sim.EntityID
	g.OnEvent(EventUnitDeath, func(e Event) {
		deaths++
		deadID = e.Unit().id
	})

	t.Logf("BEFORE Kill: Valid=%v storeRow=%d", u.Valid(), w.Transforms.Row(id))
	u.Kill()
	// Kill is deferred: still alive until the step resolves it.
	if !u.Valid() {
		t.Fatal("unit must remain valid until Step resolves the kill")
	}
	w.Step()

	_, present := rawPos(w, id)
	t.Logf("AFTER Kill+Step: deaths=%d deadID==unit=%v Valid=%v storePresent=%v",
		deaths, deadID == id, u.Valid(), present)
	if deaths != 1 {
		t.Fatalf("death event fired %d times, want 1", deaths)
	}
	if deadID != id {
		t.Fatalf("death event carried entity %v, want %v", deadID, id)
	}
	if u.Valid() || present {
		t.Fatalf("killed unit still present: Valid=%v storePresent=%v", u.Valid(), present)
	}

	// EDGE: Kill on the now-dead handle is a silent no-op, no second death.
	u.Kill()
	w.Step()
	if deaths != 1 {
		t.Fatalf("Kill on dead unit fired another death event (deaths=%d)", deaths)
	}
}

// TestUnitRemoveIsSilentAndImmediate: Remove deletes the unit immediately with
// NO death event — the Kill/Remove distinction. SoT: store absence + no event.
func TestUnitRemoveIsSilentAndImmediate(t *testing.T) {
	w := sim.NewWorld(sim.Caps{Units: 16})
	g := newGame(w)
	u, id := liveUnit(t, w, g, 0, 100)

	deaths := 0
	g.OnEvent(EventUnitDeath, func(Event) { deaths++ })

	_, before := rawPos(w, id)
	u.Remove()
	_, after := rawPos(w, id)
	t.Logf("Remove: storePresent %v -> %v, Valid=%v, deaths(immediate)=%d", before, after, u.Valid(), deaths)
	if !before {
		t.Fatal("unit not present before Remove")
	}
	if after || u.Valid() {
		t.Fatalf("unit still present after Remove: storePresent=%v Valid=%v", after, u.Valid())
	}

	// Step to prove no deferred death event was queued by Remove.
	w.Step()
	if deaths != 0 {
		t.Fatalf("Remove fired %d death event(s), want 0 (Remove is silent)", deaths)
	}

	// EDGE: Remove on an invalid handle is a no-op.
	u.Remove()
	(Unit{}).Remove()
}

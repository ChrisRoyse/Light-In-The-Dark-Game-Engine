package sim

import (
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
)

// #584 — Linear/Point/Homing advance. SoT = the Target's Transform.Pos
// read directly after stepping.

func moverPosOf(w *World, id EntityID) fixed.Vec2 {
	return w.Transforms.Pos[w.Transforms.Row(id)]
}

func TestMoverLinearStepsAndExpires(t *testing.T) {
	w := NewWorld(Caps{Units: 8, Movers: 16})
	id, _ := w.CreateUnit(fixed.Vec2{}, 0) // at origin, no Movements → only the mover drives it
	m := w.Movers.Create(MoverSpec{
		Kind: MoverLinear, Target: id,
		Dir:   fixed.Vec2{X: fixed.One, Y: 0}, // +X unit vector
		Speed: 10 * fixed.One, RangeLeft: 25 * fixed.One,
	})
	// X+X=Y: 10/tick along +X. After 2 ticks pos=(20,0), still alive.
	w.Step()
	w.Step()
	if p := moverPosOf(w, id); p.X != 20*fixed.One || p.Y != 0 {
		t.Fatalf("after 2 ticks pos=(%d,%d), want (20,0)", p.X, p.Y)
	}
	if !w.Movers.Alive(m) {
		t.Fatal("mover expired early (range 25 > 20)")
	}
	// 3rd tick: pos=(30,0) and range (25-30<0) → expire.
	w.Step()
	if p := moverPosOf(w, id); p.X != 30*fixed.One {
		t.Fatalf("after 3 ticks pos.X=%d, want 30", p.X)
	}
	if w.Movers.Alive(m) {
		t.Fatal("mover did not expire after range exhausted")
	}
}

func TestMoverPointArrivesAndSnaps(t *testing.T) {
	w := NewWorld(Caps{Units: 8, Movers: 16})
	id, _ := w.CreateUnit(fixed.Vec2{}, 0)
	goal := fixed.Vec2{X: 25 * fixed.One, Y: 0}
	m := w.Movers.Create(MoverSpec{Kind: MoverPoint, Target: id, Goal: goal, Speed: 10 * fixed.One})
	// 10,20, then within-10 of 25 → snap to exactly 25 + expire.
	for i := 0; i < 3; i++ {
		w.Step()
	}
	if p := moverPosOf(w, id); p != goal {
		t.Fatalf("point mover ended at (%d,%d), want exact goal (%d,0)", p.X, p.Y, goal.X)
	}
	if w.Movers.Alive(m) {
		t.Fatal("point mover did not expire on arrival")
	}
}

func TestMoverHomingTurnsTowardAnchor(t *testing.T) {
	w := NewWorld(Caps{Units: 8, Movers: 16})
	proj, _ := w.CreateUnit(fixed.Vec2{}, 0)
	anchor, _ := w.CreateUnit(fixed.Vec2{X: 0, Y: 100 * fixed.One}, 0) // due +Y
	w.Movers.Create(MoverSpec{
		Kind: MoverHoming, Target: proj, Anchor: anchor,
		Dir:   fixed.Vec2{X: fixed.One, Y: 0}, // initially +X
		Speed: 10 * fixed.One, TurnRate: 0,    // instant turn
	})
	w.Step()
	// Instant homing → first step heads +Y toward the anchor.
	p := moverPosOf(w, proj)
	if p.Y <= 0 || p.X != 0 {
		t.Fatalf("homing first step = (%d,%d), want straight +Y toward anchor", p.X, p.Y)
	}
}

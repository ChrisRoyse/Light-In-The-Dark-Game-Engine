package litd

import (
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
)

// #591 — api Move* verbs + SpawnProjectile. SoT = the target unit's
// Position() after advancing the game.

func moverTestUnit(g *Game, x, y float64) Unit {
	id, _ := g.w.CreateUnit(fixed.Vec2{X: fromFloat(x), Y: fromFloat(y)}, 0)
	return Unit{id: id, g: g}
}

func TestAPIMovePoint(t *testing.T) {
	g, err := NewGame(GameOptions{MaxUnits: 16, Seed: 1})
	if err != nil {
		t.Fatal(err)
	}
	u := moverTestUnit(g, 0, 0)
	m := g.MovePoint(MoverOptions{Target: u, Goal: Vec2{X: 50}, Speed: 10})
	if !m.Valid() {
		t.Fatal("MovePoint returned invalid handle")
	}
	g.Advance(10) // 10/tick → reaches and snaps to (50,0), then expires
	p := u.Position()
	if p.X != 50 || p.Y != 0 {
		t.Fatalf("unit at (%.1f,%.1f), want (50,0)", p.X, p.Y)
	}
	if m.Valid() {
		t.Fatal("point mover should have expired on arrival")
	}
}

func TestAPIMoveLinear(t *testing.T) {
	g, _ := NewGame(GameOptions{MaxUnits: 16, Seed: 1})
	u := moverTestUnit(g, 0, 0)
	g.MoveLinear(MoverOptions{Target: u, Direction: Deg(0), Speed: 10, Range: 1000}) // +X
	g.Advance(3)
	if p := u.Position(); p.X != 30 {
		t.Fatalf("linear x=%.1f, want 30", p.X)
	}
}

func TestAPISpawnProjectileAndCancel(t *testing.T) {
	g, _ := NewGame(GameOptions{MaxUnits: 16, Seed: 1})
	before := g.w.Movers.Count()
	m := g.SpawnProjectile(Vec2{X: 0}, ProjectilePoint, MoverOptions{Goal: Vec2{X: 100}, Speed: 5})
	if !m.Valid() || g.w.Movers.Count() != before+1 {
		t.Fatalf("SpawnProjectile did not create a live mover (count %d->%d)", before, g.w.Movers.Count())
	}
	m.Cancel()
	if m.Valid() {
		t.Fatal("Cancel did not free the mover")
	}
	m.Cancel() // idempotent
}

func TestAPIMoveOrbitPoint(t *testing.T) {
	g, _ := NewGame(GameOptions{MaxUnits: 16, Seed: 1})
	u := moverTestUnit(g, 0, 0)
	g.MoveOrbitPoint(MoverOptions{Target: u, Goal: Vec2{X: 100, Y: 50}, Radius: 10, AngVel: Deg(90)})
	g.Advance(1) // angle→90° → center + (0,10) = (100,60)
	p := u.Position()
	if int(p.X+0.5) != 100 || int(p.Y+0.5) != 60 {
		t.Fatalf("orbit at (%.2f,%.2f), want ~(100,60)", p.X, p.Y)
	}
}

func TestAPIMoveSplineNeedsTwoPoints(t *testing.T) {
	g, _ := NewGame(GameOptions{MaxUnits: 16, Seed: 1})
	u := moverTestUnit(g, 0, 0)
	if m := g.MoveSpline(MoverOptions{Target: u, Waypoints: []Vec2{{X: 1}}, Speed: 1}); !m.IsZero() {
		t.Fatal("spline with one waypoint should fail with zero handle")
	}
}

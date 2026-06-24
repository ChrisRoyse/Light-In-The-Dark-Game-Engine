package sim

import (
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
)

const quarterBAM = fixed.Angle(0x4000) // 90°
const halfBAM = fixed.Angle(0x8000)    // 180°

func TestMoverOrbitPoint(t *testing.T) {
	w := NewWorld(Caps{Units: 8, Movers: 16})
	id, _ := w.CreateUnit(fixed.Vec2{}, 0)
	center := fixed.Vec2{X: 100 * fixed.One, Y: 50 * fixed.One}
	w.Movers.Create(MoverSpec{
		Kind: MoverOrbitPoint, Target: id, Goal: center,
		Radius: 10 * fixed.One, AngVel: quarterBAM, Angle: 0,
	})
	// Step1: angle→quarter, UnitVec=(0,1) → center+(0,10)=(100,60).
	w.Step()
	if p := moverPosOf(w, id); p.X != center.X || p.Y != center.Y+10*fixed.One {
		t.Fatalf("orbit step1 = (%d,%d), want (100,60)", p.X, p.Y)
	}
	// Step2: angle→half, UnitVec=(-1,0) → (90,50).
	w.Step()
	if p := moverPosOf(w, id); p.X != center.X-10*fixed.One || p.Y != center.Y {
		t.Fatalf("orbit step2 = (%d,%d), want (90,50)", p.X, p.Y)
	}
}

func TestMoverArcApexAndLand(t *testing.T) {
	w := NewWorld(Caps{Units: 8, Movers: 16})
	id, _ := w.CreateUnit(fixed.Vec2{}, 0)
	goal := fixed.Vec2{X: 40 * fixed.One, Y: 0}
	m := w.Movers.Create(MoverSpec{
		Kind: MoverArc, Target: id, Goal: goal, Speed: 10 * fixed.One,
		CState: [4]int64{int64(40 * fixed.One), int64(20 * fixed.One)}, // total, apex
	})
	r, _ := w.Movers.resolve(m)
	// Step to midpoint (t=0.5) → z == apex 20.
	w.Step() // (10,0) t=.25 z=15
	w.Step() // (20,0) t=.5  z=20 apex
	// z within a few ulps of apex (div/sqrt round to ~1 raw; ~2e-10 wu).
	dz := w.Movers.Height[r] - 20*fixed.One
	if dz < 0 {
		dz = -dz
	}
	if dz > 16 {
		t.Fatalf("arc apex z=%d, want ~20 (=%d)", w.Movers.Height[r], 20*fixed.One)
	}
	// Land: more steps reach goal → snap + z=0 + expire.
	w.Step()
	w.Step()
	if p := moverPosOf(w, id); p != goal {
		t.Fatalf("arc landed at (%d,%d), want exact goal", p.X, p.Y)
	}
	if w.Movers.Alive(m) {
		t.Fatal("arc did not expire on land")
	}
}

func TestMoverSplineThroughWaypoints(t *testing.T) {
	w := NewWorld(Caps{Units: 8, Movers: 16, MoverWaypoints: 64})
	id, _ := w.CreateUnit(fixed.Vec2{}, 0)
	pts := []fixed.Vec2{
		{X: 0, Y: 0},
		{X: 10 * fixed.One, Y: 0},
		{X: 10 * fixed.One, Y: 10 * fixed.One},
		{X: 20 * fixed.One, Y: 10 * fixed.One},
	}
	start, n, ok := w.Movers.AddWaypoints(pts)
	if !ok {
		t.Fatal("AddWaypoints failed")
	}
	m := w.Movers.Create(MoverSpec{
		Kind: MoverSpline, Target: id, WpStart: start, WpLen: n,
		Speed: fixed.One, // 1 param unit/tick → lands on each control point
	})
	// Step1: param=1 → Catmull-Rom at integer param == waypoint[1] exactly.
	w.Step()
	if p := moverPosOf(w, id); p != pts[1] {
		t.Fatalf("spline@1 = (%d,%d), want waypoint[1] (10,0)", p.X, p.Y)
	}
	// Step2: param=2 → waypoint[2].
	w.Step()
	if p := moverPosOf(w, id); p != pts[2] {
		t.Fatalf("spline@2 = (%d,%d), want waypoint[2] (10,10)", p.X, p.Y)
	}
	// Step3: param=3 == last → snap to final + expire.
	w.Step()
	if p := moverPosOf(w, id); p != pts[3] {
		t.Fatalf("spline end = (%d,%d), want last (20,10)", p.X, p.Y)
	}
	if w.Movers.Alive(m) {
		t.Fatal("spline did not expire at end")
	}
}

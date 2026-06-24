package fixed

import "testing"

// #583 — mover-facing trig helpers over the committed quarter-wave LUT.

func TestUnitVecCardinals(t *testing.T) {
	cases := []struct {
		a    Angle
		x, y F64
	}{
		{0, One, 0},              // +X
		{quarterTurn, 0, One},    // +Y
		{halfTurn, -One, 0},      // -X
		{halfTurn + quarterTurn, 0, -One}, // -Y
	}
	for _, c := range cases {
		uv := c.a.UnitVec()
		if uv.X != c.x || uv.Y != c.y {
			t.Fatalf("UnitVec(%d) = (%d,%d), want (%d,%d)", c.a, uv.X, uv.Y, c.x, c.y)
		}
	}
}

func TestRotateQuarterTurns(t *testing.T) {
	v := Vec2{X: One, Y: 0} // +X
	r1 := v.Rotate(quarterTurn)
	if r1.X != 0 || r1.Y != One {
		t.Fatalf("rotate +X by 90 = (%d,%d), want (0,One)", r1.X, r1.Y)
	}
	r2 := v.Rotate(halfTurn)
	if r2.X != -One || r2.Y != 0 {
		t.Fatalf("rotate +X by 180 = (%d,%d), want (-One,0)", r2.X, r2.Y)
	}
	// Rotating by a full turn (wraparound) is identity.
	if v.Rotate(0) != v {
		t.Fatal("rotate by 0 not identity")
	}
}

// Orbit placement: center + radius·UnitVec(angle) lands on the circle.
func TestOrbitPlacement(t *testing.T) {
	center := Vec2{X: 100 * One, Y: 50 * One}
	radius := 10 * One
	// At quarter turn, the point is directly +Y of center.
	p := center.Add(quarterTurn.UnitVec().Scale(radius))
	if p.X != center.X || p.Y != center.Y+radius {
		t.Fatalf("orbit@quarter = (%d,%d), want (%d,%d)", p.X, p.Y, center.X, center.Y+radius)
	}
}

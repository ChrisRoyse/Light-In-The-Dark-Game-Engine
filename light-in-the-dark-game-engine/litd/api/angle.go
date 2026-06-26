package litd

// Angle normalization and Vec2 geometry — the deterministic-survivor
// math of the math-strings-conversion category. Trigonometry routes
// through the sim's fixed-point tables (AngleTo, Polar), never math.Sin/
// Atan2, whose last bit varies across architectures (golang/go#20319).
// Distance uses math.Sqrt, which IEEE-754 mandates be correctly rounded
// and is therefore platform-stable.

import (
	"math"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
)

// Normalized returns the angle reduced to [0, 2π). JASS facing reals
// wrapped arbitrarily; this ends the ambiguity.
func (a Angle) Normalized() Angle {
	const twoPi = 2 * math.Pi
	r := math.Mod(a.rad, twoPi)
	if r < 0 {
		r += twoPi
	}
	return Angle{rad: r}
}

// DistanceTo returns the Euclidean distance between two points. JASS:
// DistanceBetweenPoints.
// JASS: DistanceBetweenPoints
func (v Vec2) DistanceTo(o Vec2) float64 {
	dx, dy := o.X-v.X, o.Y-v.Y
	return math.Sqrt(dx*dx + dy*dy)
}

// AngleTo returns the heading from v toward o, via the deterministic
// fixed-point arctangent. JASS: AngleBetweenPoints.
// JASS: AngleBetweenPoints
func (v Vec2) AngleTo(o Vec2) Angle {
	return angleFromBrad(fixed.Atan2(fromFloat(o.Y-v.Y), fromFloat(o.X-v.X)))
}

// Polar returns the point dist world units from v along heading a, via
// the deterministic fixed-point sine/cosine. JASS:
// PolarProjectionBJ.
// JASS: PolarProjectionBJ
func (v Vec2) Polar(a Angle, dist float64) Vec2 {
	b := angleToBrad(a)
	return Vec2{
		X: v.X + dist*toFloat(b.Cos()),
		Y: v.Y + dist*toFloat(b.Sin()),
	}
}

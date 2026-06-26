package litd

// Randomness and localization survivors of the math-strings-conversion
// category (jass-mapping/math-strings-conversion.md). The JASS random
// natives map onto the sim's single seeded PRNG (R-SIM-2) — never
// math/rand, whose global state would desync a deterministic match.
// SetRandomSeed reseeds that stream; the seed change is part of the
// hashed sim state, so two runs diverge after different seeds and
// reconverge after the same one.

import (
	"math"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
)

// RandomInt returns a uniformly-distributed integer in [min, max]
// inclusive, drawn from the sim PRNG. A degenerate range returns min.
// JASS: GetRandomInt
func (g *Game) RandomInt(min, max int) int {
	if g == nil || g.w == nil {
		return min
	}
	return int(g.w.RandRange(int32(min), int32(max)))
}

// RandomFloat returns a uniformly-distributed value in [0, 1) drawn
// from the sim PRNG. JASS: GetRandomReal(0, 1) (scale the result for
// other ranges).
// JASS: GetRandomReal
func (g *Game) RandomFloat() float64 {
	if g == nil || g.w == nil {
		return 0
	}
	return toFloat(g.w.RandUnit())
}

// RandomAngle returns a uniformly-distributed Angle drawn from the sim
// PRNG.
// JASS: GetRandomDirectionDeg
func (g *Game) RandomAngle() Angle {
	if g == nil || g.w == nil {
		return Angle{}
	}
	return angleFromBrad(g.w.RandAngle())
}

// SetRandomSeed reseeds the sim PRNG. The reseed is deterministic sim
// state: identical command streams that reseed identically stay in
// lockstep. JASS: SetRandomSeed.
// JASS: SetRandomSeed
func (g *Game) SetRandomSeed(seed int64) {
	if g != nil && g.w != nil {
		g.w.SetSeed(uint64(seed))
	}
}

// Localize returns the localized form of a string key. No localization
// table is bound yet, so it is the identity passthrough — the documented
// canonical default until string tables ship. JASS: GetLocalizedString.
func (g *Game) Localize(s string) string { return s }

// angleFromBrad converts a sim binary angle to the public Angle.
func angleFromBrad(b fixed.Angle) Angle {
	const bradToRad = (2 * math.Pi) / 65536.0
	return Angle{rad: float64(uint16(b)) * bradToRad}
}

// angleToBrad converts a public Angle to the sim binary angle (the
// inverse unit conversion). The float step is a plain scale; the
// trigonometry it feeds is the deterministic fixed-point table.
func angleToBrad(a Angle) fixed.Angle {
	const radToBrad = 65536.0 / (2 * math.Pi)
	return fixed.Angle(uint16(int64(a.rad * radToBrad)))
}

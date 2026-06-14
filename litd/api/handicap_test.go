package litd

import (
	"math"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
)

// TestPlayerHandicapsAPIFSV — the #373 handicap accessors round-trip
// through the sim. SoT: the sim handicap getters read back directly (not
// just the API return), proving the API writes real applied state.
func TestPlayerHandicapsAPIFSV(t *testing.T) {
	w := sim.NewWorld(sim.Caps{})
	g := newGame(w)
	p := g.Player(0)

	approx := func(got float64, want float64) bool { return math.Abs(got-want) < 1e-6 }

	// fresh slot reads 1.0 (the no-op default) through the API.
	t.Logf("FSV default: handicap=%.3f dmg=%.3f xp=%.3f revive=%.3f",
		p.Handicap(), p.HandicapDamage(), p.HandicapXP(), p.HandicapReviveTime())
	if !approx(p.Handicap(), 1) || !approx(p.HandicapDamage(), 1) ||
		!approx(p.HandicapXP(), 1) || !approx(p.HandicapReviveTime(), 1) {
		t.Fatal("fresh handicaps not 1.0")
	}

	// write through the API, read the sim SoT back as fixed-point.
	p.SetHandicap(0.6)
	p.SetHandicapDamage(0.5)
	p.SetHandicapXP(1.5)
	p.SetHandicapReviveTime(0.5)

	t.Logf("FSV write: api(handicap=%.3f dmg=%.3f xp=%.3f revive=%.3f) sot(handicap=%d dmg=%d xp=%d revive=%d)",
		p.Handicap(), p.HandicapDamage(), p.HandicapXP(), p.HandicapReviveTime(),
		int64(w.Handicap(0)), int64(w.HandicapDamage(0)), int64(w.HandicapXP(0)), int64(w.HandicapReviveTime(0)))

	if w.HandicapDamage(0) != fromFloat(0.5) {
		t.Fatalf("sot handicapDamage = %d, want %d", int64(w.HandicapDamage(0)), int64(fromFloat(0.5)))
	}
	if !approx(p.HandicapXP(), 1.5) || w.HandicapXP(0) != fromFloat(1.5) {
		t.Fatalf("xp round-trip: api=%.3f sot=%d", p.HandicapXP(), int64(w.HandicapXP(0)))
	}

	// edge: negative clamps to 0 at the sim, read back as 0 through the API.
	p.SetHandicap(-2)
	t.Logf("FSV clamp: SetHandicap(-2) -> api=%.3f sot=%d (want 0)", p.Handicap(), int64(w.Handicap(0)))
	if p.Handicap() != 0 || w.Handicap(0) != 0 {
		t.Fatalf("negative not clamped: api=%.3f sot=%d", p.Handicap(), int64(w.Handicap(0)))
	}

	// edge: invalid handle is a clean no-op that reads the 1.0 default.
	oob := g.Player(99)
	oob.SetHandicapXP(0.1) // must not panic, must not write
	t.Logf("FSV invalid: oob.HandicapXP()=%.3f (want 1.0)", oob.HandicapXP())
	if !approx(oob.HandicapXP(), 1) {
		t.Fatalf("invalid-handle handicap = %.3f, want 1.0", oob.HandicapXP())
	}
}

package litd

import (
	"math"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
)

// TestUnitPropWindowAPIFSV — Unit.PropWindow/SetPropWindow/DefaultPropWindow
// through the public API (radians). SoT: the radian value read back from
// the sim. Default 45° (pi/4) from the unit table; an override round-trips;
// invalid handle reads 0.
func TestUnitPropWindowAPIFSV(t *testing.T) {
	w := sim.NewWorld(sim.Caps{Units: 8})
	// 0x2000 BAM = quarter of a half-turn = 45° = pi/4 rad.
	if !w.BindUnitDefs([]data.Unit{{ID: "rider", Life: 100,
		MoveSpeedPerTick: 4 * fixed.One, TurnRatePerTick: 0x2000,
		CollisionSize: 16, PropWindow: 0x2000}}) {
		t.Fatal("BindUnitDefs failed")
	}
	g := newGame(w)
	id, ok := w.SpawnFromTable(0, 0, 0, fixed.Vec2{X: fixed.FromInt(100), Y: fixed.FromInt(100)})
	if !ok {
		t.Fatal("spawn failed")
	}
	u := Unit{id: id, g: g}

	approx := func(got, want float64) bool { return math.Abs(got-want) < 0.01 }
	t.Logf("FSV default: PropWindow=%.4f DefaultPropWindow=%.4f (want pi/4=%.4f)", u.PropWindow(), u.DefaultPropWindow(), math.Pi/4)
	if !approx(u.PropWindow(), math.Pi/4) || !approx(u.DefaultPropWindow(), math.Pi/4) {
		t.Fatalf("default prop window = %.4f, want pi/4", u.PropWindow())
	}

	// override to pi/8, read back; default unchanged.
	u.SetPropWindow(math.Pi / 8)
	t.Logf("FSV override: PropWindow=%.4f (want pi/8=%.4f) default=%.4f", u.PropWindow(), math.Pi/8, u.DefaultPropWindow())
	if !approx(u.PropWindow(), math.Pi/8) || !approx(u.DefaultPropWindow(), math.Pi/4) {
		t.Fatalf("override = %.4f, want pi/8", u.PropWindow())
	}

	// negative clamps to 0.
	u.SetPropWindow(-1)
	t.Logf("FSV clamp: SetPropWindow(-1) -> %.4f (want 0)", u.PropWindow())
	if u.PropWindow() != 0 {
		t.Fatalf("negative not clamped: %.4f", u.PropWindow())
	}

	var inv Unit
	if inv.PropWindow() != 0 || inv.DefaultPropWindow() != 0 {
		t.Fatal("invalid handle prop window should be 0")
	}
	inv.SetPropWindow(1) // no-op, no panic
}

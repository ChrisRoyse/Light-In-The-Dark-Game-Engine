package litd

import (
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
)

// TestUnitFlyHeightAPIFSV — Unit.FlyHeight/SetFlyHeight/DefaultFlyHeight
// through the public API. SoT: the height read back after a Step-driven
// climb (rate is world-units/sec → per-tick). Default fly height 50 from
// the unit data table; at TicksPerSecond ticks/sec a 32/sec climb is
// 32/TicksPerSecond per tick.
func TestUnitFlyHeightAPIFSV(t *testing.T) {
	w := sim.NewWorld(sim.Caps{Units: 8})
	if !w.BindUnitDefs([]data.Unit{{ID: "gryphon", Life: 100, FlyHeight: fixed.FromInt(50),
		MoveSpeedPerTick: 2 * fixed.One, CollisionSize: 16}}) {
		t.Fatal("BindUnitDefs failed")
	}
	g := newGame(w)
	id, ok := w.SpawnFromTable(0, 0, 0, fixed.Vec2{X: fixed.FromInt(100), Y: fixed.FromInt(100)})
	if !ok {
		t.Fatal("spawn failed")
	}
	u := Unit{id: id, g: g}

	t.Logf("FSV default: FlyHeight=%.1f DefaultFlyHeight=%.1f (want 50/50)", u.FlyHeight(), u.DefaultFlyHeight())
	if u.FlyHeight() != 50 || u.DefaultFlyHeight() != 50 {
		t.Fatalf("default fly height wrong: %.1f", u.FlyHeight())
	}

	// climb to 100 at a rate of one full tick-step per tick: pick
	// rate = TicksPerSecond units/sec so per-tick step = 1 unit.
	u.SetFlyHeight(100, float64(data.TicksPerSecond))
	w.Step()
	t.Logf("FSV climb t+1: FlyHeight=%.1f (want 51)", u.FlyHeight())
	if u.FlyHeight() != 51 {
		t.Fatalf("after 1 tick fly height = %.1f, want 51", u.FlyHeight())
	}

	// snap: rate 0 jumps instantly.
	u.SetFlyHeight(80, 0)
	t.Logf("FSV snap: FlyHeight=%.1f (want 80)", u.FlyHeight())
	if u.FlyHeight() != 80 {
		t.Fatalf("snap fly height = %.1f, want 80", u.FlyHeight())
	}

	// negative height clamps to 0.
	u.SetFlyHeight(-5, 0)
	t.Logf("FSV clamp: SetFlyHeight(-5) -> %.1f (want 0)", u.FlyHeight())
	if u.FlyHeight() != 0 {
		t.Fatalf("negative height not clamped: %.1f", u.FlyHeight())
	}

	// invalid handle: clean 0, no panic.
	var inv Unit
	if inv.FlyHeight() != 0 || inv.DefaultFlyHeight() != 0 {
		t.Fatal("invalid handle fly height should be 0")
	}
	inv.SetFlyHeight(10, 1) // no-op, no panic
}

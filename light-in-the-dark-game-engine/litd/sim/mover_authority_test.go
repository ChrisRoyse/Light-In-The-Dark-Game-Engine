package sim

import (
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim/path"
)

// #588 — authority suspends pathing; flying ignores terrain. SoT = unit
// Transform.Pos after stepping.

func TestMoverAuthorityControlPathingMoves(t *testing.T) {
	// Control: no mover → the unit's own order movement runs (+X).
	w := NewWorld(Caps{Units: 8, Movers: 8})
	unit, _ := w.CreateUnit(fixed.Vec2{}, 0)
	w.Movements.Add(w.Ents, w.Transforms, unit, 10*fixed.One, fixed.Angle(0xFFFF))
	w.StartMoveTo(unit, fixed.Vec2{X: 1000 * fixed.One})
	w.Step()
	if p := moverPosOf(w, unit); p.X <= 0 {
		t.Fatalf("control: unit did not move under its own order: x=%d", p.X)
	}
}

func TestMoverAuthoritySuspendsPathing(t *testing.T) {
	w := NewWorld(Caps{Units: 8, Movers: 8})
	unit, _ := w.CreateUnit(fixed.Vec2{}, 0)
	w.Movements.Add(w.Ents, w.Transforms, unit, 10*fixed.One, fixed.Angle(0xFFFF))
	w.StartMoveTo(unit, fixed.Vec2{X: 1000 * fixed.One}) // wants +X
	// MoverAuthority drives the unit +Y instead; its own +X order suspends.
	w.Movers.Create(MoverSpec{
		Kind: MoverPoint, Target: unit, Goal: fixed.Vec2{Y: 1000 * fixed.One},
		Speed: 5 * fixed.One, Flags: MoverAuthority,
	})
	w.Step()
	p := moverPosOf(w, unit)
	if p.X != 0 {
		t.Fatalf("unit moved +X (%d) — authority did not suspend its pathing", p.X)
	}
	if p.Y <= 0 {
		t.Fatalf("authority mover did not drive the unit +Y: y=%d", p.Y)
	}
}

func TestMoverTerrainBlocksGroundNotFlying(t *testing.T) {
	mk := func(flying bool) fixed.F64 {
		w := NewWorld(Caps{Units: 8, Movers: 8})
		g := path.NewGrid()
		for x := int32(0); x <= 8; x++ {
			g.SetFlags(x, 0, path.Walkable)
		}
		g.SetFlags(3, 0, 0) // cell x=3 (world x∈[96,128)) is a wall
		w.SetGrid(g)
		proj, _ := w.CreateUnit(CellCenter(0), 0) // cell (0,0)
		var flags uint8
		if flying {
			flags = MoverFlying
		}
		w.Movers.Create(MoverSpec{
			Kind: MoverPoint, Target: proj, Goal: fixed.Vec2{X: 300 * fixed.One, Y: CellCenter(0).Y},
			Speed: 32 * fixed.One, Flags: flags, // ~one cell/tick
		})
		for i := 0; i < 8; i++ {
			w.Step()
		}
		return moverPosOf(w, proj).X
	}
	ground := mk(false)
	flying := mk(true)
	// Ground mover is stopped before the wall cell (world x < 96).
	if ground >= 96*fixed.One {
		t.Fatalf("ground mover passed the wall: x=%d (want < 96)", ground)
	}
	// Flying mover sails through to (near) the goal.
	if flying <= 96*fixed.One {
		t.Fatalf("flying mover blocked by terrain: x=%d (want > 96)", flying)
	}
}

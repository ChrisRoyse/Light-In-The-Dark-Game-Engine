package sim

import (
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
)

// #586 — custom mover step. SoT = Target Transform.Pos + the mover's
// CState column read directly after stepping.

func TestMoverCustomStepDrivesMotion(t *testing.T) {
	w := NewWorld(Caps{Units: 8, Movers: 16})
	const stepID uint16 = 7
	// Custom step: move +X by CState[0] each tick; CState[1] counts ticks;
	// done after 3 ticks. X+X=Y: a 5-per-tick advance.
	w.Movers.RegisterMoverStep(stepID, func(w *World, pos fixed.Vec2, cs [4]int64) (fixed.Vec2, [4]int64, bool) {
		delta := fixed.Vec2{X: fixed.F64(cs[0]), Y: 0}
		cs[1]++
		return moverCustomNext(pos, delta), cs, cs[1] >= 3
	})
	id, _ := w.CreateUnit(fixed.Vec2{}, 0)
	m := w.Movers.Create(MoverSpec{
		Kind: MoverCustom, Target: id, Cont: stepID,
		CState: [4]int64{int64(5 * fixed.One), 0, 0, 0},
	})
	r, _ := w.Movers.resolve(m)
	w.Step() // pos (5,0), cs[1]=1
	w.Step() // pos (10,0), cs[1]=2
	if p := moverPosOf(w, id); p.X != 10*fixed.One {
		t.Fatalf("after 2 ticks pos.X=%d, want 10", p.X)
	}
	if w.Movers.CState[r][1] != 2 {
		t.Fatalf("CState tick counter=%d, want 2", w.Movers.CState[r][1])
	}
	if !w.Movers.Alive(m) {
		t.Fatal("expired early")
	}
	w.Step() // cs[1]=3 → done
	if w.Movers.Alive(m) {
		t.Fatal("custom mover did not complete at done")
	}
}

func TestMoverCustomUnregisteredFailsClosed(t *testing.T) {
	w := NewWorld(Caps{Units: 8, Movers: 16})
	id, _ := w.CreateUnit(fixed.Vec2{}, 0)
	m := w.Movers.Create(MoverSpec{Kind: MoverCustom, Target: id, Cont: 99}) // never registered
	w.Step()
	if w.Movers.Alive(m) {
		t.Fatal("unregistered custom mover should fail closed (complete), still alive")
	}
}

func TestRegisterMoverStepGuards(t *testing.T) {
	w := NewWorld(Caps{Movers: 4})
	mustPanic := func(name string, f func()) {
		defer func() {
			if recover() == nil {
				t.Fatalf("%s did not panic", name)
			}
		}()
		f()
	}
	mustPanic("id 0", func() { w.Movers.RegisterMoverStep(0, func(*World, fixed.Vec2, [4]int64) (fixed.Vec2, [4]int64, bool) { return fixed.Vec2{}, [4]int64{}, false }) })
	mustPanic("nil fn", func() { w.Movers.RegisterMoverStep(1, nil) })
	fn := func(*World, fixed.Vec2, [4]int64) (fixed.Vec2, [4]int64, bool) { return fixed.Vec2{}, [4]int64{}, false }
	w.Movers.RegisterMoverStep(2, fn)
	mustPanic("dup", func() { w.Movers.RegisterMoverStep(2, fn) })
}

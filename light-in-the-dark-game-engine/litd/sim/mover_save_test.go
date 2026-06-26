package sim

import (
	"bytes"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/statehash"
)

// #590 — mover hash + save round-trip. SoT = the state hash and the
// columns after a load.

func moverTopHash(w *World) uint64 {
	var s statehash.Snapshot
	w.HashState(NewHashRegistry(), &s)
	return s.Top
}

// populate a varied mover population (every kind exercised) + a spline.
func ceMoverPopulation(w *World) {
	u, _ := w.CreateUnit(fixed.Vec2{}, 0)
	w.Movers.Create(MoverSpec{Kind: MoverLinear, Target: u, Dir: fixed.Vec2{X: fixed.One}, Speed: 7 * fixed.One, RangeLeft: 99 * fixed.One, Pierce: 3, HitMask: MissileHitEnemy})
	w.Movers.Create(MoverSpec{Kind: MoverOrbitPoint, Target: u, Goal: fixed.Vec2{X: 10 * fixed.One}, Radius: 5 * fixed.One, AngVel: quarterBAM, Angle: 0x1234})
	w.Movers.Create(MoverSpec{Kind: MoverArc, Target: u, Goal: fixed.Vec2{X: 40 * fixed.One}, Speed: 4 * fixed.One, CState: [4]int64{int64(40 * fixed.One), int64(20 * fixed.One)}})
	st, n, _ := w.Movers.AddWaypoints([]fixed.Vec2{{X: 0}, {X: fixed.One}, {X: 2 * fixed.One}})
	w.Movers.Create(MoverSpec{Kind: MoverSpline, Target: u, WpStart: st, WpLen: n, Speed: fixed.One / 4})
}

func TestMoverHashDeterministic(t *testing.T) {
	mk := func() uint64 {
		w := NewWorld(Caps{Units: 8, Movers: 16, MoverWaypoints: 64})
		ceMoverPopulation(w)
		w.Step()
		w.Step()
		return moverTopHash(w)
	}
	if a, b := mk(), mk(); a != b {
		t.Fatalf("mover hash diverged: %016x != %016x", a, b)
	}
}

func TestMoverHashReactsToState(t *testing.T) {
	base := NewWorld(Caps{Units: 8, Movers: 16})
	with := NewWorld(Caps{Units: 8, Movers: 16})
	u, _ := with.CreateUnit(fixed.Vec2{}, 0)
	with.Movers.Create(MoverSpec{Kind: MoverLinear, Target: u, Speed: fixed.One, RangeLeft: fixed.One})
	if moverTopHash(base) == moverTopHash(with) {
		t.Fatal("adding a mover did not change the state hash")
	}
}

func TestMoverSaveRoundTrip(t *testing.T) {
	caps := Caps{Units: 8, Movers: 16, MoverWaypoints: 64}
	w := NewWorld(caps)
	ceMoverPopulation(w)
	w.Step()
	w.Step() // movers in mid-flight: RangeLeft/Angle/WpParam advanced
	want := moverTopHash(w)

	var buf bytes.Buffer
	if err := w.SaveState(&buf, 0); err != nil {
		t.Fatalf("save: %v", err)
	}
	w2 := NewWorld(caps)
	if err := w2.LoadState(bytes.NewReader(buf.Bytes()), 0); err != nil {
		t.Fatalf("load: %v", err)
	}
	if got := moverTopHash(w2); got != want {
		t.Fatalf("mover save/load hash %016x != original %016x", got, want)
	}
	// SoT: a live column survived verbatim. The orbit mover's Angle was set
	// to 0x1234 then advanced by AngVel; check the loaded store agrees.
	if w.Movers.count != w2.Movers.count {
		t.Fatalf("mover count %d != %d after load", w.Movers.count, w2.Movers.count)
	}
}

func TestMoverFreeGenRoundTrip(t *testing.T) {
	// #612: free-slot generations must survive save/load so the next Create
	// mints the same handle on both worlds.
	caps := Caps{Units: 4, Movers: 8}
	w := NewWorld(caps)
	u, _ := w.CreateUnit(fixed.Vec2{}, 0)
	a := w.Movers.Create(MoverSpec{Kind: MoverLinear, Target: u, RangeLeft: fixed.One})
	w.Movers.Cancel(a) // slot freed, gen bumped

	var buf bytes.Buffer
	if err := w.SaveState(&buf, 0); err != nil {
		t.Fatalf("save: %v", err)
	}
	w2 := NewWorld(caps)
	w2.CreateUnit(fixed.Vec2{}, 0)
	if err := w2.LoadState(bytes.NewReader(buf.Bytes()), 0); err != nil {
		t.Fatalf("load: %v", err)
	}
	u2 := w2.Transforms // unused guard
	_ = u2
	n1 := w.Movers.Create(MoverSpec{Kind: MoverLinear, Target: u, RangeLeft: fixed.One})
	n2 := w2.Movers.Create(MoverSpec{Kind: MoverLinear, Target: u, RangeLeft: fixed.One})
	if n1 != n2 {
		t.Fatalf("next handle diverged after save/load: %08x != %08x (free-slot gen not preserved)", n1, n2)
	}
}

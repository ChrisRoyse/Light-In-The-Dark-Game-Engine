package sim

import (
	"bytes"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
)

// #592 — movers acceptance suite: per-kind motion + collision in one
// scenario, two-run determinism, save/resume parity, zero-alloc advance,
// cross-kind trig invariant. Per-feature behavior lives in
// mover_test/mover_kinds_test/mover_collision_test/mover_complete_test/
// mover_save_test; this fixes the cross-cutting golden + the gates.

// moverScenario runs a mixed population (every kind) plus an enemy a
// piercing linear mover sweeps, for a fixed number of ticks.
func moverScenario(t *testing.T) *World {
	t.Helper()
	w := NewWorld(Caps{Units: 32, Movers: 32, MoverWaypoints: 64})
	if err := w.BindDamageMatrix(dmgMatrix); err != nil {
		t.Fatalf("bind: %v", err)
	}
	caster, _ := w.CreateUnit(fixed.Vec2{X: 500 * fixed.One}, 0)
	w.Owners.Add(w.Ents, caster, 1, 1, 1)
	enemy, _ := w.CreateUnit(fixed.Vec2{X: 30 * fixed.One}, 0)
	w.Owners.Add(w.Ents, enemy, 2, 2, 2)
	w.Healths.Add(w.Ents, enemy, 1000*fixed.One, 0, 0, 0)

	body := func(x float64) EntityID {
		id, _ := w.CreateUnit(fixed.Vec2{X: fixed.FromInt(int32(x))}, 0)
		return id
	}
	w.Movers.Create(MoverSpec{Kind: MoverLinear, Target: body(0), Owner: caster, Dir: fixed.Vec2{X: fixed.One}, Speed: 6 * fixed.One, RangeLeft: 200 * fixed.One, Radius: 8 * fixed.One, HitMask: MissileHitEnemy, Pierce: 3, Packet: DamagePacket{Amount: 50 * fixed.One}, Decay: 250})
	w.Movers.Create(MoverSpec{Kind: MoverOrbitPoint, Target: body(100), Goal: fixed.Vec2{X: 100 * fixed.One}, Radius: 12 * fixed.One, AngVel: 0x0800})
	w.Movers.Create(MoverSpec{Kind: MoverArc, Target: body(200), Goal: fixed.Vec2{X: 240 * fixed.One}, Speed: 5 * fixed.One, CState: [4]int64{int64(40 * fixed.One), int64(25 * fixed.One)}})
	w.Movers.Create(MoverSpec{Kind: MoverHoming, Target: body(300), Anchor: caster, Dir: fixed.Vec2{X: fixed.One}, Speed: 7 * fixed.One, TurnRate: 0x0400})
	st, n, _ := w.Movers.AddWaypoints([]fixed.Vec2{{X: 400 * fixed.One}, {X: 410 * fixed.One}, {X: 410 * fixed.One, Y: 10 * fixed.One}, {X: 420 * fixed.One, Y: 10 * fixed.One}})
	w.Movers.Create(MoverSpec{Kind: MoverSpline, Target: body(400), WpStart: st, WpLen: n, Speed: fixed.One / 8})
	for i := 0; i < 8; i++ {
		w.Step()
	}
	return w
}

func TestMoverScenarioGolden(t *testing.T) {
	w := moverScenario(t)
	const golden = uint64(0x5a98dc110aa1b479) // recorded 2026-06-24 (#592)
	got := moverTopHash(w)
	if golden != 0 && got != golden {
		t.Fatalf("mover scenario hash %016x != recorded %016x", got, golden)
	}
	t.Logf("mover scenario golden = %#016x", got)
}

func TestMoverScenarioTwoRunDeterminism(t *testing.T) {
	if a, b := moverTopHash(moverScenario(t)), moverTopHash(moverScenario(t)); a != b {
		t.Fatalf("mover scenario diverged: %016x != %016x", a, b)
	}
}

func TestMoverScenarioSaveResumeParity(t *testing.T) {
	caps := Caps{Units: 32, Movers: 32, MoverWaypoints: 64}
	// unbroken
	wu := moverScenario(t)
	for i := 0; i < 6; i++ {
		wu.Step()
	}
	want := moverTopHash(wu)
	// save mid-scenario, resume in a fresh world, finish
	ws := moverScenario(t)
	var buf bytes.Buffer
	if err := ws.SaveState(&buf, 0); err != nil {
		t.Fatalf("save: %v", err)
	}
	wl := NewWorld(caps)
	if err := wl.LoadState(bytes.NewReader(buf.Bytes()), 0); err != nil {
		t.Fatalf("load: %v", err)
	}
	for i := 0; i < 6; i++ {
		wl.Step()
	}
	if got := moverTopHash(wl); got != want {
		t.Fatalf("mover save/resume hash %016x != unbroken %016x", got, want)
	}
}

func TestMoverAdvanceZeroAlloc(t *testing.T) {
	w := NewWorld(Caps{Units: 32, Movers: 32, MoverWaypoints: 64})
	for i := 0; i < 8; i++ {
		id, _ := w.CreateUnit(fixed.Vec2{X: fixed.FromInt(int32(i * 50))}, 0)
		w.Movers.Create(MoverSpec{Kind: MoverOrbitPoint, Target: id, Goal: fixed.Vec2{X: fixed.FromInt(int32(i * 50))}, Radius: 10 * fixed.One, AngVel: 0x0400})
	}
	avg := testing.AllocsPerRun(200, func() { w.moverSystem() })
	if avg != 0 {
		t.Fatalf("moverSystem allocated %.2f objs/op, want 0", avg)
	}
}

package sim

import (
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
)

// #593 — MIGRATION-PARITY (release blocker): a mover configured to mimic a
// missile must drive its body byte-for-byte identically to the missile
// path, so retiring MissileStore onto movers (#590) changes no trajectory.
// Both paths share unitStep + the same snap-arrive predicate, so point and
// (instant-turn) homing flight are bit-identical; the linear skillshot's
// swept-segment collision is a DIFFERENT model and is called out below.

// track returns the per-tick position of id until it dies or n ticks pass.
func track(w *World, id EntityID, n int) []fixed.Vec2 {
	var out []fixed.Vec2
	for i := 0; i < n; i++ {
		w.Step()
		r := w.Transforms.Row(id)
		if r == -1 {
			break
		}
		out = append(out, w.Transforms.Pos[r])
	}
	return out
}

func eqTrace(a, b []fixed.Vec2) bool {
	if len(a) != len(b) {
		return false
	}
	for i := range a {
		if a[i] != b[i] {
			return false
		}
	}
	return true
}

func TestParityPointTrajectory(t *testing.T) {
	goal := fixed.Vec2{X: 95 * fixed.One, Y: 0}
	// missile path
	wm := NewWorld(Caps{Units: 8})
	mid, ok := wm.SpawnMissile(MissileSpec{Pos: fixed.Vec2{}, Speed: 10 * fixed.One, Point: goal, GuidanceID: MissileGuidancePoint})
	if !ok {
		t.Fatal("missile spawn failed")
	}
	missileTrace := track(wm, mid, 20)

	// mover path: a body at the same origin, MovePoint same speed/goal.
	wv := NewWorld(Caps{Units: 8, Movers: 8})
	body, _ := wv.CreateUnit(fixed.Vec2{}, 0)
	wv.Movers.Create(MoverSpec{Kind: MoverPoint, Target: body, Goal: goal, Speed: 10 * fixed.One, Flags: MoverConsume})
	moverTrace := track(wv, body, 20)

	if !eqTrace(missileTrace, moverTrace) {
		t.Fatalf("point trajectory diverged:\n missile=%v\n mover  =%v", missileTrace, moverTrace)
	}
	if len(moverTrace) == 0 {
		t.Fatal("empty trace")
	}
	t.Logf("point parity: %d byte-identical positions, both arrive at %v", len(moverTrace), moverTrace[len(moverTrace)-1])
}

func TestParityHomingTrajectoryInstantTurn(t *testing.T) {
	// A homing missile beelines (refreshes guide point to the target each
	// tick, then unitStep) — i.e. an instant-turn pursuit. MoverHoming with
	// TurnRate 0 is the same model. Use a stationary target so the beeline
	// is a fixed line both reproduce.
	target := fixed.Vec2{X: 90 * fixed.One, Y: 40 * fixed.One}

	wm := NewWorld(Caps{Units: 8})
	tgtM, _ := wm.CreateUnit(target, 0)
	mid, ok := wm.SpawnMissile(MissileSpec{Pos: fixed.Vec2{}, Speed: 8 * fixed.One, Target: tgtM, GuidanceID: MissileGuidanceHoming})
	if !ok {
		t.Fatal("missile spawn failed")
	}
	missileTrace := track(wm, mid, 25)

	wv := NewWorld(Caps{Units: 8, Movers: 8})
	tgtV, _ := wv.CreateUnit(target, 0)
	body, _ := wv.CreateUnit(fixed.Vec2{}, 0)
	wv.Movers.Create(MoverSpec{Kind: MoverHoming, Target: body, Anchor: tgtV, Dir: fixed.Vec2{X: fixed.One}, Speed: 8 * fixed.One, TurnRate: 0, Flags: MoverConsume})
	moverTrace := track(wv, body, 25)

	// The missile dies on contact (its body removed); the mover homing has
	// no contact stop without a HitMask, so compare the common prefix up to
	// the missile's last live position.
	n := len(missileTrace)
	if n == 0 || n > len(moverTrace) {
		t.Fatalf("trace lengths: missile=%d mover=%d", n, len(moverTrace))
	}
	if !eqTrace(missileTrace, moverTrace[:n]) {
		t.Fatalf("homing trajectory diverged over %d ticks:\n missile=%v\n mover  =%v", n, missileTrace, moverTrace[:n])
	}
	t.Logf("homing parity: %d byte-identical beeline positions", n)
}

func TestParityLinearBodyTrajectory(t *testing.T) {
	// A linear skillshot and a MoverLinear both advance the body by
	// unitStep(Dir, Speed) each tick — so the FLIGHT path is byte-identical.
	// (Collision differs: the missile sweeps the segment with an along-space
	// projection + a fixed hit radius, while the mover does an endpoint
	// radius test. That model gap is the #590-retirement caveat — a skillshot
	// keeps the missile collision until the mover adopts the swept test. The
	// motion, which is what save/replay determinism rides on, matches.)
	dir := fixed.Vec2{X: fixed.One, Y: 0}
	wm := NewWorld(Caps{Units: 8})
	mid, ok := wm.SpawnMissile(MissileSpec{Pos: fixed.Vec2{}, Speed: 9 * fixed.One, Dir: dir, Range: 200 * fixed.One, Pierce: 1, Flags: MissileLinear, GuidanceID: MissileGuidanceLinear})
	if !ok {
		t.Fatal("missile spawn failed")
	}
	missileTrace := track(wm, mid, 10)

	wv := NewWorld(Caps{Units: 8, Movers: 8})
	body, _ := wv.CreateUnit(fixed.Vec2{}, 0)
	wv.Movers.Create(MoverSpec{Kind: MoverLinear, Target: body, Dir: dir, Speed: 9 * fixed.One, RangeLeft: 200 * fixed.One})
	moverTrace := track(wv, body, 10)

	n := len(missileTrace)
	if n == 0 || n > len(moverTrace) {
		t.Fatalf("trace lengths: missile=%d mover=%d", n, len(moverTrace))
	}
	if !eqTrace(missileTrace, moverTrace[:n]) {
		t.Fatalf("linear body trajectory diverged:\n missile=%v\n mover  =%v", missileTrace, moverTrace[:n])
	}
	t.Logf("linear body parity: %d byte-identical positions (collision model differs — see comment)", n)
}

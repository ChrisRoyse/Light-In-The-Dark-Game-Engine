package sim

// #380 ground-truth verification. The discovery claims (a) attack-move to a
// point neither moves nor acquires, and (b) OrderAttack on an out-of-range
// target does not path into range. But attack.go:163-173 already chases an
// engaged out-of-range target. Code and doc disagree — so RUN it and read the
// SoT (Transform positions + target Life), never trust the claim.

import (
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
)

func fx(n int) fixed.F64 { return fixed.F64(n) * fixed.One }

// TestOrderAttackPursuesOutOfRangeTarget — claim (b). Attacker has a weapon of
// range 100 and a target 1000 away; with a Movement component it must close to
// within range and draw the target's Life down.
func TestOrderAttackPursuesOutOfRangeTarget(t *testing.T) {
	w := NewWorld(Caps{})
	if err := w.BindDamageMatrix(atkMatrix); err != nil {
		t.Fatal(err)
	}
	w.SetAcquireInterval(1)
	a := atkUnit(t, w, 0, fixed.Vec2{X: fx(1000), Y: fx(1000)}, fx(60))
	arm(t, w, a, 0, 0) // range 100
	v := atkUnit(t, w, 1, fixed.Vec2{X: fx(2000), Y: fx(1000)}, 0)

	ar, vr := w.Transforms.Row(a), w.Healths.Row(v)
	startX := w.Transforms.Pos[ar].X
	lifeBefore := w.Healths.Life[vr]
	t.Logf("FSV before: attacker.X=%d victim.Life=%d (range=100, gap=1000)", startX/fixed.One, lifeBefore/fixed.One)

	if !w.IssueOrder(a, Order{Kind: OrderAttack, Target: v}, false) {
		t.Fatal("issue OrderAttack")
	}
	for i := 0; i < 120; i++ {
		w.Step()
	}
	endX := w.Transforms.Pos[ar].X
	lifeAfter := w.Healths.Life[vr]
	t.Logf("FSV after 120 ticks: attacker.X=%d victim.Life=%d", endX/fixed.One, lifeAfter/fixed.One)

	if endX <= startX {
		t.Fatalf("attacker did not pursue: X %d -> %d", startX/fixed.One, endX/fixed.One)
	}
	if lifeAfter >= lifeBefore {
		t.Fatalf("pursuit reached no damage: victim Life %d -> %d", lifeBefore/fixed.One, lifeAfter/fixed.One)
	}
}

// TestAttackMoveToPointBehavior — claim (a). Issue OrderAttack with a far point
// and NO target, an enemy sitting off the direct path but well inside
// acquisition range. Observe what the sim actually does today: does the unit
// advance toward the point, and does it acquire the enemy?
func TestAttackMoveToPointBehavior(t *testing.T) {
	w := NewWorld(Caps{})
	if err := w.BindDamageMatrix(atkMatrix); err != nil {
		t.Fatal(err)
	}
	w.SetAcquireInterval(1)
	a := atkUnit(t, w, 0, fixed.Vec2{X: fx(1000), Y: fx(1000)}, fx(60))
	arm(t, w, a, 0, 0)
	w.Combats.AcquisitionRange[w.Combats.Row(a)] = fx(600)
	// enemy 300 away — inside acquisition range, so an acquiring stance WOULD see it.
	_ = atkUnit(t, w, 1, fixed.Vec2{X: fx(1300), Y: fx(1000)}, 0)

	ar := w.Transforms.Row(a)
	startX := w.Transforms.Pos[ar].X
	t.Logf("FSV before: attacker.X=%d (attack-move target X=3000, enemy at X=1300, acqRange=600)", startX/fixed.One)

	if !w.IssueOrder(a, Order{Kind: OrderAttack, Point: fixed.Vec2{X: fx(3000), Y: fx(1000)}, Target: 0}, false) {
		t.Fatal("issue attack-move")
	}
	for i := 0; i < 30; i++ {
		w.Step()
	}
	endX := w.Transforms.Pos[ar].X
	acquired := w.Combats.Target[w.Combats.Row(a)]
	curOrder := uint8(255)
	if or := w.Orders.Row(a); or != -1 {
		curOrder = w.Orders.Kind[or]
	}
	t.Logf("FSV after 30 ticks: attacker.X=%d acquiredTarget=%d currentOrderKind=%d (OrderAttack=%d OrderStop=%d)",
		endX/fixed.One, acquired, curOrder, OrderAttack, OrderStop)
	// Verified behavior: the Target==0 attack order completes immediately
	// (target 0 is dead), the unit falls back to its idle Stop stance, Stop
	// auto-acquires the nearby enemy, and the attack cycle chases it. So the
	// unit DOES move and DOES acquire — contrary to the discovery's claim (a).
	if acquired == 0 {
		t.Fatalf("expected the nearby enemy to be acquired via the idle-acquire stance")
	}
	if endX <= startX {
		t.Fatalf("expected the unit to advance (chasing the acquired enemy)")
	}
}

// TestAttackMoveTravelsToPoint — #384: with NO enemy in range, an attack-move
// (OrderAttack with a point, no target) now TRAVELS to the point and the order
// completes on arrival (it no longer reverts to Stop and sits). SoT = the
// attacker's Transform.X reaching the point's X.
func TestAttackMoveTravelsToPoint(t *testing.T) {
	w := NewWorld(Caps{})
	if err := w.BindDamageMatrix(atkMatrix); err != nil {
		t.Fatal(err)
	}
	w.SetAcquireInterval(1)
	a := atkUnit(t, w, 0, fixed.Vec2{X: fx(1000), Y: fx(1000)}, fx(60))
	arm(t, w, a, 0, 0)
	w.Combats.AcquisitionRange[w.Combats.Row(a)] = fx(600)
	// no enemy anywhere; gap to the point = 2000 units, speed 60 => ~34 ticks.

	ar := w.Transforms.Row(a)
	startX := w.Transforms.Pos[ar].X
	if !w.IssueOrder(a, Order{Kind: OrderAttack, Point: fixed.Vec2{X: fx(3000), Y: fx(1000)}, Target: 0}, false) {
		t.Fatal("issue attack-move")
	}
	// snapshot mid-travel to prove progress, then run to arrival.
	for i := 0; i < 10; i++ {
		w.Step()
	}
	midX := w.Transforms.Pos[ar].X
	for i := 0; i < 80; i++ {
		w.Step()
	}
	endX := w.Transforms.Pos[ar].X
	t.Logf("FSV attack-move travel: X start=%d mid(10t)=%d end(90t)=%d (point X=3000)",
		startX/fixed.One, midX/fixed.One, endX/fixed.One)
	if midX <= startX {
		t.Fatalf("attack-move did not begin travelling: X %d -> %d", startX/fixed.One, midX/fixed.One)
	}
	if endX != fx(3000) {
		t.Fatalf("attack-move did not arrive at the point: X=%d, want 3000", endX/fixed.One)
	}
}

// TestAttackMoveEngagesEnRouteThenArrives — the full primitive: an enemy sits on
// the path; the attack-mover acquires it en route, kills it, then resumes to the
// point. SoT = enemy Life (dead) AND attacker advancing past the enemy to the
// point.
func TestAttackMoveEngagesEnRouteThenArrives(t *testing.T) {
	w := NewWorld(Caps{})
	if err := w.BindDamageMatrix(atkMatrix); err != nil {
		t.Fatal(err)
	}
	w.SetAcquireInterval(1)
	a := atkUnit(t, w, 0, fixed.Vec2{X: fx(1000), Y: fx(1000)}, fx(60))
	arm(t, w, a, 0, 0) // 10 dmg, range 100
	w.Combats.AcquisitionRange[w.Combats.Row(a)] = fx(600)
	e := atkUnit(t, w, 1, fixed.Vec2{X: fx(1500), Y: fx(1000)}, 0)
	w.Healths.Life[w.Healths.Row(e)] = fx(15) // dies in 2 hits

	ar, er := w.Transforms.Row(a), w.Healths.Row(e)
	t.Logf("FSV before: attacker.X=%d enemy.Life=%d (enemy at X=1500, point X=3000)",
		w.Transforms.Pos[ar].X/fixed.One, w.Healths.Life[er]/fixed.One)
	if !w.IssueOrder(a, Order{Kind: OrderAttack, Point: fixed.Vec2{X: fx(3000), Y: fx(1000)}, Target: 0}, false) {
		t.Fatal("issue attack-move")
	}
	enemyDiedBy := -1
	for i := 0; i < 200; i++ {
		w.Step()
		if enemyDiedBy == -1 && !w.Ents.Alive(e) {
			enemyDiedBy = i
		}
	}
	endX := w.Transforms.Pos[ar].X
	t.Logf("FSV after 200t: enemyAlive=%v (diedAtTick=%d) attacker.X=%d", w.Ents.Alive(e), enemyDiedBy, endX/fixed.One)
	if w.Ents.Alive(e) {
		t.Fatalf("attack-mover did not kill the enemy en route")
	}
	if endX <= fx(1500) {
		t.Fatalf("attack-mover did not continue past the enemy toward the point: X=%d", endX/fixed.One)
	}
}

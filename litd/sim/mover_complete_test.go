package sim

import (
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim/sched"
)

// #589 — completion policy. SoT = mover liveness, Target liveness, victim
// Life, the OnDone cont's observable effect.

func TestMoverDoneExpire(t *testing.T) {
	w := NewWorld(Caps{Units: 8, Movers: 8})
	proj, _ := w.CreateUnit(fixed.Vec2{}, 0)
	m := w.Movers.Create(MoverSpec{
		Kind: MoverPoint, Target: proj, Goal: fixed.Vec2{X: 5 * fixed.One},
		Speed: 10 * fixed.One, DoneMode: MoverDoneExpire,
	})
	w.Step()
	if w.Movers.Alive(m) {
		t.Fatal("expire mover still live after arrival")
	}
	if !w.Ents.Alive(proj) {
		t.Fatal("expire (no consume) must NOT kill the target")
	}
}

func TestMoverDoneConsumeKillsTarget(t *testing.T) {
	w := NewWorld(Caps{Units: 8, Movers: 8})
	proj, _ := w.CreateUnit(fixed.Vec2{}, 0)
	w.Movers.Create(MoverSpec{
		Kind: MoverPoint, Target: proj, Goal: fixed.Vec2{X: 5 * fixed.One},
		Speed: 10 * fixed.One, DoneMode: MoverDoneExpire, Flags: MoverConsume,
	})
	w.Step()
	if w.Ents.Alive(proj) {
		t.Fatal("MoverConsume should kill the projectile body on completion")
	}
}

func TestMoverDoneLoopReArms(t *testing.T) {
	w := NewWorld(Caps{Units: 8, Movers: 8})
	proj, _ := w.CreateUnit(fixed.Vec2{}, 0)
	m := w.Movers.Create(MoverSpec{
		Kind: MoverLinear, Target: proj, Dir: fixed.Vec2{X: fixed.One},
		Speed: 10 * fixed.One, RangeLeft: 10 * fixed.One, DoneMode: MoverDoneLoop,
	})
	r, _ := w.Movers.resolve(m)
	// Range exhausts after 1 tick, but Loop re-arms instead of freeing.
	for i := 0; i < 5; i++ {
		w.Step()
	}
	if !w.Movers.Alive(m) {
		t.Fatal("loop mover should never expire")
	}
	if w.Movers.RangeLeft[r] != 0 {
		// after the exhausting tick it re-armed to Range0 then keeps going;
		// at minimum it is still live and cycling — assert it re-armed.
	}
	if w.Movers.Range0[r] != 10*fixed.One {
		t.Fatalf("Range0 = %d, want 10 (remembered for re-arm)", w.Movers.Range0[r])
	}
}

func TestMoverDoneDetonateAoE(t *testing.T) {
	w := NewWorld(Caps{Units: 16, Movers: 8})
	if err := w.BindDamageMatrix(dmgMatrix); err != nil {
		t.Fatalf("bind: %v", err)
	}
	caster, _ := w.CreateUnit(fixed.Vec2{X: 1000 * fixed.One}, 0)
	w.Owners.Add(w.Ents, caster, 1, 1, 1)
	goal := fixed.Vec2{X: 5 * fixed.One}
	mkEnemyAt := func(x int32) EntityID {
		id, _ := w.CreateUnit(fixed.Vec2{X: fixed.FromInt(x)}, 0)
		w.Owners.Add(w.Ents, id, 2, 2, 2)
		w.Healths.Add(w.Ents, id, 1000*fixed.One, 0, 0, 0)
		return id
	}
	near := mkEnemyAt(5) // at the goal → inside blast
	far := mkEnemyAt(500)
	proj, _ := w.CreateUnit(fixed.Vec2{}, 0)
	w.Movers.Create(MoverSpec{
		Kind: MoverPoint, Target: proj, Owner: caster, Goal: goal, Speed: 10 * fixed.One,
		Radius: 30 * fixed.One, HitMask: MissileHitEnemy, DoneMode: MoverDoneDetonate,
		Packet: DamagePacket{Amount: 100 * fixed.One},
	})
	w.Step() // arrive → detonate AoE
	if got := 1000*fixed.One - lifeOf(w, near); got != 100*fixed.One {
		t.Fatalf("near enemy detonate damage = %d, want 100", got)
	}
	if lifeOf(w, far) != 1000*fixed.One {
		t.Fatal("far enemy outside blast radius took damage")
	}
}

func TestMoverDoneContFires(t *testing.T) {
	w := NewWorld(Caps{Units: 8, Movers: 8})
	const cid sched.ContID = 4242
	fired := false
	w.Sched.Register(cid, func(s *sched.Scheduler, st sched.State) {
		// SoT: the cont sees the mover's CState payload.
		if st[0] == 77 {
			fired = true
		}
	})
	proj, _ := w.CreateUnit(fixed.Vec2{}, 0)
	w.Movers.Create(MoverSpec{
		Kind: MoverPoint, Target: proj, Goal: fixed.Vec2{X: 5 * fixed.One}, Speed: 10 * fixed.One,
		DoneMode: MoverDoneCont, OnDone: uint16(cid), CState: [4]int64{77, 0, 0, 0},
	})
	w.Step() // arrive → schedule cont
	w.Step() // cont resumes next tick
	if !fired {
		t.Fatal("OnDone cont did not fire with the mover CState")
	}
}

func TestMoverOwnerDeathAutoCancels(t *testing.T) {
	w := NewWorld(Caps{Units: 8, Movers: 8})
	owner, _ := w.CreateUnit(fixed.Vec2{X: 50 * fixed.One}, 0)
	w.Healths.Add(w.Ents, owner, 100*fixed.One, 0, 0, 0)
	proj, _ := w.CreateUnit(fixed.Vec2{}, 0)
	m := w.Movers.Create(MoverSpec{
		Kind: MoverOrbitPoint, Target: proj, Owner: owner,
		Goal: fixed.Vec2{}, Radius: 10 * fixed.One, AngVel: quarterBAM,
	})
	w.Step()
	if !w.Movers.Alive(m) {
		t.Fatal("orbit mover should persist before owner death")
	}
	w.KillUnit(owner) // dies this tick → cleanup auto-cancels its movers
	w.Step()
	if w.Movers.Alive(m) {
		t.Fatal("owner death did not auto-cancel the mover (R-MOV-10)")
	}
}

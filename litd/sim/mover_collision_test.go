package sim

import (
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
)

// #587 — mover collision/pierce/decay/already-hit. SoT = victim Life, the
// mover's Pierce/HitN columns, read directly after stepping.

func lifeOf(w *World, id EntityID) fixed.F64 { return w.Healths.Life[w.Healths.Row(id)] }

func TestMoverCollisionPierceAndDecay(t *testing.T) {
	w := NewWorld(Caps{Units: 16, Movers: 8})
	if err := w.BindDamageMatrix(dmgMatrix); err != nil { // AttackType 0/armor 0 -> 1.0x
		t.Fatalf("bind matrix: %v", err)
	}
	caster, _ := w.CreateUnit(fixed.Vec2{X: 1000 * fixed.One}, 0)
	w.Owners.Add(w.Ents, caster, 1, 1, 1)
	mkVictim := func() EntityID {
		id, _ := w.CreateUnit(fixed.Vec2{}, 0) // at origin
		w.Owners.Add(w.Ents, id, 2, 2, 2)      // enemy team
		w.Healths.Add(w.Ents, id, 1000*fixed.One, 0, 0, 0)
		return id
	}
	v1 := mkVictim()
	v2 := mkVictim()
	proj, _ := w.CreateUnit(fixed.Vec2{}, 0) // projectile body at origin
	m := w.Movers.Create(MoverSpec{
		Kind: MoverLinear, Target: proj, Owner: caster,
		Dir: fixed.Vec2{X: fixed.One}, Speed: 0, RangeLeft: 100 * fixed.One, // stationary collider
		Radius: 5 * fixed.One, HitMask: MissileHitEnemy, Pierce: 5,
		Packet: DamagePacket{Amount: 100 * fixed.One}, Decay: 500, // 50% decay/hit
	})
	r, _ := w.Movers.resolve(m)

	w.Step() // both victims in radius → hit; damage applies in phase 5
	// SoT: v1 (lower id, hit first) took 100; v2 took 50 (post-decay).
	if got := 1000*fixed.One - lifeOf(w, v1); got != 100*fixed.One {
		t.Fatalf("v1 damage = %d, want 100", got)
	}
	if got := 1000*fixed.One - lifeOf(w, v2); got != 50*fixed.One {
		t.Fatalf("v2 damage = %d, want 50 (after 50%% decay)", got)
	}
	if w.Movers.Pierce[r] != 3 || w.Movers.HitN[r] != 2 {
		t.Fatalf("Pierce=%d HitN=%d, want 3,2", w.Movers.Pierce[r], w.Movers.HitN[r])
	}

	// Second tick: both already hit → no new damage, no pierce spent.
	w.Step()
	if got := 1000*fixed.One - lifeOf(w, v1); got != 100*fixed.One {
		t.Fatalf("v1 re-hit: damage now %d, want still 100 (already-hit ring)", got)
	}
	if w.Movers.Pierce[r] != 3 || w.Movers.HitN[r] != 2 {
		t.Fatalf("after re-tick Pierce=%d HitN=%d, want unchanged 3,2", w.Movers.Pierce[r], w.Movers.HitN[r])
	}
}

func TestMoverCollisionPierceExhaustionCompletes(t *testing.T) {
	w := NewWorld(Caps{Units: 16, Movers: 8})
	caster, _ := w.CreateUnit(fixed.Vec2{X: 1000 * fixed.One}, 0)
	w.Owners.Add(w.Ents, caster, 1, 1, 1)
	v1, _ := w.CreateUnit(fixed.Vec2{}, 0)
	w.Owners.Add(w.Ents, v1, 2, 2, 2)
	w.Healths.Add(w.Ents, v1, 1000*fixed.One, 0, 0, 0)
	proj, _ := w.CreateUnit(fixed.Vec2{}, 0)
	m := w.Movers.Create(MoverSpec{
		Kind: MoverLinear, Target: proj, Owner: caster, Speed: 0,
		Dir: fixed.Vec2{X: fixed.One}, RangeLeft: 100 * fixed.One,
		Radius: 5 * fixed.One, HitMask: MissileHitEnemy, Pierce: 1, // single-hit
		Packet: DamagePacket{Amount: 10 * fixed.One},
	})
	w.Step()
	if w.Movers.Alive(m) {
		t.Fatal("pierce-1 mover should be consumed after its hit")
	}
}

func TestMoverCollisionAllyFiltered(t *testing.T) {
	w := NewWorld(Caps{Units: 16, Movers: 8})
	caster, _ := w.CreateUnit(fixed.Vec2{X: 1000 * fixed.One}, 0)
	w.Owners.Add(w.Ents, caster, 1, 1, 1)
	ally, _ := w.CreateUnit(fixed.Vec2{}, 0)
	w.Owners.Add(w.Ents, ally, 1, 1, 1) // same team as caster
	w.Healths.Add(w.Ents, ally, 1000*fixed.One, 0, 0, 0)
	proj, _ := w.CreateUnit(fixed.Vec2{}, 0)
	w.Movers.Create(MoverSpec{
		Kind: MoverLinear, Target: proj, Owner: caster, Speed: 0,
		Dir: fixed.Vec2{X: fixed.One}, RangeLeft: 100 * fixed.One,
		Radius: 5 * fixed.One, HitMask: MissileHitEnemy, Pierce: 5,
		Packet: DamagePacket{Amount: 100 * fixed.One},
	})
	w.Step()
	if lifeOf(w, ally) != 1000*fixed.One {
		t.Fatalf("ally took damage from enemy-only mask: life=%d", lifeOf(w, ally))
	}
}

func TestMoverCollisionZeroAlloc(t *testing.T) {
	w := NewWorld(Caps{Units: 16, Movers: 8})
	proj, _ := w.CreateUnit(fixed.Vec2{X: 5000 * fixed.One}, 0) // empty space
	w.Movers.Create(MoverSpec{
		Kind: MoverLinear, Target: proj, Speed: 0, Dir: fixed.Vec2{X: fixed.One},
		RangeLeft: 1 << 40, Radius: 5 * fixed.One, HitMask: MissileHitEnemy, Pierce: 9,
	})
	r := int32(1)
	avg := testing.AllocsPerRun(500, func() { w.moverCollide(r) })
	if avg != 0 {
		t.Fatalf("moverCollide allocated %.2f objs/op, want 0 (reused scratch)", avg)
	}
}

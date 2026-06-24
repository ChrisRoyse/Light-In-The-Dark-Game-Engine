package sim

// #590 launch parity FSV. The mover-backed spawnMoverProjectile must deliver
// the SAME damage as the legacy SpawnMissile for every guidance mode. SoT =
// the victim's Health floor read directly after each path runs to completion
// in an otherwise-identical world (synthetic X - dmg = Y). The two paths are
// NOT compared by return value — only by the state they leave behind.

import (
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
)

// runToQuiet steps until no movers and no missiles remain live, or a cap.
func runToQuiet(w *World, cap int) {
	for i := 0; i < cap; i++ {
		w.Step()
		if w.Movers.Count() == 0 && w.Missiles.Count() == 0 {
			return
		}
	}
}

func TestSpawnProjectilePointParity(t *testing.T) {
	const dmg = 30
	build := func() (*World, EntityID, EntityID) {
		w := lmWorld(t)
		shooter := atkUnit(t, w, 0, xy(1000, 1000), 0)
		victim := atkUnit(t, w, 1, xy(1300, 1000), 0)
		return w, shooter, victim
	}
	spec := func(shooter, victim EntityID, vpos fixed.Vec2) MissileSpec {
		return MissileSpec{
			Pos: xy(1000, 1000), Source: shooter, Point: vpos,
			Speed:  100 * fixed.One,
			Packet: DamagePacket{Source: shooter, Target: victim, Amount: dmg * fixed.One},
		}
	}

	wa, sa, va := build()
	if _, ok := wa.SpawnMissile(spec(sa, va, xy(1300, 1000))); !ok {
		t.Fatal("legacy point spawn failed")
	}
	runToQuiet(wa, 20)

	wb, sb, vb := build()
	if _, ok := wb.spawnMoverProjectile(spec(sb, vb, xy(1300, 1000))); !ok {
		t.Fatal("mover point spawn failed")
	}
	runToQuiet(wb, 20)

	t.Logf("POINT victim: legacy=%d mover=%d (want %d)", life(wa, va), life(wb, vb), 100-dmg)
	if life(wa, va) != 100-dmg {
		t.Fatalf("legacy point dmg wrong: victim=%d want %d", life(wa, va), 100-dmg)
	}
	if life(wb, vb) != life(wa, va) {
		t.Fatalf("PARITY BREAK point: mover victim=%d != legacy %d", life(wb, vb), life(wa, va))
	}
}

func TestSpawnProjectileHomingParity(t *testing.T) {
	const dmg = 25
	build := func() (*World, EntityID, EntityID) {
		w := lmWorld(t)
		shooter := atkUnit(t, w, 0, xy(1000, 1000), 0)
		victim := atkUnit(t, w, 1, xy(1400, 1200), 0) // stationary guide for determinism
		return w, shooter, victim
	}
	spec := func(shooter, victim EntityID) MissileSpec {
		return MissileSpec{
			Pos: xy(1000, 1000), Source: shooter, Target: victim,
			Speed:  90 * fixed.One,
			Packet: DamagePacket{Source: shooter, Amount: dmg * fixed.One},
		}
	}

	wa, sa, va := build()
	if _, ok := wa.SpawnMissile(spec(sa, va)); !ok {
		t.Fatal("legacy homing spawn failed")
	}
	runToQuiet(wa, 30)

	wb, sb, vb := build()
	if _, ok := wb.spawnMoverProjectile(spec(sb, vb)); !ok {
		t.Fatal("mover homing spawn failed")
	}
	runToQuiet(wb, 30)

	t.Logf("HOMING victim: legacy=%d mover=%d (want %d)", life(wa, va), life(wb, vb), 100-dmg)
	if life(wa, va) != 100-dmg {
		t.Fatalf("legacy homing dmg wrong: victim=%d want %d", life(wa, va), 100-dmg)
	}
	if life(wb, vb) != life(wa, va) {
		t.Fatalf("PARITY BREAK homing: mover victim=%d != legacy %d", life(wb, vb), life(wa, va))
	}
}

func TestSpawnProjectileLinearParity(t *testing.T) {
	const dmg = 40
	build := func() (*World, EntityID, EntityID, EntityID) {
		w := lmWorld(t)
		shooter := atkUnit(t, w, 0, xy(1000, 1000), 0)
		foe := atkUnit(t, w, 1, xy(1300, 1000), 0)      // in lane
		offlane := atkUnit(t, w, 1, xy(1300, 1400), 0)  // off lane — must NOT be hit
		return w, shooter, foe, offlane
	}
	spec := func(shooter EntityID) MissileSpec {
		return MissileSpec{
			Pos: xy(1000, 1000), Source: shooter, Speed: 100 * fixed.One,
			Flags: MissileLinear, Dir: xy(1, 0), Range: 1000 * fixed.One, Pierce: 1,
			Packet: DamagePacket{Source: shooter, Amount: dmg * fixed.One},
		}
	}

	wa, sa, fa, oa := build()
	if _, ok := wa.SpawnMissile(spec(sa)); !ok {
		t.Fatal("legacy linear spawn failed")
	}
	runToQuiet(wa, 20)

	wb, sb, fb, ob := build()
	if _, ok := wb.spawnMoverProjectile(spec(sb)); !ok {
		t.Fatal("mover linear spawn failed")
	}
	runToQuiet(wb, 20)

	t.Logf("LINEAR foe: legacy=%d mover=%d | offlane: legacy=%d mover=%d (want foe=%d, offlane=100)",
		life(wa, fa), life(wb, fb), life(wa, oa), life(wb, ob), 100-dmg)
	if life(wa, fa) != 100-dmg || life(wa, oa) != 100 {
		t.Fatalf("legacy linear wrong: foe=%d offlane=%d", life(wa, fa), life(wa, oa))
	}
	if life(wb, fb) != life(wa, fa) {
		t.Fatalf("PARITY BREAK linear foe: mover=%d != legacy %d", life(wb, fb), life(wa, fa))
	}
	if life(wb, ob) != life(wa, oa) {
		t.Fatalf("PARITY BREAK linear offlane: mover=%d != legacy %d", life(wb, ob), life(wa, oa))
	}
}

// Degenerate specs the legacy path rejects, the mover path must reject too —
// deterministic (0,false), never a silent fallback.
func TestSpawnProjectileRejectionParity(t *testing.T) {
	w := lmWorld(t)
	s := atkUnit(t, w, 0, xy(0, 0), 0)
	bad := []MissileSpec{
		{Pos: xy(0, 0), Source: s, Speed: 0, Target: s},                                                   // zero speed
		{Pos: xy(0, 0), Source: s, Speed: 10 * fixed.One, Accel: -1},                                      // negative accel
		{Pos: xy(0, 0), Source: s, Speed: 10 * fixed.One, Flags: MissileLinear, Dir: xy(0, 0), Range: 100, Pierce: 1}, // zero dir skillshot
		{Pos: xy(0, 0), Source: s, Speed: 10 * fixed.One, Flags: MissileLinear, Dir: xy(1, 0), Range: 0, Pierce: 1},   // zero range skillshot
	}
	for i, m := range bad {
		_, legacyOK := w.SpawnMissile(m)
		_, moverOK := w.spawnMoverProjectile(m)
		t.Logf("reject[%d]: legacy ok=%v mover ok=%v", i, legacyOK, moverOK)
		if legacyOK || moverOK {
			t.Fatalf("spec %d: both paths must reject (legacy=%v mover=%v)", i, legacyOK, moverOK)
		}
	}
}

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

// Multi-hit pierce+decay parity: a skillshot through three foes must decay the
// payload identically via both paths. Guards the Decay-convention bridge in
// spawnMoverProjectile (MissileSpec keep-per-mille -> MoverSpec reduction). The
// asymmetric Decay=700 (input keep 70%) would mismap under the naive
// pass-through and is the regression that exposed the bug.
func TestSpawnProjectileDecayParity(t *testing.T) {
	build := func() (*World, EntityID, []EntityID) {
		w := lmWorld(t)
		s := atkUnit(t, w, 0, xy(1000, 1000), 0)
		f1 := atkUnit(t, w, 1, xy(1200, 1000), 0)
		f2 := atkUnit(t, w, 1, xy(1400, 1000), 0)
		f3 := atkUnit(t, w, 1, xy(1600, 1000), 0)
		return w, s, []EntityID{f1, f2, f3}
	}
	spec := func(s EntityID) MissileSpec {
		return MissileSpec{
			Pos: xy(1000, 1000), Source: s, Speed: 800 * fixed.One,
			Flags: MissileLinear, Dir: xy(1, 0), Range: 2000 * fixed.One, Pierce: 3, Decay: 700,
			Packet: DamagePacket{Source: s, Amount: 50 * fixed.One},
		}
	}
	wa, sa, fa := build()
	if _, ok := wa.SpawnMissile(spec(sa)); !ok {
		t.Fatal("legacy decay spawn")
	}
	runToQuiet(wa, 20)

	wb, sb, fb := build()
	if _, ok := wb.spawnMoverProjectile(spec(sb)); !ok {
		t.Fatal("mover decay spawn")
	}
	runToQuiet(wb, 20)

	for i := range fa {
		la, lb := life(wa, fa[i]), life(wb, fb[i])
		t.Logf("decay foe%d: legacy=%d mover=%d", i+1, la, lb)
		if la == 100 {
			t.Fatalf("legacy foe%d untouched — test setup wrong", i+1)
		}
		if lb != la {
			t.Fatalf("DECAY PARITY BREAK foe%d: mover=%d != legacy %d", i+1, lb, la)
		}
	}
}

// Per-hit presentation cue: a piercing projectile fires OnMissileImpact once
// per pierced foe (matching the legacy linear missile), gated on ProjRender.
func TestSpawnProjectilePerHitCue(t *testing.T) {
	w := lmWorld(t)
	s := atkUnit(t, w, 0, xy(1000, 1000), 0)
	atkUnit(t, w, 1, xy(1200, 1000), 0)
	atkUnit(t, w, 1, xy(1400, 1000), 0)
	cues := 0
	w.OnMissileImpact = func(uint32, EntityID, fixed.Vec2, EntityID) { cues++ }
	if _, ok := w.spawnMoverProjectile(MissileSpec{
		Pos: xy(1000, 1000), Source: s, Speed: 800 * fixed.One,
		Flags: MissileLinear, Dir: xy(1, 0), Range: 2000 * fixed.One, Pierce: 2,
		Packet: DamagePacket{Source: s, Amount: 10 * fixed.One},
	}); !ok {
		t.Fatal("spawn")
	}
	runToQuiet(w, 20)
	t.Logf("per-hit cues fired = %d (want 2)", cues)
	if cues != 2 {
		t.Fatalf("OnMissileImpact fired %d times, want 2 (one per pierced foe)", cues)
	}
}

// Acceleration parity: a point projectile with Accel must integrate speed the
// same as the legacy missile (Speed += Accel after each move). SoT = the body's
// X after two ticks. Speed 10 + Accel 5: tick1 moves 10 (x=10, speed->15),
// tick2 moves 15 (x=25, speed->20) — identical via both paths.
func TestSpawnProjectileAccelParity(t *testing.T) {
	spec := func(s EntityID) MissileSpec {
		return MissileSpec{
			Pos: xy(0, 0), Point: xy(1000, 0), Source: s,
			Speed: 10 * fixed.One, Accel: 5 * fixed.One, GuidanceID: MissileGuidancePoint,
		}
	}
	posX := func(spawn func(*World, EntityID) (EntityID, bool)) int64 {
		w := lmWorld(t)
		s := atkUnit(t, w, 0, xy(0, 200), 0)
		id, ok := spawn(w, s)
		if !ok {
			t.Fatal("spawn")
		}
		w.Step()
		w.Step()
		return w.Transforms.Pos[w.Transforms.Row(id)].X.Floor()
	}
	legacy := posX(func(w *World, s EntityID) (EntityID, bool) { return w.SpawnMissile(spec(s)) })
	mover := posX(func(w *World, s EntityID) (EntityID, bool) { return w.spawnMoverProjectile(spec(s)) })
	t.Logf("accel pos.X after 2 ticks: legacy=%d mover=%d (want 25)", legacy, mover)
	if legacy != 25 {
		t.Fatalf("legacy accel baseline = %d, want 25", legacy)
	}
	if mover != legacy {
		t.Fatalf("ACCEL PARITY BREAK: mover x=%d != legacy %d (mover not accelerating)", mover, legacy)
	}
}

// Expire-signal parity: a linear skillshot that spends its range in an empty
// lane fires OnMissileExpire (payload-less) via both paths — the missile
// expiry cue must survive the mover migration. SoT = the expire-callback count.
func TestSpawnProjectileExpireSignalParity(t *testing.T) {
	spec := func(s EntityID) MissileSpec {
		return MissileSpec{
			Pos: xy(1000, 1000), Source: s, Speed: 100 * fixed.One,
			Flags: MissileLinear, Dir: xy(1, 0), Range: 250 * fixed.One, Pierce: 1,
			Packet: DamagePacket{Source: s, Amount: 30 * fixed.One},
		}
	}
	count := func(spawn func(*World, EntityID) bool) (expires, impacts int) {
		w := lmWorld(t)
		s := atkUnit(t, w, 0, xy(1000, 1000), 0) // empty lane: no foes
		w.OnMissileExpire = func(uint32, EntityID, fixed.Vec2) { expires++ }
		w.OnMissileImpact = func(uint32, EntityID, fixed.Vec2, EntityID) { impacts++ }
		if !spawn(w, s) {
			t.Fatal("spawn")
		}
		runToQuiet(w, 12)
		return
	}
	le, li := count(func(w *World, s EntityID) bool { _, ok := w.SpawnMissile(spec(s)); return ok })
	me, mi := count(func(w *World, s EntityID) bool { _, ok := w.spawnMoverProjectile(spec(s)); return ok })
	t.Logf("expire: legacy=%d mover=%d | impact: legacy=%d mover=%d", le, me, li, mi)
	if le != 1 || li != 0 {
		t.Fatalf("legacy expiry baseline wrong: expires=%d impacts=%d (want 1/0)", le, li)
	}
	if me != le || mi != li {
		t.Fatalf("EXPIRE PARITY BREAK: mover expires=%d impacts=%d, legacy %d/%d", me, mi, le, li)
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

package sim

// #590 golden-safety linchpin. Flipping SpawnMissile onto movers churns every
// determinism golden that involves ranged combat (attack.go auto-fires
// projectiles). That churn is only LEGITIMATE if combat OUTCOMES are invariant
// — same damage AND same impact tick. Final-health parity is proven elsewhere;
// this proves the projectile arrives on the EXACT SAME TICK via both paths, so
// a rebumped golden differs only in projectile representation, never in when a
// unit takes the hit. SoT = the tick at which the victim's Health first drops.

import (
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
)

// firstHitTick steps up to cap and returns the tick (1-based step count) on
// which the victim's life first falls below 100, or -1 if it never does.
func firstHitTick(w *World, victim EntityID, cap int) int {
	for i := 1; i <= cap; i++ {
		w.Step()
		if life(w, victim) < 100 {
			return i
		}
	}
	return -1
}

func TestProjectileArrivalTickParity(t *testing.T) {
	type mode struct {
		name string
		spec func(shooter, victim EntityID) MissileSpec
	}
	modes := []mode{
		{"point", func(s, v EntityID) MissileSpec {
			return MissileSpec{
				Pos: xy(1000, 1000), Source: s, Point: xy(1450, 1000), Speed: 70 * fixed.One,
				Packet: DamagePacket{Source: s, Target: v, Amount: 20 * fixed.One},
			}
		}},
		{"homing", func(s, v EntityID) MissileSpec {
			return MissileSpec{
				Pos: xy(1000, 1000), Source: s, Target: v, Speed: 60 * fixed.One,
				Packet: DamagePacket{Source: s, Amount: 20 * fixed.One},
			}
		}},
		{"linear", func(s, v EntityID) MissileSpec {
			return MissileSpec{
				Pos: xy(1000, 1000), Source: s, Speed: 55 * fixed.One,
				Flags: MissileLinear, Dir: xy(1, 0), Range: 2000 * fixed.One, Pierce: 1,
				Packet: DamagePacket{Source: s, Amount: 20 * fixed.One},
			}
		}},
	}
	for _, m := range modes {
		build := func() (*World, EntityID, EntityID) {
			w := lmWorld(t)
			s := atkUnit(t, w, 0, xy(1000, 1000), 0)
			v := atkUnit(t, w, 1, xy(1450, 1000), 0)
			return w, s, v
		}
		wa, sa, va := build()
		if _, ok := wa.SpawnMissile(m.spec(sa, va)); !ok {
			t.Fatalf("%s: legacy spawn failed", m.name)
		}
		legacyTick := firstHitTick(wa, va, 60)

		wb, sb, vb := build()
		if _, ok := wb.spawnMoverProjectile(m.spec(sb, vb)); !ok {
			t.Fatalf("%s: mover spawn failed", m.name)
		}
		moverTick := firstHitTick(wb, vb, 60)

		t.Logf("%s: legacy impact tick=%d, mover impact tick=%d", m.name, legacyTick, moverTick)
		if legacyTick == -1 {
			t.Fatalf("%s: legacy missile never hit (test bug)", m.name)
		}
		if moverTick != legacyTick {
			t.Fatalf("%s: ARRIVAL-TICK DIVERGENCE — legacy hits t%d, mover hits t%d; "+
				"the flip would shift combat timing, not just representation", m.name, legacyTick, moverTick)
		}
	}
}

package sim

// #590 — the missile-control facade (Detonate/Expire/SetTarget) must drive a
// mover-backed projectile too, not just a legacy MissileStore row. SoT = the
// victim Health + the captured presentation callbacks/events, read after the
// facade call drives the underlying mover to completion.

import (
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
)

func TestDetonateMissileMoverProjectile(t *testing.T) {
	w := lmWorld(t)
	shooter := atkUnit(t, w, 0, xy(1000, 1000), 0)
	victim := atkUnit(t, w, 1, xy(2000, 1000), 0) // far — natural arrival is many ticks off
	var cueAt []string
	w.OnMissileImpact = func(tick uint32, id EntityID, at fixed.Vec2, tgt EntityID) {
		cueAt = append(cueAt, "fired")
	}
	id, ok := w.spawnMoverProjectile(MissileSpec{
		Pos: xy(1000, 1000), Source: shooter, Point: xy(2000, 1000), Speed: 50 * fixed.One,
		Packet: DamagePacket{Source: shooter, Target: victim, Amount: 30 * fixed.One},
	})
	if !ok {
		t.Fatal("spawn point projectile")
	}
	w.Step() // a few units of flight, nowhere near the goal yet
	if life(w, victim) != 100 {
		t.Fatalf("victim damaged before detonate: %d", life(w, victim))
	}
	if !w.DetonateMissile(id) {
		t.Fatal("DetonateMissile returned false for a live projectile")
	}
	w.Step() // flush the queued damage + reap the body
	t.Logf("after detonate: victimHP=%d cues=%d bodyAlive=%v", life(w, victim), len(cueAt), w.Ents.Alive(id))
	if life(w, victim) != 70 {
		t.Fatalf("detonate did not deliver: victim=%d want 70", life(w, victim))
	}
	if len(cueAt) != 1 {
		t.Fatalf("OnMissileImpact fired %d times, want 1", len(cueAt))
	}
	if w.Ents.Alive(id) {
		t.Fatal("detonated projectile body must be consumed")
	}
}

func TestExpireMissileMoverProjectile(t *testing.T) {
	w := lmWorld(t)
	shooter := atkUnit(t, w, 0, xy(1000, 1000), 0)
	victim := atkUnit(t, w, 1, xy(2000, 1000), 0)
	var expired int
	w.OnMissileExpire = func(tick uint32, id EntityID, at fixed.Vec2) { expired++ }
	id, ok := w.spawnMoverProjectile(MissileSpec{
		Pos: xy(1000, 1000), Source: shooter, Point: xy(2000, 1000), Speed: 50 * fixed.One,
		Packet: DamagePacket{Source: shooter, Target: victim, Amount: 30 * fixed.One},
	})
	if !ok {
		t.Fatal("spawn point projectile")
	}
	w.Step()
	if !w.ExpireMissile(id) {
		t.Fatal("ExpireMissile returned false for a live projectile")
	}
	w.Step()
	t.Logf("after expire: victimHP=%d expiredCbs=%d bodyAlive=%v", life(w, victim), expired, w.Ents.Alive(id))
	if life(w, victim) != 100 {
		t.Fatalf("expire delivered damage (should be payload-less): victim=%d", life(w, victim))
	}
	if expired != 1 {
		t.Fatalf("OnMissileExpire fired %d times, want 1", expired)
	}
	if w.Ents.Alive(id) {
		t.Fatal("expired projectile body must be consumed")
	}
}

func TestSetMissileTargetMoverProjectileRetargetsToHoming(t *testing.T) {
	w := lmWorld(t)
	shooter := atkUnit(t, w, 0, xy(1000, 1000), 0)
	offlane := atkUnit(t, w, 1, xy(1300, 1600), 0) // way off the +X lane — a skillshot would miss
	// Launch a linear skillshot straight down +X (would never touch the off-lane foe).
	id, ok := w.spawnMoverProjectile(MissileSpec{
		Pos: xy(1000, 1000), Source: shooter, Speed: 80 * fixed.One,
		Flags: MissileLinear, Dir: xy(1, 0), Range: 2000 * fixed.One, Pierce: 1,
		Packet: DamagePacket{Source: shooter, Amount: 35 * fixed.One},
	})
	if !ok {
		t.Fatal("spawn linear skillshot")
	}
	w.Step()
	// Retarget it onto the off-lane foe: now it must home and deliver.
	if !w.SetMissileTarget(id, offlane) {
		t.Fatal("SetMissileTarget returned false")
	}
	for i := 0; i < 40 && w.Ents.Alive(id); i++ {
		w.Step()
	}
	t.Logf("after retarget+home: offlaneHP=%d bodyAlive=%v", life(w, offlane), w.Ents.Alive(id))
	if life(w, offlane) != 65 {
		t.Fatalf("retargeted projectile did not home+deliver: offlane=%d want 65", life(w, offlane))
	}
}

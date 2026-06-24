package sim

import (
	"bytes"
	"fmt"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/statehash"
)

func TestMissileAccelerationHashSaveFSV(t *testing.T) {
	w := lmWorld(t)
	shooter := atkUnit(t, w, 0, xy(0, 200), 0)
	id, ok := w.SpawnMissile(MissileSpec{
		Pos:        xy(0, 0),
		Point:      xy(1000, 0),
		Source:     shooter,
		Speed:      10 * fixed.One,
		Accel:      5 * fixed.One,
		GuidanceID: MissileGuidancePoint,
		ImpactID:   MissileImpactDeliver,
	})
	if !ok {
		t.Fatal("spawn accelerating missile")
	}
	before := missileStateDump(w, id)
	w.Step()
	after1 := missileStateDump(w, id)
	w.Step()
	after2 := missileStateDump(w, id)
	t.Logf("FSV missile acceleration BEFORE: %s", before)
	t.Logf("FSV missile acceleration AFTER t1: %s", after1)
	t.Logf("FSV missile acceleration AFTER t2: %s", after2)

	mr, okm := w.projMover(id)
	if !okm {
		t.Fatal("projectile has no live mover")
	}
	tr := w.Transforms.Row(id)
	if w.Transforms.Pos[tr].X != 25*fixed.One || w.Movers.Speed[mr] != 20*fixed.One || w.Movers.Accel[mr] != 5*fixed.One {
		t.Fatalf("acceleration integration wrong: %s", after2)
	}

	var base, changed statehash.Snapshot
	w.HashState(NewHashRegistry(), &base)
	w.Movers.Accel[mr] = 6 * fixed.One
	w.HashState(NewHashRegistry(), &changed)
	w.Movers.Accel[mr] = 5 * fixed.One
	t.Logf("FSV missile acceleration hash: base=%016x changed=%016x", base.Top, changed.Top)
	if base.Top == changed.Top {
		t.Fatal("Accel mutation invisible to state hash")
	}

	var buf bytes.Buffer
	if err := w.SaveState(&buf, 5); err != nil {
		t.Fatal(err)
	}
	loaded := lmWorld(t)
	if err := loaded.LoadState(bytes.NewReader(buf.Bytes()), 5); err != nil {
		t.Fatal(err)
	}
	var loadedHash statehash.Snapshot
	loaded.HashState(NewHashRegistry(), &loadedHash)
	t.Logf("FSV missile acceleration save: bytes=%d beforeHash=%016x loadedHash=%016x loaded=%s",
		buf.Len(), base.Top, loadedHash.Top, missileStateDump(loaded, id))
	if loadedHash.Top != base.Top {
		t.Fatal("save/load did not preserve acceleration missile state")
	}

	beforeCount := w.Movers.Count()
	bad, badOK := w.SpawnMissile(MissileSpec{
		Pos: xy(0, 0), Point: xy(100, 0), Source: shooter,
		Speed: 10 * fixed.One, Accel: -fixed.One,
	})
	afterCount := w.Movers.Count()
	t.Logf("FSV missile negative acceleration BEFORE count=%d AFTER id=%d ok=%v count=%d",
		beforeCount, bad.Index(), badOK, afterCount)
	if badOK || bad != 0 || afterCount != beforeCount {
		t.Fatal("negative acceleration must fail closed without mutating mover rows")
	}
}

func TestMissileHitMaskFiltersRelationAndClassFSV(t *testing.T) {
	allyWorld, allyShooter, ally, groundEnemy, airEnemy := missileMaskWorld(t)
	id, ok := allyWorld.SpawnMissile(MissileSpec{
		Pos: xy(1000, 1000), Source: allyShooter, Speed: 500 * fixed.One,
		Flags: MissileLinear, GuidanceID: MissileGuidanceLinear, ImpactID: MissileImpactPierce,
		Dir: xy(1, 0), Range: 1000 * fixed.One, Pierce: 3,
		HitMask: MissileHitAlly | MissileHitGround,
		Packet:  DamagePacket{Source: allyShooter, Amount: 30 * fixed.One},
	})
	if !ok {
		t.Fatal("spawn ally-mask missile")
	}
	beforeAlly := missileMaskDump(allyWorld, id, ally, groundEnemy, airEnemy)
	allyWorld.Step()
	afterAlly := missileMaskDump(allyWorld, id, ally, groundEnemy, airEnemy)
	t.Logf("FSV missile hit mask ally BEFORE: %s", beforeAlly)
	t.Logf("FSV missile hit mask ally AFTER:  %s", afterAlly)
	if life(allyWorld, ally) != 70 || life(allyWorld, groundEnemy) != 100 || life(allyWorld, airEnemy) != 100 {
		t.Fatalf("ally ground mask hit wrong targets: %s", afterAlly)
	}

	airWorld, airShooter, airAlly, airGroundEnemy, airOnlyEnemy := missileMaskWorld(t)
	id, ok = airWorld.SpawnMissile(MissileSpec{
		Pos: xy(1000, 1000), Source: airShooter, Speed: 500 * fixed.One,
		Flags: MissileLinear, GuidanceID: MissileGuidanceLinear, ImpactID: MissileImpactPierce,
		Dir: xy(1, 0), Range: 1000 * fixed.One, Pierce: 3,
		HitMask: MissileHitEnemy | MissileHitAir,
		Packet:  DamagePacket{Source: airShooter, Amount: 30 * fixed.One},
	})
	if !ok {
		t.Fatal("spawn air-mask missile")
	}
	beforeAir := missileMaskDump(airWorld, id, airAlly, airGroundEnemy, airOnlyEnemy)
	airWorld.Step()
	afterAir := missileMaskDump(airWorld, id, airAlly, airGroundEnemy, airOnlyEnemy)
	t.Logf("FSV missile hit mask air BEFORE: %s", beforeAir)
	t.Logf("FSV missile hit mask air AFTER:  %s", afterAir)
	if life(airWorld, airAlly) != 100 || life(airWorld, airGroundEnemy) != 100 || life(airWorld, airOnlyEnemy) != 70 {
		t.Fatalf("enemy air mask hit wrong targets: %s", afterAir)
	}
}

func missileMaskWorld(t *testing.T) (*World, EntityID, EntityID, EntityID, EntityID) {
	t.Helper()
	w := lmWorld(t)
	shooter := atkUnit(t, w, 0, xy(1000, 900), 0)
	ally := atkUnit(t, w, 0, xy(1150, 1000), 0)
	groundEnemy := atkUnit(t, w, 1, xy(1300, 1000), 0)
	airEnemy := atkUnit(t, w, 1, xy(1400, 1000), 0)
	addCollisionClass(t, w, ally, PathGround)
	addCollisionClass(t, w, groundEnemy, PathGround)
	addCollisionClass(t, w, airEnemy, PathAir)
	return w, shooter, ally, groundEnemy, airEnemy
}

func addCollisionClass(t *testing.T, w *World, id EntityID, flags uint8) {
	t.Helper()
	if !w.Collisions.Add(w.Ents, id, 0, flags) {
		t.Fatalf("Collisions.Add failed for %d", id)
	}
}

func missileStateDump(w *World, id EntityID) string {
	pr := w.ProjRender.Row(id)
	mr, okm := w.projMover(id)
	tr := w.Transforms.Row(id)
	if pr == -1 || !okm || tr == -1 {
		return fmt.Sprintf("alive=%v projrow=%d count=%d", w.Ents.Alive(id), pr, w.ProjRender.Count())
	}
	p := w.Transforms.Pos[tr]
	return fmt.Sprintf("alive=%v pos=(%d,%d) speed=%d accel=%d hit=%04x guid=%d range=%d pierce=%d count=%d",
		w.Ents.Alive(id), p.X.Floor(), p.Y.Floor(),
		int64(w.Movers.Speed[mr]), int64(w.Movers.Accel[mr]), w.Movers.HitMask[mr],
		w.ProjRender.Guidance[pr], int64(w.Movers.RangeLeft[mr]),
		w.Movers.Pierce[mr], w.ProjRender.Count())
}

func missileMaskDump(w *World, id, ally, groundEnemy, airEnemy EntityID) string {
	return fmt.Sprintf("missile={%s} allyLife=%d groundEnemyLife=%d airEnemyLife=%d",
		missileStateDump(w, id), life(w, ally), life(w, groundEnemy), life(w, airEnemy))
}

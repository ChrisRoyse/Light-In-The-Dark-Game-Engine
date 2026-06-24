package sim

// #590 — MoverDoneImpact: a completion mode that delivers the payload
// exactly ONCE at the mover's final position, reproducing the missile
// point/homing-arrival impact model (impactMissile). The migration needs
// this because moverDetonate fires the payload once PER masked unit in
// radius (AoE), which would re-run a point-targeted effect list N times.
// SoT = victim Healths.Life (read directly), the missile path for parity,
// and the state hash for the new DoneMode being real, saved state.

import (
	"bytes"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/statehash"
)

// Edge 1 (happy path): a MoverPoint arriving at its goal fires the packet
// ONCE at the impact target, the body is consumed, the slot is freed.
func TestMoverDoneImpactSingleDelivery(t *testing.T) {
	w := lmWorld(t)
	caster := atkUnit(t, w, 0, xy(1000, 1000), 0)
	foe := atkUnit(t, w, 1, xy(1100, 1000), 0)
	body, _ := w.CreateUnit(xy(1000, 1000), 0)
	mid := w.Movers.Create(MoverSpec{
		Kind: MoverPoint, Target: body, Owner: caster,
		Goal: xy(1100, 1000), Speed: 25 * fixed.One,
		Anchor: foe, DoneMode: MoverDoneImpact, Flags: MoverConsume,
		Packet: DamagePacket{Source: caster, Amount: 40 * fixed.One},
	})
	t.Logf("BEFORE foe=%d bodyAlive=%v moverAlive=%v", life(w, foe), w.Ents.Alive(body), w.Movers.Alive(mid))
	for i := 0; i < 6 && w.Movers.Alive(mid); i++ {
		w.Step()
		t.Logf("t%d foe=%d moverAlive=%v", i+1, life(w, foe), w.Movers.Alive(mid))
	}
	if life(w, foe) != 60 {
		t.Fatalf("foe life=%d, want 60 (single 40 delivery on arrival)", life(w, foe))
	}
	if w.Movers.Alive(mid) {
		t.Fatal("impact mover not freed after delivery")
	}
}

// Edge 2 (parity): a homing missile delivering to a stationary target on
// arrival and a MoverPoint+Anchor+DoneImpact deliver the SAME damage at the
// SAME tick — proving the impact model matches the missile it replaces.
func TestMoverDoneImpactMissileParity(t *testing.T) {
	const speed = 25 * fixed.One
	// missile homing-deliver
	wm := lmWorld(t)
	sm := atkUnit(t, wm, 0, xy(1000, 1000), 0)
	fm := atkUnit(t, wm, 1, xy(1100, 1000), 0)
	if _, ok := wm.SpawnMissile(MissileSpec{
		Pos: xy(1000, 1000), Source: sm, Speed: speed, Target: fm,
		GuidanceID: MissileGuidanceHoming,
		Packet:     DamagePacket{Source: sm, Amount: 40 * fixed.One},
	}); !ok {
		t.Fatal("missile spawn")
	}
	// mover point + impact
	wv := lmWorld(t)
	sv := atkUnit(t, wv, 0, xy(1000, 1000), 0)
	fv := atkUnit(t, wv, 1, xy(1100, 1000), 0)
	body, _ := wv.CreateUnit(xy(1000, 1000), 0)
	wv.Movers.Create(MoverSpec{
		Kind: MoverPoint, Target: body, Owner: sv,
		Goal: xy(1100, 1000), Speed: speed, Anchor: fv,
		DoneMode: MoverDoneImpact, Flags: MoverConsume,
		Packet:   DamagePacket{Source: sv, Amount: 40 * fixed.One},
	})
	for i := 0; i < 6; i++ {
		wm.Step()
		wv.Step()
		t.Logf("t%d missileFoe=%d moverFoe=%d", i+1, life(wm, fm), life(wv, fv))
		if life(wm, fm) != life(wv, fv) {
			t.Fatalf("tick %d: mover foe HP %d != missile foe HP %d (impact parity broken)", i+1, life(wv, fv), life(wm, fm))
		}
	}
	if life(wv, fv) != 60 {
		t.Fatalf("final foe HP=%d, want 60", life(wv, fv))
	}
}

// Edge 3 (discriminator): two foes sit at the goal. DoneImpact delivers to
// the single packet target only; DoneDetonate (the AoE mode) would hit both.
// Proves single-shot semantics vs per-unit AoE.
func TestMoverDoneImpactIsNotAoE(t *testing.T) {
	w := lmWorld(t)
	caster := atkUnit(t, w, 0, xy(1000, 1000), 0)
	foe1 := atkUnit(t, w, 1, xy(1100, 1000), 0)
	foe2 := atkUnit(t, w, 1, xy(1108, 1000), 0) // also within a small radius of the goal
	body, _ := w.CreateUnit(xy(1000, 1000), 0)
	w.Movers.Create(MoverSpec{
		Kind: MoverPoint, Target: body, Owner: caster,
		Goal: xy(1100, 1000), Speed: 25 * fixed.One,
		Anchor: foe1, DoneMode: MoverDoneImpact, Flags: MoverConsume,
		Radius: 50 * fixed.One, // a radius that WOULD catch foe2 under AoE
		Packet: DamagePacket{Source: caster, Amount: 40 * fixed.One},
	})
	for i := 0; i < 6; i++ {
		w.Step()
	}
	t.Logf("foe1=%d foe2=%d (want 60 / 100 — single-shot, not AoE)", life(w, foe1), life(w, foe2))
	if life(w, foe1) != 60 {
		t.Fatalf("foe1 (impact target) life=%d, want 60", life(w, foe1))
	}
	if life(w, foe2) != 100 {
		t.Fatalf("foe2 life=%d, want 100 — DoneImpact must be single-shot, not AoE (that is DoneDetonate)", life(w, foe2))
	}
}

// Edge 4 (guide died → point degrade): the Anchor dies before arrival, so
// the impact degrades to a point detonation that delivers to no one (packet
// target resolves to 0) rather than to a corpse. A bystander foe at the goal
// is untouched (no AoE). Fail-closed: never deliver to a dead guide.
func TestMoverDoneImpactGuideDiedDegrades(t *testing.T) {
	w := lmWorld(t)
	caster := atkUnit(t, w, 0, xy(1000, 1000), 0)
	guide := atkUnit(t, w, 1, xy(1100, 1000), 0)
	bystander := atkUnit(t, w, 1, xy(1100, 1000), 0) // same spot, would be AoE-caught
	body, _ := w.CreateUnit(xy(1000, 1000), 0)
	w.Movers.Create(MoverSpec{
		Kind: MoverPoint, Target: body, Owner: caster,
		Goal: xy(1100, 1000), Speed: 25 * fixed.One,
		Anchor: guide, DoneMode: MoverDoneImpact, Flags: MoverConsume,
		Packet: DamagePacket{Source: caster, Amount: 40 * fixed.One},
	})
	w.KillUnit(guide) // guide gone well before arrival
	for i := 0; i < 6; i++ {
		w.Step()
	}
	t.Logf("bystander=%d (want 100 — degraded point impact delivers to no one)", life(w, bystander))
	if life(w, bystander) != 100 {
		t.Fatalf("bystander life=%d, want 100 (a dead guide must not redirect the packet, and impact is not AoE)", life(w, bystander))
	}
}

// Edge 4b (homing arrival): a projectile homing mover (MoverConsume) snap-
// arrives at its live anchor and completes, delivering once via DoneImpact —
// the missile homing-impact model. SoT = anchor-foe Healths.Life + the mover
// being freed.
func TestMoverHomingArrivalDelivers(t *testing.T) {
	w := lmWorld(t)
	caster := atkUnit(t, w, 0, xy(1000, 1000), 0)
	foe := atkUnit(t, w, 1, xy(1100, 1000), 0)
	body, _ := w.CreateUnit(xy(1000, 1000), 0)
	mid := w.Movers.Create(MoverSpec{
		Kind: MoverHoming, Target: body, Owner: caster, Anchor: foe,
		Dir: xy(1, 0), Speed: 25 * fixed.One, TurnRate: 0,
		DoneMode: MoverDoneImpact, Flags: MoverConsume,
		Packet:   DamagePacket{Source: caster, Amount: 40 * fixed.One},
	})
	for i := 0; i < 8 && w.Movers.Alive(mid); i++ {
		w.Step()
		t.Logf("t%d foe=%d alive=%v", i+1, life(w, foe), w.Movers.Alive(mid))
	}
	if life(w, foe) != 60 {
		t.Fatalf("foe life=%d, want 60 (homing arrival delivered 40 once)", life(w, foe))
	}
	if w.Movers.Alive(mid) {
		t.Fatal("homing projectile not freed on arrival")
	}
}

// Edge 4c (homing parity): a homing missile and a MoverHoming+Consume+Impact,
// both to the same stationary target, deliver identical damage at the same tick.
func TestMoverHomingArrivalMissileParity(t *testing.T) {
	const speed = 25 * fixed.One
	wm := lmWorld(t)
	sm := atkUnit(t, wm, 0, xy(1000, 1000), 0)
	fm := atkUnit(t, wm, 1, xy(1090, 1040), 0) // off-axis so homing actually turns
	wm.SpawnMissile(MissileSpec{
		Pos: xy(1000, 1000), Source: sm, Speed: speed, Target: fm,
		GuidanceID: MissileGuidanceHoming,
		Packet:     DamagePacket{Source: sm, Amount: 40 * fixed.One},
	})
	wv := lmWorld(t)
	sv := atkUnit(t, wv, 0, xy(1000, 1000), 0)
	fv := atkUnit(t, wv, 1, xy(1090, 1040), 0)
	body, _ := wv.CreateUnit(xy(1000, 1000), 0)
	wv.Movers.Create(MoverSpec{
		Kind: MoverHoming, Target: body, Owner: sv, Anchor: fv,
		Dir: xy(1, 0), Speed: speed, TurnRate: 0,
		DoneMode: MoverDoneImpact, Flags: MoverConsume,
		Packet:   DamagePacket{Source: sv, Amount: 40 * fixed.One},
	})
	for i := 0; i < 10; i++ {
		wm.Step()
		wv.Step()
		if life(wm, fm) != life(wv, fv) {
			t.Fatalf("tick %d: mover foe %d != missile foe %d (homing impact parity broken)", i+1, life(wv, fv), life(wm, fm))
		}
	}
	if life(wv, fv) != 60 {
		t.Fatalf("final foe HP=%d, want 60", life(wv, fv))
	}
}

// Edge 4d (gate): a NON-consume homing mover (a guided unit) does NOT
// complete on reaching its anchor — it keeps pursuing. Proves arrival-
// complete is scoped to projectile bodies.
func TestMoverHomingNonConsumeDoesNotArrive(t *testing.T) {
	w := lmWorld(t)
	caster := atkUnit(t, w, 0, xy(1000, 1000), 0)
	foe := atkUnit(t, w, 1, xy(1050, 1000), 0)
	body, _ := w.CreateUnit(xy(1000, 1000), 0)
	mid := w.Movers.Create(MoverSpec{
		Kind: MoverHoming, Target: body, Owner: caster, Anchor: foe,
		Dir: xy(1, 0), Speed: 25 * fixed.One, TurnRate: 0,
		// no MoverConsume, no DoneImpact
	})
	for i := 0; i < 8; i++ {
		w.Step()
	}
	if !w.Movers.Alive(mid) {
		t.Fatal("non-consume homing mover completed on arrival — arrival-complete must be projectile-only")
	}
	t.Logf("non-consume homing still alive after passing anchor (correct): %v", w.Movers.Alive(mid))
}

// Edge 4e (guide death → expire): a projectile homing mover whose anchor
// dies mid-flight completes (expires) instead of flying forever — the
// missile guide-invalidation model. SoT = the mover is freed within a few
// ticks, not still flying.
func TestMoverHomingGuideDeathExpires(t *testing.T) {
	w := lmWorld(t)
	caster := atkUnit(t, w, 0, xy(1000, 1000), 0)
	anchor := atkUnit(t, w, 1, xy(5000, 1000), 0) // far away
	body, _ := w.CreateUnit(xy(1000, 1000), 0)
	mid := w.Movers.Create(MoverSpec{
		Kind: MoverHoming, Target: body, Owner: caster, Anchor: anchor,
		Dir: xy(1, 0), Speed: 50 * fixed.One, TurnRate: 0,
		DoneMode: MoverDoneExpire, Flags: MoverConsume,
		Packet:   DamagePacket{Source: caster, Amount: 40 * fixed.One},
	})
	w.KillUnit(anchor)
	freedAt := -1
	for i := 0; i < 6; i++ {
		w.Step()
		if !w.Movers.Alive(mid) {
			freedAt = i + 1
			break
		}
	}
	t.Logf("homing mover freed at tick %d after guide death (was at x=5000 away)", freedAt)
	if w.Movers.Alive(mid) {
		t.Fatal("homing mover still flying after its guide died — leak (must expire)")
	}
}

// Edge 5 (determinism/save): a mover mid-flight with DoneMode=Impact saves
// and reloads hash-identical, and the DoneMode column is real hashed state
// (mutating it moves the hash).
func TestMoverDoneImpactSaveAndHash(t *testing.T) {
	build := func() *World {
		w := lmWorld(t)
		caster := atkUnit(t, w, 0, xy(1000, 1000), 0)
		foe := atkUnit(t, w, 1, xy(1300, 1000), 0)
		body, _ := w.CreateUnit(xy(1000, 1000), 0)
		w.Movers.Create(MoverSpec{
			Kind: MoverPoint, Target: body, Owner: caster,
			Goal: xy(1300, 1000), Speed: 20 * fixed.One, Anchor: foe,
			DoneMode: MoverDoneImpact, Flags: MoverConsume,
			Packet:   DamagePacket{Source: caster, Amount: 40 * fixed.One},
		})
		w.Step() // mid-flight, not yet arrived
		return w
	}
	a := build()
	var sa statehash.Snapshot
	a.HashState(NewHashRegistry(), &sa)

	r := int32(0)
	for i := int32(1); i < int32(len(a.Movers.live)); i++ {
		if a.Movers.live[i] {
			r = i
			break
		}
	}
	if r == 0 {
		t.Fatal("no live mover")
	}
	orig := a.Movers.DoneMode[r]
	a.Movers.DoneMode[r] = uint8(MoverDoneExpire)
	var sa2 statehash.Snapshot
	a.HashState(NewHashRegistry(), &sa2)
	if sa2.Top == sa.Top {
		t.Fatal("DoneMode mutation invisible to the hash — not hashed state")
	}
	a.Movers.DoneMode[r] = orig

	var buf bytes.Buffer
	if err := a.SaveState(&buf, 3); err != nil {
		t.Fatal(err)
	}
	w2 := lmWorld(t)
	if err := w2.LoadState(bytes.NewReader(buf.Bytes()), 3); err != nil {
		t.Fatal(err)
	}
	var sl statehash.Snapshot
	w2.HashState(NewHashRegistry(), &sl)
	t.Logf("impact mover save/load: orig=%016x loaded=%016x", sa.Top, sl.Top)
	if sl.Top != sa.Top {
		t.Fatal("DoneImpact mover save/load diverged")
	}
}

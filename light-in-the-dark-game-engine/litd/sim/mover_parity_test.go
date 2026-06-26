package sim

import (
	"bytes"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/statehash"
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
	// (Collision once differed: the missile sweeps the segment with an
	// along-space projection + a fixed hit radius, while the default mover
	// does an endpoint radius test. #620 closed that gap — a MoverLinear with
	// the MoverSwept flag now reproduces the missile's swept hit test exactly;
	// see TestParityLinearSwept* below. This test still asserts only the
	// MOTION path, which is what save/replay determinism rides on.)
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

// ── #620 — linear COLLISION parity (the gap the body-trajectory test above
// called out). A swept MoverLinear (MoverSwept flag) must deliver to the
// SAME foes in the SAME tick as a linear missile, closing the last gap
// before MissileStore retirement (#590). The swept mover's Radius is set to
// missileHitRadius so its integer hit-test math is byte-identical to the
// missile's fixed radius. SoT = each foe's Healths.Life after each tick,
// read directly and compared missile↔mover, plus the OLD endpoint mover as
// a control that must TUNNEL (proving the swept option is load-bearing).

// sweptLinear builds a swept MoverLinear mimicking a linear missile spec.
func sweptLinear(w *World, owner, body EntityID, dir fixed.Vec2, speed, rng fixed.F64, pierce int32, decay uint16, amt fixed.F64) {
	w.Movers.Create(MoverSpec{
		Kind: MoverLinear, Target: body, Owner: owner, Dir: dir, Speed: speed,
		RangeLeft: rng, Radius: fixed.FromInt(missileHitRadius),
		HitMask: MissileHitEnemy, Pierce: pierce, Decay: decay,
		Packet: DamagePacket{Source: owner, Amount: amt},
		Flags:  MoverSwept | MoverConsume,
	})
}

// TestParityLinearSweptCollision: a fast shot whose foe sits MID-segment
// (between this tick's start and end). The missile's swept test and the
// swept mover both cross it; the legacy endpoint mover tunnels past it.
func TestParityLinearSweptCollision(t *testing.T) {
	base := xy(1000, 1000)
	foeAt := xy(1100, 1000) // along 100 — inside a 200/tick step, but 100 > radius from the endpoint (1200)
	const speed = 200 * fixed.One
	const dmg = 30 * fixed.One

	wm := lmWorld(t)
	sm := atkUnit(t, wm, 0, base, 0)
	fm := atkUnit(t, wm, 1, foeAt, 0)
	if _, ok := wm.SpawnMissile(MissileSpec{
		Pos: base, Source: sm, Speed: speed, Flags: MissileLinear,
		Dir: xy(1, 0), Range: 2000 * fixed.One, Pierce: 1,
		Packet: DamagePacket{Source: sm, Amount: dmg},
	}); !ok {
		t.Fatal("missile spawn")
	}

	ws := lmWorld(t)
	ss := atkUnit(t, ws, 0, base, 0)
	fs := atkUnit(t, ws, 1, foeAt, 0)
	bodyS, _ := ws.CreateUnit(base, 0)
	sweptLinear(ws, ss, bodyS, xy(1, 0), speed, 2000*fixed.One, 1, 0, dmg)

	we := lmWorld(t) // control: legacy endpoint model (no MoverSwept)
	se := atkUnit(t, we, 0, base, 0)
	fe := atkUnit(t, we, 1, foeAt, 0)
	bodyE, _ := we.CreateUnit(base, 0)
	we.Movers.Create(MoverSpec{
		Kind: MoverLinear, Target: bodyE, Owner: se, Dir: xy(1, 0), Speed: speed,
		RangeLeft: 2000 * fixed.One, Radius: fixed.FromInt(missileHitRadius),
		HitMask: MissileHitEnemy, Pierce: 1,
		Packet: DamagePacket{Source: se, Amount: dmg}, Flags: MoverConsume,
	})

	t.Logf("BEFORE foeHP  missile=%d swept=%d endpoint=%d", life(wm, fm), life(ws, fs), life(we, fe))
	for i := 0; i < 4; i++ {
		wm.Step()
		ws.Step()
		we.Step()
		t.Logf("t%d   foeHP  missile=%d swept=%d endpoint=%d", i+1, life(wm, fm), life(ws, fs), life(we, fe))
		if life(wm, fm) != life(ws, fs) {
			t.Fatalf("tick %d: swept foe HP %d != missile %d (same-tick collision parity broken)", i+1, life(ws, fs), life(wm, fm))
		}
	}
	if life(wm, fm) != 70 || life(ws, fs) != 70 {
		t.Fatalf("AFTER missile=%d swept=%d, want both 70 (30 dmg delivered)", life(wm, fm), life(ws, fs))
	}
	if life(we, fe) != 100 {
		t.Fatalf("AFTER endpoint foe=%d, want 100 — the legacy endpoint test must TUNNEL past a mid-segment foe (this is exactly why #620 adds the swept model)", life(we, fe))
	}
}

// TestParityLinearSweptPierceOrder: three foes in one fast window, pierce 3,
// no decay — the swept mover hits all three at the same tick and same
// front-to-back order as the missile (identical per-foe HP every tick).
func TestParityLinearSweptPierceOrder(t *testing.T) {
	base := xy(1000, 1000)
	foes := []fixed.Vec2{xy(1200, 1000), xy(1400, 1000), xy(1600, 1000)}
	const speed = 800 * fixed.One
	const dmg = 25 * fixed.One

	wm := lmWorld(t)
	sm := atkUnit(t, wm, 0, base, 0)
	fm := make([]EntityID, len(foes))
	for i, p := range foes {
		fm[i] = atkUnit(t, wm, 1, p, 0)
	}
	if _, ok := wm.SpawnMissile(MissileSpec{
		Pos: base, Source: sm, Speed: speed, Flags: MissileLinear,
		Dir: xy(1, 0), Range: 3000 * fixed.One, Pierce: 3,
		Packet: DamagePacket{Source: sm, Amount: dmg},
	}); !ok {
		t.Fatal("missile spawn")
	}

	ws := lmWorld(t)
	ss := atkUnit(t, ws, 0, base, 0)
	fs := make([]EntityID, len(foes))
	for i, p := range foes {
		fs[i] = atkUnit(t, ws, 1, p, 0)
	}
	bodyS, _ := ws.CreateUnit(base, 0)
	sweptLinear(ws, ss, bodyS, xy(1, 0), speed, 3000*fixed.One, 3, 0, dmg)

	for i := 0; i < 3; i++ {
		wm.Step()
		ws.Step()
		for k := range foes {
			if life(wm, fm[k]) != life(ws, fs[k]) {
				t.Fatalf("tick %d foe[%d]: swept HP %d != missile %d (pierce order/tick parity broken)", i+1, k, life(ws, fs[k]), life(wm, fm[k]))
			}
		}
		t.Logf("t%d  foeHP missile=%d/%d/%d swept=%d/%d/%d", i+1,
			life(wm, fm[0]), life(wm, fm[1]), life(wm, fm[2]),
			life(ws, fs[0]), life(ws, fs[1]), life(ws, fs[2]))
	}
	for k := range foes {
		if life(ws, fs[k]) != 75 {
			t.Fatalf("foe[%d] swept HP=%d, want 75 (all three pierced for 25)", k, life(ws, fs[k]))
		}
	}
}

// TestParityLinearSweptOffLaneAndCap: an off-lane foe (outside the lateral
// radius) is rejected by both paths, and a pierce-2 budget against 3 in-lane
// foes hits exactly the front two — missile and swept agree, mover consumed.
func TestParityLinearSweptOffLaneAndCap(t *testing.T) {
	base := xy(1000, 1000)
	f1At, f2At, f3At := xy(1200, 1000), xy(1400, 1000), xy(1600, 1000)
	offAt := xy(1300, 1400) // 400 off the lane — far outside radius 40
	const speed = 800 * fixed.One
	const dmg = 25 * fixed.One

	wm := lmWorld(t)
	sm := atkUnit(t, wm, 0, base, 0)
	m1, m2, m3 := atkUnit(t, wm, 1, f1At, 0), atkUnit(t, wm, 1, f2At, 0), atkUnit(t, wm, 1, f3At, 0)
	moff := atkUnit(t, wm, 1, offAt, 0)
	wm.SpawnMissile(MissileSpec{
		Pos: base, Source: sm, Speed: speed, Flags: MissileLinear,
		Dir: xy(1, 0), Range: 3000 * fixed.One, Pierce: 2,
		Packet: DamagePacket{Source: sm, Amount: dmg},
	})

	ws := lmWorld(t)
	ss := atkUnit(t, ws, 0, base, 0)
	s1, s2, s3 := atkUnit(t, ws, 1, f1At, 0), atkUnit(t, ws, 1, f2At, 0), atkUnit(t, ws, 1, f3At, 0)
	soff := atkUnit(t, ws, 1, offAt, 0)
	bodyS, _ := ws.CreateUnit(base, 0)
	mid := ws.Movers.Create(MoverSpec{
		Kind: MoverLinear, Target: bodyS, Owner: ss, Dir: xy(1, 0), Speed: speed,
		RangeLeft: 3000 * fixed.One, Radius: fixed.FromInt(missileHitRadius),
		HitMask: MissileHitEnemy, Pierce: 2,
		Packet: DamagePacket{Source: ss, Amount: dmg}, Flags: MoverSwept | MoverConsume,
	})

	wm.Step()
	ws.Step()
	t.Logf("missile f1/f2/f3/off = %d/%d/%d/%d", life(wm, m1), life(wm, m2), life(wm, m3), life(wm, moff))
	t.Logf("swept   f1/f2/f3/off = %d/%d/%d/%d", life(ws, s1), life(ws, s2), life(ws, s3), life(ws, soff))
	pairs := [][2]EntityID{{m1, s1}, {m2, s2}, {m3, s3}, {moff, soff}}
	for k, p := range pairs {
		if life(wm, p[0]) != life(ws, p[1]) {
			t.Fatalf("foe[%d]: missile %d != swept %d", k, life(wm, p[0]), life(ws, p[1]))
		}
	}
	if life(ws, s1) != 75 || life(ws, s2) != 75 {
		t.Fatalf("front two not both hit: f1=%d f2=%d", life(ws, s1), life(ws, s2))
	}
	if life(ws, s3) != 100 {
		t.Fatalf("third foe hit past the pierce cap: f3=%d", life(ws, s3))
	}
	if life(ws, soff) != 100 {
		t.Fatalf("off-lane foe hit: %d (lateral radius reject failed)", life(ws, soff))
	}
	if ws.Movers.Alive(mid) {
		t.Fatal("pierce-2 swept mover must be consumed once the budget is spent")
	}
}

// TestSweptLinearOwnerGoneFailClosed: a swept mover whose Owner has no team
// (unowned/vanished launcher) has nothing to classify foes against — it
// flies through, dealing nothing (the missile's sor==-1 fail-closed guard,
// ported #620). KillUnit is a DEFERRED cleanup, so the owner is made
// genuinely unowned here (no Owners row) to reach the guard on the first
// collision pass.
func TestSweptLinearOwnerGoneFailClosed(t *testing.T) {
	w := lmWorld(t)
	unowned, _ := w.CreateUnit(xy(1000, 1000), 0) // no Owners.Add → Owners.Row == -1
	foe := atkUnit(t, w, 1, xy(1100, 1000), 0)
	body, _ := w.CreateUnit(xy(1000, 1000), 0)
	sweptLinear(w, unowned, body, xy(1, 0), 200*fixed.One, 2000*fixed.One, 1, 0, 30*fixed.One)

	before := life(w, foe)
	w.Step()
	after := life(w, foe)
	t.Logf("unowned-launcher swept: foe HP before=%d after=%d", before, after)
	if after != before {
		t.Fatalf("swept mover with an unowned launcher dealt damage (foe %d -> %d) — fail-closed broken", before, after)
	}
}

// TestSweptLinearSaveParity: a swept mover mid-flight saves and reloads
// hash-identical (the MoverSwept flag is real, hashed state), and toggling
// the flag moves the hash (proving it is hashed, not silently dropped).
func TestSweptLinearSaveParity(t *testing.T) {
	build := func() *World {
		w := lmWorld(t)
		caster := atkUnit(t, w, 0, xy(1000, 1000), 0)
		atkUnit(t, w, 1, xy(1400, 1000), 0)
		body, _ := w.CreateUnit(xy(1000, 1000), 0)
		sweptLinear(w, caster, body, xy(1, 0), 80*fixed.One, 4000*fixed.One, 5, 0, 10*fixed.One)
		w.Step() // mid-flight
		return w
	}
	a := build()
	var sa statehash.Snapshot
	a.HashState(NewHashRegistry(), &sa)

	// the flag is hashed state: clear MoverSwept on the live mover → hash moves.
	r := int32(0)
	for i := int32(1); i < int32(len(a.Movers.live)); i++ {
		if a.Movers.live[i] {
			r = i
			break
		}
	}
	if r == 0 {
		t.Fatal("no live mover to mutate")
	}
	orig := a.Movers.Flags[r]
	a.Movers.Flags[r] = orig &^ MoverSwept
	var sa2 statehash.Snapshot
	a.HashState(NewHashRegistry(), &sa2)
	if sa2.Top == sa.Top {
		t.Fatal("MoverSwept flag mutation invisible to the hash — the bit is not hashed state")
	}
	a.Movers.Flags[r] = orig // restore

	var buf bytes.Buffer
	if err := a.SaveState(&buf, 7); err != nil {
		t.Fatal(err)
	}
	w2 := lmWorld(t)
	if err := w2.LoadState(bytes.NewReader(buf.Bytes()), 7); err != nil {
		t.Fatal(err)
	}
	var sl statehash.Snapshot
	w2.HashState(NewHashRegistry(), &sl)
	t.Logf("swept mover save/load: orig=%016x loaded=%016x", sa.Top, sl.Top)
	if sl.Top != sa.Top {
		t.Fatal("swept mover save/load diverged")
	}
}

// TestSweptLinearZeroAlloc: the swept collision advance allocates nothing
// (bucket-grid walk, no scratch slice), matching the missile's linear path.
func TestSweptLinearZeroAlloc(t *testing.T) {
	w := lmWorld(t)
	caster := atkUnit(t, w, 0, xy(1000, 1000), 0)
	for i := int32(0); i < 5; i++ {
		atkUnit(t, w, 1, xy(1200+i*60, 1000), 0)
	}
	body, _ := w.CreateUnit(xy(1000, 1000), 0)
	sweptLinear(w, caster, body, xy(1, 0), 80*fixed.One, 1<<40, 99, 0, 5*fixed.One)
	w.Step()
	allocs := testing.AllocsPerRun(50, func() { w.Step() })
	t.Logf("allocs/op advancing a swept linear mover: %v", allocs)
	if allocs != 0 {
		t.Fatalf("swept linear advance allocates: %v", allocs)
	}
}

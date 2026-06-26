package sim

import (
	"fmt"
	"strings"
	"testing"
	"testing/fstest"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
)

// msWorld: attacker (team 0) at x=1000, victim (team 1) at x=1400,
// neutral 1000‰ matrix. Returns world + ids.
func msWorld(t *testing.T) (*World, EntityID, EntityID) {
	t.Helper()
	w := NewWorld(Caps{})
	if err := w.BindDamageMatrix([][]int32{{1000}}); err != nil {
		t.Fatal(err)
	}
	a := atkUnit(t, w, 0, fixed.Vec2{X: 1000 * fixed.One, Y: 1000 * fixed.One}, 0)
	v := atkUnit(t, w, 1, fixed.Vec2{X: 1400 * fixed.One, Y: 1000 * fixed.One}, 0)
	return w, a, v
}

// missilePos renders a missile's position for flight traces.
func missilePos(w *World, id EntityID) string {
	tr := w.Transforms.Row(id)
	if tr == -1 {
		return "REMOVED"
	}
	p := w.Transforms.Pos[tr]
	return fmt.Sprintf("(%d,%d)", p.X.Floor(), p.Y.Floor())
}

// Edge 1: the source dies mid-flight; the homing missile still
// delivers (launch-rolled packet, dead source recorded as attacker).
func TestMissileHomingDeliversAfterSourceDeath(t *testing.T) {
	w, a, v := msWorld(t)
	id, ok := w.SpawnMissile(MissileSpec{
		Pos:    fixed.Vec2{X: 1000 * fixed.One, Y: 1000 * fixed.One},
		Source: a, Target: v, Speed: 100 * fixed.One,
		Packet: DamagePacket{Source: a, Target: v, Amount: 30 * fixed.One, AttackType: 0},
	})
	if !ok {
		t.Fatal("spawn failed")
	}
	var impacts []string
	w.OnMissileImpact = func(tick uint32, mid EntityID, at fixed.Vec2, tgt EntityID) {
		impacts = append(impacts, fmt.Sprintf("t%d impact at (%d,%d) tgt=%d", tick, at.X.Floor(), at.Y.Floor(), tgt))
	}
	w.KillUnit(a) // source dies tick 1, missile is 4 ticks out
	for i := 0; i < 6; i++ {
		w.Step()
		t.Logf("t%d missile@%s sourceAlive=%v", w.Tick(), missilePos(w, id), w.Ents.Alive(a))
	}
	for _, l := range impacts {
		t.Logf("%s", l)
	}
	hr := w.Healths.Row(v)
	if w.Healths.Life[hr] != 70*fixed.One {
		t.Fatalf("victim life = %d, want 70 — homing must deliver after source death", w.Healths.Life[hr])
	}
	cr := w.Combats.Row(v)
	if w.Combats.LastAttacker[cr] != a {
		t.Fatalf("LastAttacker = %d, want the dead source %d", w.Combats.LastAttacker[cr], a)
	}
	if w.Ents.Alive(id) {
		t.Fatal("missile must be removed after impact")
	}
}

// Edge 2a: target dies mid-flight, NO AoE flag → expire, payload
// never delivers.
func TestMissileTargetDeathExpires(t *testing.T) {
	w, a, v := msWorld(t)
	id, _ := w.SpawnMissile(MissileSpec{
		Pos:    fixed.Vec2{X: 1000 * fixed.One, Y: 1000 * fixed.One},
		Source: a, Target: v, Speed: 50 * fixed.One,
		Packet: DamagePacket{Source: a, Target: v, Amount: 30 * fixed.One},
	})
	expired := ""
	w.OnMissileExpire = func(tick uint32, mid EntityID, last fixed.Vec2) {
		expired = fmt.Sprintf("t%d expire, last known (%d,%d)", tick, last.X.Floor(), last.Y.Floor())
	}
	damaged := false
	w.RegisterHandler(1, func(w *World, e Event) { damaged = true })
	w.Subscribe(EvUnitDamaged, 1)
	w.Step()
	w.KillUnit(v)
	for i := 0; i < 8; i++ {
		w.Step()
		t.Logf("t%d missile@%s", w.Tick(), missilePos(w, id))
	}
	t.Logf("%s", expired)
	if expired == "" {
		t.Fatal("missile never expired")
	}
	if damaged {
		t.Fatal("expire variant must not deliver damage")
	}
}

// Edge 2b: same death, MissileAoE + area payload → continues to the
// last known position and detonates, hitting the bystander there.
func TestMissileTargetDeathDetonates(t *testing.T) {
	resetEffectExecs()
	t.Cleanup(resetEffectExecs)
	RegisterCoreEffectExecs()

	// compile area{damage} through the real loader
	fsys := fstest.MapFS{
		"combat/damage-table.toml": &fstest.MapFile{Data: []byte("attack-types = [\"normal\"]\narmor-types = [\"none\"]\n[coefficients]\nnormal = [1000]\n")},
		"abilities/core.toml": &fstest.MapFile{Data: []byte(`
[[ability]]
id = "blast"
name = "Blast"
[[ability.effects]]
prim = "area"
radius = 150.0
max-targets = 4
[[ability.effects.effects]]
prim = "damage"
amount = 25
attack-type = "normal"
`)},
	}
	tb, err := data.Load(fsys)
	if err != nil {
		t.Fatal(err)
	}

	w, a, v := msWorld(t)
	if err := w.BindEffects(tb.Effects); err != nil {
		t.Fatal(err)
	}
	// bystander 100 wu from the victim (inside blast radius 150)
	by := atkUnit(t, w, 1, fixed.Vec2{X: 1500 * fixed.One, Y: 1000 * fixed.One}, 0)

	id, _ := w.SpawnMissile(MissileSpec{
		Pos:    fixed.Vec2{X: 1000 * fixed.One, Y: 1000 * fixed.One},
		Source: a, Target: v, Speed: 50 * fixed.One, Flags: MissileAoE,
		Payload: tb.Abilities[0].Effects,
	})
	var impact string
	w.OnMissileImpact = func(tick uint32, mid EntityID, at fixed.Vec2, tgt EntityID) {
		impact = fmt.Sprintf("t%d detonate at (%d,%d) tgt=%d", tick, at.X.Floor(), at.Y.Floor(), tgt)
	}
	w.Step()
	w.KillUnit(v)
	for i := 0; i < 10; i++ {
		w.Step()
		t.Logf("t%d missile@%s", w.Tick(), missilePos(w, id))
	}
	t.Logf("%s", impact)
	if impact == "" {
		t.Fatal("AoE missile never detonated")
	}
	if !strings.Contains(impact, "tgt=0") {
		t.Fatalf("detonation must resolve at point (tgt=0): %s", impact)
	}
	bhr := w.Healths.Row(by)
	if w.Healths.Life[bhr] != 75*fixed.One {
		t.Fatalf("bystander life = %d, want 75 — detonate must splash the last known position", w.Healths.Life[bhr])
	}
}

// Edge 3: damage fixed at launch, mitigation at impact — armor
// changes mid-flight affect the result, the rolled amount does not.
func TestMissileDamageFixedAtLaunch(t *testing.T) {
	w, a, v := msWorld(t)
	launch := DamagePacket{Source: a, Target: v, Amount: 40 * fixed.One, AttackType: 0}
	t.Logf("packet at launch: %+v", launch)
	w.SpawnMissile(MissileSpec{
		Pos:    fixed.Vec2{X: 1000 * fixed.One, Y: 1000 * fixed.One},
		Source: a, Target: v, Speed: 50 * fixed.One,
		Packet: launch,
	})
	// buff the victim's armor mid-flight: 0 → 10 (multiplier ≈ 0.625)
	w.Step()
	w.Healths.ArmorValue[w.Healths.Row(v)] = 10
	var post fixed.F64
	w.RegisterHandler(1, func(w *World, e Event) { post = fixed.F64(e.Arg) })
	w.Subscribe(EvUnitDamaged, 1)
	for i := 0; i < 10; i++ {
		w.Step()
	}
	want := (40 * fixed.One).Mul(armorMult[10-ArmorLUTMin])
	t.Logf("applied breakdown: amount=40 (launch roll) × 1000‰ × armorMult(10)=%d → post=%d (want %d)",
		armorMult[10-ArmorLUTMin], post, want)
	if post != want {
		t.Fatalf("post = %d, want launch-amount × impact-armor = %d", post, want)
	}
}

// Edge 6: pool exhaustion — cap missiles spawn, cap+1 fails
// deterministically; impacts free rows for the next wave.
func TestMissilePoolExhaustion(t *testing.T) {
	w, a, v := msWorld(t)
	capN := w.Caps().Projectiles
	n := 0
	for i := 0; i < capN; i++ {
		if _, ok := w.SpawnMissile(MissileSpec{
			Pos:    fixed.Vec2{X: 1000 * fixed.One, Y: 1000 * fixed.One},
			Source: a, Target: v, Speed: fixed.One,
			Packet: DamagePacket{Source: a, Target: v, Amount: 1},
		}); ok {
			n++
		}
	}
	_, ok := w.SpawnMissile(MissileSpec{
		Pos:    fixed.Vec2{X: 1000 * fixed.One, Y: 1000 * fixed.One},
		Source: a, Target: v, Speed: fixed.One,
	})
	t.Logf("spawned %d/%d; spawn %d → ok=%v; live=%d", n, capN, capN+1, ok, w.ProjRender.Count())
	if n != capN || ok {
		t.Fatalf("want exactly %d spawns then deterministic refusal, got %d / ok=%v", capN, n, ok)
	}
}

// Edge 7 analogue: data referencing spawn-missile (no exec registered
// until missile-type rows exist) refuses to BIND — load-time error,
// never a runtime fallback.
func TestMissileSpawnPrimUnregisteredFailsClosed(t *testing.T) {
	resetEffectExecs()
	t.Cleanup(resetEffectExecs)
	RegisterCoreEffectExecs()
	arena := []data.CompiledEffect{{Prim: data.EPSpawnMissile}}
	w := NewWorld(Caps{})
	err := w.BindEffects(arena)
	t.Logf("bind error: %v", err)
	if err == nil || !strings.Contains(err.Error(), `"spawn-missile"`) {
		t.Fatalf("err = %v, want unregistered spawn-missile refusal", err)
	}
}

// Mid-flight retarget: missiles are ordinary entities — a GuideEnt
// write redirects the flight.
func TestMissileRetarget(t *testing.T) {
	w, a, v := msWorld(t)
	v2 := atkUnit(t, w, 1, fixed.Vec2{X: 1000 * fixed.One, Y: 1400 * fixed.One}, 0)
	id, _ := w.SpawnMissile(MissileSpec{
		Pos:    fixed.Vec2{X: 1000 * fixed.One, Y: 1000 * fixed.One},
		Source: a, Target: v, Speed: 60 * fixed.One,
		Packet: DamagePacket{Source: a, Target: v, Amount: 30 * fixed.One},
	})
	w.Step()
	mr, okm := w.projMover(id) // script-level retarget: redirect the mover's anchor
	if !okm {
		t.Fatal("projectile has no live mover")
	}
	w.Movers.Anchor[mr] = v2
	for i := 0; i < 12 && w.Ents.Alive(id); i++ {
		w.Step()
		t.Logf("t%d missile@%s", w.Tick(), missilePos(w, id))
	}
	if w.Healths.Life[w.Healths.Row(v)] != 100*fixed.One {
		t.Fatal("original target must be untouched after retarget")
	}
	if w.Healths.Life[w.Healths.Row(v2)] != 70*fixed.One {
		t.Fatalf("retargeted victim life = %d, want 70", w.Healths.Life[w.Healths.Row(v2)])
	}
}

// FIRE integration: a ProjSpeed weapon launches a missile entity at
// the FIRE edge; damage arrives ticks later, rolled at launch.
func TestMissileWeaponFire(t *testing.T) {
	w, a, v := msWorld(t)
	cr := w.Combats.Row(a)
	c := w.Combats
	c.DmgBase[cr][0] = 10
	c.Cooldown[cr][0] = 27
	c.DamagePt[cr][0] = 10
	c.Backswing[cr][0] = 10
	c.Range[cr][0] = 500 * fixed.One
	c.ProjSpeed[cr][0] = 100 * fixed.One
	c.Target[cr] = v
	hr := w.Healths.Row(v)
	fireTick, hitTick := uint32(0), uint32(0)
	w.OnAttackTransition = func(tick uint32, id EntityID, slot int, from, to uint8) {
		if to == AtkBackswing {
			fireTick = tick
		}
	}
	w.RegisterHandler(1, func(w *World, e Event) { hitTick = w.Tick() })
	w.Subscribe(EvUnitDamaged, 1)
	for i := 0; i < 20 && hitTick == 0; i++ {
		w.Step()
		if w.ProjRender.Count() > 0 {
			t.Logf("t%d missile in flight @%s", w.Tick(), missilePos(w, w.ProjRender.Entity[0]))
		}
	}
	t.Logf("FIRE at t%d, impact at t%d, victim life=%d", fireTick, hitTick, w.Healths.Life[hr])
	if fireTick == 0 || hitTick <= fireTick {
		t.Fatalf("impact (%d) must trail FIRE (%d) — flight, not instant", hitTick, fireTick)
	}
	if w.Healths.Life[hr] != 90*fixed.One {
		t.Fatalf("victim life = %d, want 90", w.Healths.Life[hr])
	}
}

// R-GC-1: flight advance allocates nothing.
func TestMissileAdvanceAllocs(t *testing.T) {
	w, a, v := msWorld(t)
	for i := 0; i < 64; i++ {
		w.SpawnMissile(MissileSpec{
			Pos:    fixed.Vec2{X: 1000 * fixed.One, Y: 1000 * fixed.One},
			Source: a, Target: v, Speed: fixed.One / 1024, // slow: never arrives during the measurement
		})
	}
	allocs := testing.AllocsPerRun(100, func() { w.moverSystem() })
	if allocs != 0 {
		t.Fatalf("projectile moverSystem allocates %v/run, want 0 (R-GC-1)", allocs)
	}
}

func BenchmarkMissileAdvance(b *testing.B) {
	w := NewWorld(Caps{})
	w.BindDamageMatrix([][]int32{{1000}})
	mk := func(team uint8, pos fixed.Vec2) EntityID {
		id, _ := w.CreateUnit(pos, 0)
		w.Owners.Add(w.Ents, id, team, team, team)
		w.Healths.Add(w.Ents, id, 100*fixed.One, 0, 0, 0)
		w.Combats.Add(w.Ents, id)
		return id
	}
	a := mk(0, fixed.Vec2{X: 1000 * fixed.One, Y: 1000 * fixed.One})
	v := mk(1, fixed.Vec2{X: 9000 * fixed.One, Y: 9000 * fixed.One})
	for i := 0; i < 500; i++ {
		w.SpawnMissile(MissileSpec{
			Pos:    fixed.Vec2{X: 1000 * fixed.One, Y: 1000 * fixed.One},
			Source: a, Target: v, Speed: fixed.One / 1024,
		})
	}
	b.ReportAllocs()
	b.ResetTimer() // setup (NewWorld + spawns) must not count
	for i := 0; i < b.N; i++ {
		w.moverSystem()
	}
}

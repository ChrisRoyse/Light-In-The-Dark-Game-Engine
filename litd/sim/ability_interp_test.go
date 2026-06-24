package sim

// #595 — ability op interpreter FSV. SoT = the instantiated primitives and
// their state read DIRECTLY after execution: the spawned projectile entity,
// the live mover columns, group membership, KV bytes, and the victims' Life
// after deferred/looped/branched ops resolve. Synthetic 50-damage impact
// effect (X=50, one hit ⇒ Life drops by exactly 50) so every outcome has a
// known expected value.

import (
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
)

// interpResolver resolves the fixed names used by these specs.
type interpResolver struct{}

func (interpResolver) EffectListByName(n string) (data.EffectList, bool) {
	if n == "impact" {
		return data.EffectList{Off: 0, Len: 1}, true // arena[0] = 50-damage
	}
	return data.EffectList{}, false
}
func (interpResolver) EventKindByName(n string) (uint16, bool) {
	if n == "ability.impact" {
		return 90, true
	}
	return 0, false
}
func (interpResolver) MoverKindByName(n string) (MoverKind, bool) {
	if n == "linear" {
		return MoverLinear, true
	}
	return 0, false
}
func (interpResolver) KeyID(string) uint32 { return 11 } // one shared synthetic key

// interpWorld builds a world with the 50-damage impact arena bound and the
// damage matrix installed. Returns the world and the caster (player 1).
func interpWorld(t *testing.T) (*World, EntityID) {
	t.Helper()
	resetEffectExecs()
	t.Cleanup(resetEffectExecs)
	RegisterEffectExec(data.EPDamage, func(w *World, ctx EffectCtx, e *data.CompiledEffect) {
		w.QueueDamage(DamagePacket{Source: ctx.Source, Target: ctx.Target, Amount: 50 * fixed.One})
	})
	w := NewWorld(Caps{Units: 64, Movers: 16})
	if err := w.BindEffects([]data.CompiledEffect{{Prim: data.EPDamage}}); err != nil {
		t.Fatalf("bind effects: %v", err)
	}
	if err := w.BindDamageMatrix(dmgMatrix); err != nil { // attack 0 / armor 0 = 1.0x
		t.Fatalf("bind matrix: %v", err)
	}
	caster, _ := w.CreateUnit(fixed.Vec2{}, 0)
	w.Owners.Add(w.Ents, caster, 1, 1, 1)
	return w, caster
}

func interpVictim(w *World, x, y int64) EntityID {
	id, _ := w.CreateUnit(fixed.Vec2{X: fixed.F64(x) * fixed.One, Y: fixed.F64(y) * fixed.One}, 0)
	w.Owners.Add(w.Ents, id, 2, 2, 2) // enemy team
	w.Healths.Add(w.Ents, id, 1000*fixed.One, 0, 0, 0)
	return id
}

func compile(t *testing.T, ops []data.OpSource) AbilitySpec {
	t.Helper()
	spec, err := compileSrc(data.AbilitySpecSource{ID: "test", OnCast: ops}, interpResolver{})
	if err != nil {
		t.Fatalf("compile: %v", err)
	}
	return spec
}

// TestInterpFireball: spawn_projectile + attach_mover(linear, payload=impact).
// SoT = the live mover columns, the projectile transform moving each tick, and
// the victim's Life after collision.
func TestInterpFireball(t *testing.T) {
	w, caster := interpWorld(t)
	victim := interpVictim(w, 20, 0)

	idx := w.AbilityDefs.RegisterSpec(compile(t, []data.OpSource{
		{Op: "spawn_projectile"},
		{Op: "attach_mover", Mover: "linear", Effects: "impact",
			Speed: 5, Range: 100, Radius: 8, HitMask: MissileHitEnemy, Pierce: 1},
	}))

	t.Logf("BEFORE: victim Life=%d, movers live=%d, units=%d",
		int64(lifeOf(w, victim)), w.Movers.Count(), w.unitCount)

	if !w.CastAbility(idx, caster, 0, fixed.Vec2{X: 20 * fixed.One}) {
		t.Fatal("CastAbility returned false")
	}

	// SoT after cast: a mover exists at slot 1 carrying the impact payload,
	// targeting the freshly-spawned projectile.
	r := int32(1)
	if !w.Movers.live[r] {
		t.Fatal("no live mover after attach_mover")
	}
	proj := w.Movers.Target[r]
	t.Logf("AFTER cast: mover slot=%d kind=%d target=%d speed=%d payloadLen=%d projPos=%v",
		r, w.Movers.Kind[r], proj, int64(w.Movers.Speed[r]), w.Movers.Payload[r].Len,
		w.Transforms.Pos[w.Transforms.Row(proj)])
	if w.Movers.Payload[r].Len != 1 {
		t.Fatalf("mover payload len=%d, want 1 (impact)", w.Movers.Payload[r].Len)
	}

	for tick := 1; tick <= 4; tick++ {
		w.Step()
		pr := w.Transforms.Row(proj)
		px := int64(0)
		if pr != -1 {
			px = int64(w.Transforms.Pos[pr].X / fixed.One)
		}
		t.Logf("  tick %d: projX=%d victimLife=%d moverLive=%v", tick, px, int64(lifeOf(w, victim)), w.Movers.live[r])
	}

	got := 1000*fixed.One - lifeOf(w, victim)
	if got != 50*fixed.One {
		t.Fatalf("victim damage=%d, want 50 (impact list on collision)", int64(got))
	}
	if w.Movers.live[r] {
		t.Fatal("pierce-1 mover should be consumed after its hit")
	}
	// SoT: the projectile body must be cleaned up (MoverConsume) — no leaked
	// unit slot per cast.
	if w.Transforms.Row(proj) != -1 {
		t.Fatalf("projectile %d not consumed after mover completion (leak)", proj)
	}
}

// TestInterpAfterDetonation: after{5 ticks}->run_effects(impact). SoT = the
// victim Life BEFORE the delay elapses (unchanged) and AFTER (−50).
func TestInterpAfterDetonation(t *testing.T) {
	w, caster := interpWorld(t)
	victim := interpVictim(w, 0, 0)
	idx := w.AbilityDefs.RegisterSpec(compile(t, []data.OpSource{
		{Op: "after", Count: 5, Children: []data.OpSource{{Op: "run_effects", Effects: "impact"}}},
	}))
	w.CastAbility(idx, caster, victim, fixed.Vec2{})

	for tick := 1; tick <= 4; tick++ {
		w.Step()
	}
	beforeFire := lifeOf(w, victim)
	t.Logf("BEFORE fire (tick 4): victim Life=%d (want 1000, delay not elapsed)", int64(beforeFire))
	if beforeFire != 1000*fixed.One {
		t.Fatalf("victim took damage before the after-delay: Life=%d", int64(beforeFire))
	}
	w.Step() // tick 5 → block fires this tick (phase 2 cont, phase 5 applies)
	afterFire := lifeOf(w, victim)
	t.Logf("AFTER fire (tick 5): victim Life=%d (want 950)", int64(afterFire))
	if 1000*fixed.One-afterFire != 50*fixed.One {
		t.Fatalf("after-detonation damage=%d, want 50", int64(1000*fixed.One-afterFire))
	}
}

// TestInterpForEachNova: fill_group(radius) + for_each_in_group->run_effects.
// SoT = group member count and EVERY enemy's Life (−50 each); the lone ally is
// untouched (enemy-only mask).
func TestInterpForEachNova(t *testing.T) {
	w, caster := interpWorld(t)
	e1 := interpVictim(w, 10, 0)
	e2 := interpVictim(w, 0, 10)
	e3 := interpVictim(w, -10, 0)
	ally, _ := w.CreateUnit(fixed.Vec2{X: 5 * fixed.One}, 0)
	w.Owners.Add(w.Ents, ally, 1, 1, 1) // caster's team
	w.Healths.Add(w.Ents, ally, 1000*fixed.One, 0, 0, 0)

	idx := w.AbilityDefs.RegisterSpec(compile(t, []data.OpSource{
		{Op: "fill_group", Radius: 50, HitMask: MissileHitEnemy},
		{Op: "for_each_in_group", Children: []data.OpSource{{Op: "run_effects", Effects: "impact"}}},
	}))
	t.Logf("BEFORE: e1=%d e2=%d e3=%d ally=%d",
		int64(lifeOf(w, e1)), int64(lifeOf(w, e2)), int64(lifeOf(w, e3)), int64(lifeOf(w, ally)))
	w.CastAbility(idx, caster, 0, fixed.Vec2{}) // centred at origin
	w.Step()                                    // flush queued damage
	t.Logf("AFTER: e1=%d e2=%d e3=%d ally=%d",
		int64(lifeOf(w, e1)), int64(lifeOf(w, e2)), int64(lifeOf(w, e3)), int64(lifeOf(w, ally)))
	for _, e := range []EntityID{e1, e2, e3} {
		if d := 1000*fixed.One - lifeOf(w, e); d != 50*fixed.One {
			t.Fatalf("enemy %d damage=%d, want 50", e, int64(d))
		}
	}
	if lifeOf(w, ally) != 1000*fixed.One {
		t.Fatalf("ally took nova damage (mask leak): Life=%d", int64(lifeOf(w, ally)))
	}
}

// TestInterpIfBranchKV: set_kv(11)=7; if(11==7)->run_effects fires; a second
// spec whose predicate is 11==8 does NOT fire. SoT = KV bytes + victim Life.
func TestInterpIfBranchKV(t *testing.T) {
	w, caster := interpWorld(t)
	victim := interpVictim(w, 0, 0)

	// Branch taken: KV value matches the predicate arg.
	idxHit := w.AbilityDefs.RegisterSpec(compile(t, []data.OpSource{
		{Op: "set_kv", Key: "hp", Arg: 7},
		{Op: "if", Key: "hp", Arg: 7, Children: []data.OpSource{{Op: "run_effects", Effects: "impact"}}},
	}))
	w.CastAbility(idxHit, caster, victim, fixed.Vec2{})
	_, kv, _, ok := w.KV.KVGet(EntityKVOwner(caster), 11)
	t.Logf("AFTER set_kv: KV[caster,11]=%d ok=%v", kv, ok)
	if !ok || kv != 7 {
		t.Fatalf("KV not stored: val=%d ok=%v want 7", kv, ok)
	}
	w.Step()
	if d := 1000*fixed.One - lifeOf(w, victim); d != 50*fixed.One {
		t.Fatalf("if-true branch did not fire: damage=%d want 50", int64(d))
	}
	t.Logf("branch TAKEN: victim Life=%d (−50)", int64(lifeOf(w, victim)))

	// Branch NOT taken: predicate arg differs from stored value.
	victim2 := interpVictim(w, 100, 0)
	idxMiss := w.AbilityDefs.RegisterSpec(compile(t, []data.OpSource{
		{Op: "set_kv", Key: "hp", Arg: 7},
		{Op: "if", Key: "hp", Arg: 8, Children: []data.OpSource{{Op: "run_effects", Effects: "impact"}}},
	}))
	w.CastAbility(idxMiss, caster, victim2, fixed.Vec2{})
	w.Step()
	if lifeOf(w, victim2) != 1000*fixed.One {
		t.Fatalf("if-false branch fired: Life=%d want 1000", int64(lifeOf(w, victim2)))
	}
	t.Logf("branch SKIPPED: victim2 Life=%d (untouched)", int64(lifeOf(w, victim2)))
}

// TestInterpTimesDoT: times{3 reps, every 2 ticks}->run_effects. SoT = victim
// Life after each window: −50 at tick 2, −100 at tick 4, −150 at tick 6.
func TestInterpTimesDoT(t *testing.T) {
	w, caster := interpWorld(t)
	victim := interpVictim(w, 0, 0)
	idx := w.AbilityDefs.RegisterSpec(compile(t, []data.OpSource{
		{Op: "times", Count: 3, Arg: 2, Children: []data.OpSource{{Op: "run_effects", Effects: "impact"}}},
	}))
	w.CastAbility(idx, caster, victim, fixed.Vec2{})
	want := map[int]int64{2: 50, 4: 100, 6: 150}
	for tick := 1; tick <= 7; tick++ {
		w.Step()
		dmg := int64((1000*fixed.One - lifeOf(w, victim)) / fixed.One)
		t.Logf("  tick %d: cumulative damage=%d", tick, dmg)
		if w, ok := want[tick]; ok && dmg != w {
			t.Fatalf("tick %d cumulative damage=%d, want %d", tick, dmg, w)
		}
	}
	if d := int64((1000*fixed.One - lifeOf(w, victim)) / fixed.One); d != 150 {
		t.Fatalf("final DoT damage=%d, want 150 (3×50)", d)
	}
}

// TestInterpCastZeroAlloc: executing a cast must not allocate (R-ABL-6) — the
// interpreter only touches pooled primitives and a stack-resident context.
func TestInterpCastZeroAlloc(t *testing.T) {
	w, caster := interpWorld(t)
	e1 := interpVictim(w, 10, 0)
	_ = e1
	idx := w.AbilityDefs.RegisterSpec(compile(t, []data.OpSource{
		{Op: "fill_group", Radius: 50, HitMask: MissileHitEnemy},
		{Op: "for_each_in_group", Children: []data.OpSource{
			{Op: "set_kv", Key: "hp", Arg: 1},
			{Op: "run_effects", Effects: "impact"},
		}},
	}))
	// Step inside the measured func so the deferred damage buffer drains each
	// iteration (an undrained QueueDamage buffer would grow and read as a false
	// alloc). The group is auto-freed per cast, so the pool does not leak.
	avg := testing.AllocsPerRun(200, func() {
		w.CastAbility(idx, caster, 0, fixed.Vec2{})
		w.Step()
	})
	if avg != 0 {
		t.Fatalf("CastAbility+Step allocated %.2f objs/op, want 0 (R-ABL-6)", avg)
	}
}

// TestInterpEmitEvent: emit_event rings a custom kind on the event ring. SoT =
// a subscribed handler's observed (kind, src, arg).
func TestInterpEmitEvent(t *testing.T) {
	w, caster := interpWorld(t)
	victim := interpVictim(w, 0, 0)
	var gotKind uint16
	var gotArg int64
	var fired int
	w.RegisterHandler(HandlerID(1), func(_ *World, e Event) {
		fired++
		gotKind, gotArg = e.Kind, e.Arg
	})
	w.Subscribe(90, HandlerID(1))
	idx := w.AbilityDefs.RegisterSpec(compile(t, []data.OpSource{
		{Op: "emit_event", Event: "ability.impact", Arg: 42},
	}))
	w.CastAbility(idx, caster, victim, fixed.Vec2{})
	w.Step() // event flush in phase 6
	t.Logf("AFTER: handler fired=%d kind=%d arg=%d", fired, gotKind, gotArg)
	if fired != 1 || gotKind != 90 || gotArg != 42 {
		t.Fatalf("emit_event: fired=%d kind=%d arg=%d, want 1/90/42", fired, gotKind, gotArg)
	}
}

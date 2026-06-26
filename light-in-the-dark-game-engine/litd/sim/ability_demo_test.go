package sim

// #601 — full-stack PRD2 demo: one composable ability that exercises ALL FIVE
// primitives end-to-end in a real world, proving they operate together
// deterministically (R-ABL-6). SoT = the actual sim state after the cast —
// KV bytes (03), the mover-struck victim's Life (05 + effect arena), the
// after-delayed AoE victims' Life (01 timer + 02 group), the custom-event log
// (04) — plus the 64-bit state hash (two runs identical) and a save-mid-ability
// → load → completes round-trip.

import (
	"bytes"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/statehash"
)

const flameAftershockName = "flame.aftershock"

// flameBurst is the demo ability source — touches all five primitives:
//
//	set_kv (03) → spawn_projectile + attach_mover (05, effect arena on hit) →
//	after (01) { fill_group (02) → for_each_in_group → run_effects ; emit_event (04) }
func flameBurst() data.AbilitySpecSource {
	return data.AbilitySpecSource{
		ID: "flame_burst", Name: "Flame Burst", CastType: "active",
		CastRange: 900, ManaCost: 10, Cooldown: 2.0, CastPoint: 0.05,
		OnCast: []data.OpSource{
			{Op: "set_kv", Key: "casts", Arg: 1},
			{Op: "spawn_projectile"},
			{Op: "attach_mover", Mover: "linear", Effects: "burst_hit",
				Speed: 10, Range: 300, Radius: 24, HitMask: MissileHitEnemy, Pierce: 1},
			{Op: "after", Count: 10, Children: []data.OpSource{
				{Op: "fill_group", HitMask: MissileHitEnemy, Radius: 200},
				{Op: "for_each_in_group", Children: []data.OpSource{{Op: "run_effects", Effects: "aoe_hit"}}},
				{Op: "emit_event", Event: flameAftershockName, Arg: 7},
			}},
		},
	}
}

// buildFlameWorld constructs the demo world and registers the ability. Effect
// execs are global; the caller resets/registers them once. Returns the world,
// the ability ref, the aftershock event kind, and the caster.
func buildFlameWorld(t *testing.T) (*World, uint16, uint16, EntityID) {
	t.Helper()
	w := NewWorld(Caps{Units: 32, Movers: 16})
	if err := w.BindEffects([]data.CompiledEffect{{Prim: data.EPDamage}}); err != nil {
		t.Fatalf("bind effects: %v", err)
	}
	if err := w.BindDamageMatrix(dmgMatrix); err != nil {
		t.Fatalf("bind matrix: %v", err)
	}
	w.RegisterEffectListName("burst_hit", EffectListSpan(0, 1))
	w.RegisterEffectListName("aoe_hit", EffectListSpan(0, 1))
	kind := w.CustomEvents.RegisterEventKind(flameAftershockName)
	ref, err := w.registerSrcAuto(flameBurst())
	if err != nil {
		t.Fatalf("register flame_burst: %v", err)
	}
	caster := atkUnit(t, w, 1, fixed.Vec2{}, 0)
	if w.Abilities.Row(caster) == -1 {
		w.Abilities.Add(w.Ents, caster)
	}
	ar := w.Abilities.Row(caster)
	w.Abilities.Mana[ar] = 100 * fixed.One
	w.Abilities.MaxMana[ar] = 100 * fixed.One
	if _, ok := w.grantAbilityRef(caster, ref); !ok {
		t.Fatal("grant flame_burst failed")
	}
	return w, ref, kind, caster
}

// addEnemy spawns an enemy unit at (x,0) with 1000 life.
func addEnemy(t *testing.T, w *World, x int64) EntityID {
	id := atkUnit(t, w, 2, fixed.Vec2{X: fixed.F64(x) * fixed.One}, 0)
	hr := w.Healths.Row(id)
	w.Healths.MaxLife[hr] = 1000 * fixed.One
	w.Healths.Life[hr] = 1000 * fixed.One
	return id
}

func TestFlameBurstFullStackFSV(t *testing.T) {
	resetEffectExecs()
	t.Cleanup(resetEffectExecs)
	RegisterEffectExec(data.EPDamage, func(w *World, ctx EffectCtx, e *data.CompiledEffect) {
		w.QueueDamage(DamagePacket{Source: ctx.Source, Target: ctx.Target, Amount: 40 * fixed.One})
	})

	w, ref, kind, caster := buildFlameWorld(t)
	target := addEnemy(t, w, 100) // along the mover path, also the cast target
	aoe := addEnemy(t, w, 160)    // only reachable by the after-delayed AoE

	var aftershocks int
	w.RegisterHandler(HandlerID(1), func(_ *World, e Event) {
		if e.Kind == kind {
			aftershocks++
		}
	})
	w.Subscribe(kind, HandlerID(1))

	t.Logf("BEFORE: target=%d aoe=%d KV=%v", int64(lifeOf(w, target)), int64(lifeOf(w, aoe)), kvInt(w, caster, "casts"))
	w.IssueOrder(caster, Order{Kind: OrderCastAbility, Target: target, Data: ref}, false)

	// Step through cast point (~tick 2: EFFECT edge spawns mover + schedules the
	// after block), the mover flight + impact, and the tick-~12 aftershock.
	for i := 0; i < 20; i++ {
		w.Step()
	}

	casts := kvInt(w, caster, "casts")
	tgtDmg := 1000*fixed.One - lifeOf(w, target)
	aoeDmg := 1000*fixed.One - lifeOf(w, aoe)
	t.Logf("AFTER: KV casts=%d targetDmg=%d aoeDmg=%d aftershocks=%d",
		casts, int64(tgtDmg), int64(aoeDmg), aftershocks)

	// SoT: KV write (03) landed.
	if casts != 1 {
		t.Fatalf("KV 'casts'=%d, want 1 (set_kv did not run)", casts)
	}
	// SoT: the mover (05) + effect arena struck the path target.
	if tgtDmg < 40*fixed.One {
		t.Fatalf("target damage=%d, want >=40 (mover impact)", int64(tgtDmg))
	}
	// SoT: the after-delayed (01) AoE (02) reached the off-path enemy.
	if aoeDmg != 40*fixed.One {
		t.Fatalf("aoe enemy damage=%d, want 40 (after→fill_group→for_each→run_effects)", int64(aoeDmg))
	}
	// SoT: the custom event (04) fired exactly once.
	if aftershocks != 1 {
		t.Fatalf("aftershock events=%d, want 1", aftershocks)
	}
}

func TestFlameBurstDeterministicHash(t *testing.T) {
	resetEffectExecs()
	t.Cleanup(resetEffectExecs)
	RegisterEffectExec(data.EPDamage, func(w *World, ctx EffectCtx, e *data.CompiledEffect) {
		w.QueueDamage(DamagePacket{Source: ctx.Source, Target: ctx.Target, Amount: 40 * fixed.One})
	})
	run := func() uint64 {
		w, ref, _, caster := buildFlameWorld(t)
		addEnemy(t, w, 100)
		addEnemy(t, w, 160)
		w.IssueOrder(caster, Order{Kind: OrderCastAbility, Target: w.Transforms.Entity[w.Transforms.Row(caster)+1], Data: ref}, false)
		for i := 0; i < 20; i++ {
			w.Step()
		}
		reg := NewHashRegistry()
		return w.HashState(reg, &statehash.Snapshot{}).Top
	}
	h1, h2 := run(), run()
	t.Logf("run1=%016x run2=%016x", h1, h2)
	if h1 != h2 {
		t.Fatalf("non-deterministic: %016x != %016x", h1, h2)
	}
}

// TestFlameBurstSaveMidAbilityResumes saves AFTER the cast scheduled its after
// block but BEFORE the block fires, loads into a fresh world, and confirms the
// AoE still detonates — the deferred ability block round-trips save/load.
func TestFlameBurstSaveMidAbilityResumes(t *testing.T) {
	resetEffectExecs()
	t.Cleanup(resetEffectExecs)
	RegisterEffectExec(data.EPDamage, func(w *World, ctx EffectCtx, e *data.CompiledEffect) {
		w.QueueDamage(DamagePacket{Source: ctx.Source, Target: ctx.Target, Amount: 40 * fixed.One})
	})

	w, ref, _, caster := buildFlameWorld(t)
	target := addEnemy(t, w, 100)
	aoe := addEnemy(t, w, 160)
	fp := w.JoinFingerprint()
	w.IssueOrder(caster, Order{Kind: OrderCastAbility, Target: target, Data: ref}, false)

	// Step just past the EFFECT edge so the after block is parked but unfired.
	for i := 0; i < 5; i++ {
		w.Step()
	}
	aoeBefore := lifeOf(w, aoe)
	t.Logf("AT SAVE (tick %d): aoe Life=%d (want full, block not fired yet)", w.tick, int64(aoeBefore))
	if aoeBefore != 1000*fixed.One {
		t.Fatalf("AoE already hit before save: Life=%d — move the save earlier", int64(aoeBefore))
	}

	var buf bytes.Buffer
	if err := w.SaveState(&buf, fp); err != nil {
		t.Fatalf("save: %v", err)
	}

	// Fresh world: re-register the data-side setup (effects/matrix/named lists/
	// event/ability) in the SAME order, then load. registerAbilityDispatch is
	// auto in NewWorld; the parked block resumes over the rebuilt AbilityBook.
	w2 := NewWorld(Caps{Units: 32, Movers: 16})
	if err := w2.BindEffects([]data.CompiledEffect{{Prim: data.EPDamage}}); err != nil {
		t.Fatal(err)
	}
	if err := w2.BindDamageMatrix(dmgMatrix); err != nil {
		t.Fatal(err)
	}
	w2.RegisterEffectListName("burst_hit", EffectListSpan(0, 1))
	w2.RegisterEffectListName("aoe_hit", EffectListSpan(0, 1))
	w2.CustomEvents.RegisterEventKind(flameAftershockName)
	if _, err := w2.registerSrcAuto(flameBurst()); err != nil {
		t.Fatalf("re-register ability: %v", err)
	}
	if err := w2.LoadState(bytes.NewReader(buf.Bytes()), fp); err != nil {
		t.Fatalf("load: %v", err)
	}

	// Resolve the AoE enemy in the loaded world (same EntityID) and step past
	// the scheduled fire tick.
	aoeAfterLoad := lifeOf(w2, aoe)
	t.Logf("AFTER LOAD (tick %d): aoe Life=%d", w2.tick, int64(aoeAfterLoad))
	for i := 0; i < 15; i++ {
		w2.Step()
	}
	aoeFinal := lifeOf(w2, aoe)
	t.Logf("AFTER RESUME (tick %d): aoe Life=%d (want 960 = -40 from the resumed aftershock)", w2.tick, int64(aoeFinal))
	if 1000*fixed.One-aoeFinal != 40*fixed.One {
		t.Fatalf("resumed aftershock did not fire: aoe damage=%d, want 40", int64(1000*fixed.One-aoeFinal))
	}
}

// kvInt reads an entity-scoped int KV by key name (SoT helper).
func kvInt(w *World, e EntityID, key string) int64 {
	id := w.KV.InternKey(key)
	_, v, _, ok := w.KV.KVGet(EntityKVOwner(e), id)
	if !ok {
		return -1
	}
	return v
}

package sim

import (
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
)

// dmgMatrix is the synthetic per-mille matrix: one attack type row
// per test scenario. Row 0 "neutral" = 1000 everywhere; row 1
// "strong" = 1500 vs armor-type 0.
var dmgMatrix = [][]int32{
	{1000, 700},
	{1500, 350},
}

// dmgWorld spawns a world with the synthetic matrix bound and two
// units: victim (armorValue, armorType as given) and attacker.
func dmgWorld(t *testing.T, armorValue int16, armorType uint8) (*World, EntityID, EntityID) {
	t.Helper()
	w := NewWorld(Caps{})
	if err := w.BindDamageMatrix(dmgMatrix); err != nil {
		t.Fatalf("bind matrix: %v", err)
	}
	mk := func(x int32, av int16, at uint8) EntityID {
		id, ok := w.CreateUnit(fixed.Vec2{X: fixed.FromInt(x), Y: fixed.FromInt(100)}, 0)
		if !ok ||
			!w.Healths.Add(w.Ents, id, 100*fixed.One, 0, av, at) ||
			!w.Combats.Add(w.Ents, id) {
			t.Fatal("spawn failed")
		}
		return id
	}
	victim := mk(100, armorValue, armorType)
	attacker := mk(200, 0, 0)
	return w, victim, attacker
}

// stepWithPackets queues the packets from inside phase 5 (the
// OnCombatPhase hook — exactly where attack cycles and effect execs
// run) and advances one tick.
func stepWithPackets(w *World, ps ...DamagePacket) {
	w.OnCombatPhase = func(tick uint32) {
		for _, p := range ps {
			w.QueueDamage(p)
		}
		w.OnCombatPhase = nil
	}
	w.Step()
}

// Armor LUT spot checks against hand-computed values (X+X=Y
// discipline: armor 0 → exactly 1.0; armor 10 → 1/1.6 = 0.625).
func TestDamageArmorLUT(t *testing.T) {
	get := func(a int) fixed.F64 { return armorMult[a-ArmorLUTMin] }
	if got := get(0); got != fixed.One {
		t.Fatalf("armor 0 multiplier = %d, want exactly 1.0 (%d)", got, fixed.One)
	}
	// armor 10: ideal 1/(1+0.6) = 0.625; k truncates at 2^-32 before
	// the divide, so allow a few ulp — the value is still bit-stable
	// across platforms (pure fixed-point construction)
	ideal := fixed.One.Mul(fixed.FromInt(1000)).Div(fixed.FromInt(1600))
	if got := get(10); got < ideal-8 || got > ideal+8 {
		t.Fatalf("armor 10 multiplier = %d, want 0.625 ± 8 ulp (%d)", got, ideal)
	}
	// monotone decreasing over the whole range
	for a := ArmorLUTMin + 1; a <= ArmorLUTMax; a++ {
		if get(a) >= get(a-1) {
			t.Fatalf("LUT not strictly decreasing at armor %d: %d >= %d", a, get(a), get(a-1))
		}
	}
	// negative armor amplifies: armor −5 → 2 − 0.94^5 ≈ 1.2665
	if got := get(-5); got <= fixed.One || got >= 2*fixed.One {
		t.Fatalf("armor -5 multiplier = %d, want amplification in (1, 2)", got)
	}
	t.Logf("LUT: m(-20)=%d m(-5)=%d m(0)=%d m(10)=%d m(100)=%d",
		get(-20), get(-5), get(0), get(10), get(100))
}

// Happy path with exact arithmetic: amount 40, attack-type 1 vs
// armor-type 0 (coeff 1500), armor 0 → 40 × 1.5 × 1.0 = 60 damage,
// life 100 → 40.
func TestDamageHappyPath(t *testing.T) {
	w, victim, attacker := dmgWorld(t, 0, 0)
	hr := w.Healths.Row(victim)
	before := w.Healths.Life[hr]

	var damagedEvents []Event
	w.RegisterHandler(1, func(w *World, e Event) {
		if e.Kind == EvUnitDamaged {
			damagedEvents = append(damagedEvents, e)
		}
	})
	w.Subscribe(EvUnitDamaged, 1)

	stepWithPackets(w, DamagePacket{Source: attacker, Target: victim, Amount: 40 * fixed.One, AttackType: 1})

	after := w.Healths.Life[w.Healths.Row(victim)]
	t.Logf("life before=%d after=%d (raw fixed)", before, after)
	if want := 40 * fixed.One; after != want {
		t.Fatalf("life = %d, want %d (100 − 40×1500‰×1.0)", after, want)
	}
	cr := w.Combats.Row(victim)
	if w.Combats.LastAttacker[cr] != attacker || w.Combats.LastDamagedTick[cr] != w.Tick() {
		t.Fatalf("threat memory: attacker=%d tick=%d, want %d/%d",
			w.Combats.LastAttacker[cr], w.Combats.LastDamagedTick[cr], attacker, w.Tick())
	}
	if len(damagedEvents) != 1 || damagedEvents[0].Arg != int64(60*fixed.One) {
		t.Fatalf("EvUnitDamaged = %+v, want one event with Arg=%d", damagedEvents, int64(60*fixed.One))
	}
}

// Mutual kill: A and B each queue a lethal packet at the other in
// the same tick. Both apply (buffer order), both die, both death
// events fire, both entities are gone after phase 7.
func TestDamageMutualKill(t *testing.T) {
	w, a, b := dmgWorld(t, 0, 0)
	var deaths []EntityID
	w.OnDeathEvent = func(tick uint32, id EntityID) { deaths = append(deaths, id) }

	stepWithPackets(w,
		DamagePacket{Source: a, Target: b, Amount: 150 * fixed.One, AttackType: 0},
		DamagePacket{Source: b, Target: a, Amount: 150 * fixed.One, AttackType: 0},
	)
	if len(deaths) != 2 || deaths[0] != b || deaths[1] != a {
		t.Fatalf("deaths = %v, want [%d %d] in packet order", deaths, b, a)
	}
	if w.Ents.Alive(a) || w.Ents.Alive(b) {
		t.Fatal("mutual kill: both must be removed after phase 7")
	}
}

// Overkill stacking: two packets on one victim, first already
// lethal. Life floors at 0, exactly one kill, second packet still
// records the later attacker (it landed).
func TestDamageOverkillStacking(t *testing.T) {
	w, victim, attacker := dmgWorld(t, 0, 0)
	deaths := 0
	w.OnDeathEvent = func(tick uint32, id EntityID) { deaths++ }

	stepWithPackets(w,
		DamagePacket{Source: attacker, Target: victim, Amount: 500 * fixed.One, AttackType: 0},
		DamagePacket{Source: attacker, Target: victim, Amount: 500 * fixed.One, AttackType: 0},
	)
	if deaths != 1 {
		t.Fatalf("deaths = %d, want exactly 1 (KillUnit dedupes)", deaths)
	}
	if w.Ents.Alive(victim) {
		t.Fatal("victim must be removed")
	}
}

// Zero-amount packet: life unchanged, threat memory and the damage
// event still record the hit (post-mitigation 0).
func TestDamageZeroAmount(t *testing.T) {
	w, victim, attacker := dmgWorld(t, 0, 0)
	stepWithPackets(w, DamagePacket{Source: attacker, Target: victim, Amount: 0, AttackType: 0})
	hr := w.Healths.Row(victim)
	if w.Healths.Life[hr] != 100*fixed.One {
		t.Fatalf("life = %d, want untouched 100", w.Healths.Life[hr])
	}
	cr := w.Combats.Row(victim)
	if w.Combats.LastAttacker[cr] != attacker {
		t.Fatal("zero-amount packet must still record the attacker")
	}
}

// Dead-source packet: the source died this tick BEFORE apply; the
// packet still lands (damage was in flight), LastAttacker points at
// the dead entity — stale-by-design, acquire.go validates liveness.
func TestDamageDeadSource(t *testing.T) {
	w, victim, attacker := dmgWorld(t, 0, 0)
	w.OnCombatPhase = func(tick uint32) {
		w.QueueDamage(DamagePacket{Source: victim, Target: attacker, Amount: 500 * fixed.One, AttackType: 0}) // kills attacker
		w.QueueDamage(DamagePacket{Source: attacker, Target: victim, Amount: 30 * fixed.One, AttackType: 0})  // attacker's dying blow
		w.OnCombatPhase = nil
	}
	w.Step()
	if w.Ents.Alive(attacker) {
		t.Fatal("attacker must be dead")
	}
	hr := w.Healths.Row(victim)
	if want := 70 * fixed.One; w.Healths.Life[hr] != want {
		t.Fatalf("life = %d, want %d — dead-source packet must still apply", w.Healths.Life[hr], want)
	}
}

// Dead-target packet: deterministic no-op — no event, no drop count.
func TestDamageDeadTarget(t *testing.T) {
	w, victim, attacker := dmgWorld(t, 0, 0)
	stepWithPackets(w, DamagePacket{Source: attacker, Target: victim, Amount: 500 * fixed.One, AttackType: 0})
	if w.Ents.Alive(victim) {
		t.Fatal("setup: victim must be dead")
	}
	stepWithPackets(w, DamagePacket{Source: attacker, Target: victim, Amount: 50 * fixed.One, AttackType: 0})
	if w.DamageDropped() != 0 {
		t.Fatalf("dead-target packet counted as drop (%d) — it is a no-op, not an error", w.DamageDropped())
	}
}

// Buffer full: counted drop, never silent, queue keeps working next
// tick after the apply pass empties the buffer.
func TestDamageBufferFullFailClosed(t *testing.T) {
	w, victim, attacker := dmgWorld(t, 0, 0)
	w.OnCombatPhase = func(tick uint32) {
		p := DamagePacket{Source: attacker, Target: victim, Amount: 1, AttackType: 0}
		for i := 0; i < cap(w.dmgBuf); i++ {
			if !w.QueueDamage(p) {
				t.Fatalf("queue refused at %d/%d", i, cap(w.dmgBuf))
			}
		}
		if w.QueueDamage(p) {
			t.Fatal("queue accepted past capacity")
		}
		w.OnCombatPhase = nil
	}
	w.Step()
	if w.DamageDropped() != 1 {
		t.Fatalf("DamageDropped = %d, want 1", w.DamageDropped())
	}
	// buffer drained: next tick queues fine
	if len(w.dmgBuf) != 0 {
		t.Fatalf("buffer not drained: %d", len(w.dmgBuf))
	}
}

// Unbound matrix: packets drop counted — damage NEVER applies with a
// guessed coefficient.
func TestDamageUnboundMatrix(t *testing.T) {
	w := NewWorld(Caps{})
	id, _ := w.CreateUnit(fixed.Vec2{X: fixed.One, Y: fixed.One}, 0)
	w.Healths.Add(w.Ents, id, 100*fixed.One, 0, 0, 0)
	stepWithPackets(w, DamagePacket{Source: id, Target: id, Amount: 50 * fixed.One, AttackType: 0})
	if w.Healths.Life[w.Healths.Row(id)] != 100*fixed.One {
		t.Fatal("damage applied without a bound matrix")
	}
	if w.DamageDropped() != 1 {
		t.Fatalf("DamageDropped = %d, want 1 (fail closed, counted)", w.DamageDropped())
	}
}

// Negative armor amplifies; armor beyond the LUT clamps to the edge.
func TestDamageNegativeAndClampedArmor(t *testing.T) {
	w, victim, attacker := dmgWorld(t, -5, 0)
	stepWithPackets(w, DamagePacket{Source: attacker, Target: victim, Amount: 40 * fixed.One, AttackType: 0})
	lifeNeg := w.Healths.Life[w.Healths.Row(victim)]
	if lifeNeg >= 60*fixed.One {
		t.Fatalf("armor -5 took %d post-damage life, want amplified (< 60)", lifeNeg)
	}

	w2, victim2, attacker2 := dmgWorld(t, -1000, 0) // far below LUT min: clamps to -20
	stepWithPackets(w2, DamagePacket{Source: attacker2, Target: victim2, Amount: 40 * fixed.One, AttackType: 0})
	lifeClamped := w2.Healths.Life[w2.Healths.Row(victim2)]
	wantMult := armorMult[0] // -20 entry
	wantLife := (100 * fixed.One).Sub((40 * fixed.One).Mul(wantMult))
	if lifeClamped != wantLife {
		t.Fatalf("armor -1000 life = %d, want clamped-to-(-20) result %d", lifeClamped, wantLife)
	}
	t.Logf("armor -5 life=%d; armor -1000 (clamped) life=%d", lifeNeg, lifeClamped)
}

// The damage primitive backend: a compiled composition queues real
// packets; dice rolls draw the sim PRNG — same seed, same rolls.
func TestDamageExecPrimitive(t *testing.T) {
	resetEffectExecs()
	t.Cleanup(resetEffectExecs)
	RegisterCoreEffectExecs()

	arena := []data.CompiledEffect{
		// schema order: amount, dice, sides, attack-type
		{Prim: data.EPDamage, Params: [data.MaxEffectParams]int64{10, 2, 6, 1}},
	}
	run := func(seed uint64) fixed.F64 {
		w, victim, attacker := dmgWorld(t, 0, 0)
		if err := w.BindEffects(arena); err != nil {
			t.Fatal(err)
		}
		w.SetSeed(seed)
		w.OnCombatPhase = func(tick uint32) {
			w.ExecuteEffects(data.EffectList{Off: 0, Len: 1}, EffectCtx{Source: attacker, Target: victim})
			w.OnCombatPhase = nil
		}
		w.Step()
		return w.Healths.Life[w.Healths.Row(victim)]
	}
	a, b := run(42), run(42)
	if a != b {
		t.Fatalf("same seed diverged: %d vs %d", a, b)
	}
	// amount ∈ [10+2, 10+12] = [12,22]; coeff 1500‰ → damage ∈ [18,33]
	dmg := (100 * fixed.One).Sub(a)
	if dmg < 18*fixed.One || dmg > 33*fixed.One {
		t.Fatalf("rolled damage %d outside [18,33]", dmg)
	}
	c := run(43)
	t.Logf("seed42 life=%d seed43 life=%d", a, c)
}

// R-GC-1: queue + apply allocates nothing at steady state.
func TestDamageApplyAllocs(t *testing.T) {
	w, victim, attacker := dmgWorld(t, 2, 1)
	p := DamagePacket{Source: attacker, Target: victim, Amount: fixed.One / 1024, AttackType: 0}
	allocs := testing.AllocsPerRun(200, func() {
		for i := 0; i < 8; i++ {
			w.QueueDamage(p)
		}
		w.damageApplySystem()
	})
	if allocs != 0 {
		t.Fatalf("damage pipeline allocates %v/run, want 0 (R-GC-1)", allocs)
	}
}

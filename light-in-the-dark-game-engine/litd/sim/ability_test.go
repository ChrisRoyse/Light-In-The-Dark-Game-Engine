package sim

import (
	"fmt"
	"testing"
	"testing/fstest"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
)

// abilityTables loads one castable ability through the REAL loader:
// firebolt — mana 20, cooldown 1.35s→27t, cast-point 0.5s→10t,
// backswing 0.5s→10t, range 500, payload damage 30; plus "torrent"
// with a 1s channel.
func abilityTables(t *testing.T) *data.Tables {
	t.Helper()
	fsys := fstest.MapFS{
		"combat/damage-table.toml": &fstest.MapFile{Data: []byte("attack-types = [\"magic\"]\narmor-types = [\"none\"]\n[coefficients]\nmagic = [1000]\n")},
		"abilities/core.toml": &fstest.MapFile{Data: []byte(`
[[ability]]
id = "firebolt"
name = "Firebolt"
mana-cost = 20
cooldown = 1.35
cast-point = 0.5
backswing = 0.5
cast-range = 500
[[ability.effects]]
prim = "damage"
amount = 30
attack-type = "magic"

[[ability]]
id = "torrent"
name = "Torrent"
mana-cost = 40
cooldown = 2.0
cast-point = 0.25
channel = 1.0
backswing = 0.25
cast-range = 500
[[ability.effects]]
prim = "damage"
amount = 10
attack-type = "magic"
`)},
	}
	tb, err := data.Load(fsys)
	if err != nil {
		t.Fatalf("ability tables must load: %v", err)
	}
	return tb
}

// abilityWorld: caster (team 0, 100 mana, regen 0) with firebolt in
// slot 0 and torrent in slot 1; victim (team 1) 100 wu away.
func abilityWorld(t *testing.T) (*World, *data.Tables, EntityID, EntityID, *[]string) {
	t.Helper()
	resetEffectExecs()
	t.Cleanup(resetEffectExecs)
	RegisterCoreEffectExecs()
	tb := abilityTables(t)
	w := NewWorld(Caps{})
	if err := w.BindDamageMatrix(tb.Coeff); err != nil {
		t.Fatal(err)
	}
	if err := w.BindEffects(tb.Effects); err != nil {
		t.Fatal(err)
	}
	if !w.BindAbilityDefs(tb.Abilities) {
		t.Fatal("BindAbilityDefs failed")
	}
	caster := atkUnit(t, w, 0, fixed.Vec2{X: 1000 * fixed.One, Y: 1000 * fixed.One}, 0)
	victim := atkUnit(t, w, 1, fixed.Vec2{X: 1100 * fixed.One, Y: 1000 * fixed.One}, 0)
	if !w.Abilities.Add(w.Ents, caster) {
		t.Fatal("ability row add failed")
	}
	ar := w.Abilities.Row(caster)
	w.Abilities.Mana[ar] = 100 * fixed.One
	w.Abilities.MaxMana[ar] = 100 * fixed.One
	for i := range tb.Abilities {
		if !w.SetAbility(caster, i, i) {
			t.Fatalf("SetAbility %d failed", i)
		}
	}
	trace := &[]string{}
	w.OnCastTransition = func(tick uint32, id EntityID, slot int, from, to uint8) {
		*trace = append(*trace, fmt.Sprintf("t%d e%d s%d %s→%s", tick, id, slot, CastStateName(from), CastStateName(to)))
	}
	return w, tb, caster, victim, trace
}

// abilityRef returns defIndex+1 for an ability ID.
func abilityRef(t *testing.T, tb *data.Tables, id string) uint16 {
	t.Helper()
	for i := range tb.Abilities {
		if tb.Abilities[i].ID == id {
			return uint16(i + 1)
		}
	}
	t.Fatalf("ability %q not in tables", id)
	return 0
}

// Happy cast: castpoint exactly 10 ticks, EFFECT lands in the same
// tick's damage pass, cooldown clock = effectTick+27, mana 100→80,
// order completes after backswing.
func TestAbilityCastHappyPath(t *testing.T) {
	w, tb, caster, victim, trace := abilityWorld(t)
	ar := w.Abilities.Row(caster)
	ref := abilityRef(t, tb, "firebolt")
	w.IssueOrder(caster, Order{Kind: OrderCastAbility, Target: victim, Data: ref}, false)
	var doneArg int64 = -1
	w.RegisterHandler(1, func(w *World, e Event) {
		if e.Kind == EvOrderDone && e.Src == caster {
			doneArg = e.Arg
		}
	})
	w.Subscribe(EvOrderDone, 1)
	for i := 0; i < 25; i++ {
		w.Step()
	}
	for _, l := range *trace {
		t.Logf("%s", l)
	}
	t.Logf("mana=%d (raw) ReadyAt[0]=%d victimLife=%d doneArg=%d",
		w.Abilities.Mana[ar], w.Abilities.ReadyAt[ar][0],
		w.Healths.Life[w.Healths.Row(victim)], doneArg)
	want := []string{
		"t1 e1 s0 ready→castpoint",
		"t11 e1 s0 castpoint→backswing",
		"t21 e1 s0 backswing→ready",
	}
	for i := range want {
		if (*trace)[i] != want[i] {
			t.Fatalf("trace[%d] = %q, want %q", i, (*trace)[i], want[i])
		}
	}
	if w.Abilities.Mana[ar] != 80*fixed.One {
		t.Fatalf("mana = %d, want 80 (cost 20 spent)", w.Abilities.Mana[ar])
	}
	if w.Abilities.ReadyAt[ar][0] != 11+27 {
		t.Fatalf("ReadyAt = %d, want 38 (effect tick 11 + 27)", w.Abilities.ReadyAt[ar][0])
	}
	if w.Healths.Life[w.Healths.Row(victim)] != 70*fixed.One {
		t.Fatal("EFFECT must deliver the 30-damage composition")
	}
	if doneArg != 1 {
		t.Fatalf("order done arg = %d, want 1 (success)", doneArg)
	}
}

// Edge 1: interrupt during CASTPOINT — mana refunded, cooldown never
// started, no effect.
func TestAbilityCastpointInterruptRefunds(t *testing.T) {
	w, tb, caster, victim, trace := abilityWorld(t)
	ar := w.Abilities.Row(caster)
	w.IssueOrder(caster, Order{Kind: OrderCastAbility, Target: victim, Data: abilityRef(t, tb, "firebolt")}, false)
	for i := 0; i < 5; i++ { // castpoint t1..11, interrupt at t6
		w.Step()
	}
	manaBefore, readyBefore := w.Abilities.Mana[ar], w.Abilities.ReadyAt[ar][0]
	w.IssueOrder(caster, Order{Kind: OrderMove, Point: fixed.Vec2{X: 2000 * fixed.One, Y: 1000 * fixed.One}}, false)
	w.Step()
	for _, l := range *trace {
		t.Logf("%s", l)
	}
	t.Logf("mana before interrupt=%d after=%d; ReadyAt before=%d after=%d",
		manaBefore, w.Abilities.Mana[ar], readyBefore, w.Abilities.ReadyAt[ar][0])
	if manaBefore != 80*fixed.One {
		t.Fatalf("setup: mana during castpoint = %d, want 80 (spent on entry)", manaBefore)
	}
	if w.Abilities.Mana[ar] != 100*fixed.One {
		t.Fatalf("mana = %d, want 100 — castpoint cancel refunds", w.Abilities.Mana[ar])
	}
	if w.Abilities.ReadyAt[ar][0] != 0 {
		t.Fatalf("ReadyAt = %d, want 0 — cooldown must not start", w.Abilities.ReadyAt[ar][0])
	}
	if w.Healths.Life[w.Healths.Row(victim)] != 100*fixed.One {
		t.Fatal("no effect may fire on a canceled castpoint")
	}
}

// Edge 2: interrupt during CHANNEL — cost spent, cooldown running.
func TestAbilityChannelInterruptKeepsCost(t *testing.T) {
	w, tb, caster, victim, trace := abilityWorld(t)
	ar := w.Abilities.Row(caster)
	w.IssueOrder(caster, Order{Kind: OrderCastAbility, Target: victim, Data: abilityRef(t, tb, "torrent")}, false)
	for i := 0; i < 10; i++ { // castpoint 5t (0.25s) → channel t6..26; interrupt at t11
		w.Step()
	}
	if got := CastStateName(w.Abilities.CastState[ar][1]); got != "channel" {
		t.Fatalf("setup: state = %s, want channel", got)
	}
	w.IssueOrder(caster, Order{Kind: OrderMove, Point: fixed.Vec2{X: 2000 * fixed.One, Y: 1000 * fixed.One}}, false)
	w.Step()
	for _, l := range *trace {
		t.Logf("%s", l)
	}
	t.Logf("mana=%d ReadyAt[1]=%d", w.Abilities.Mana[ar], w.Abilities.ReadyAt[ar][1])
	if w.Abilities.Mana[ar] != 60*fixed.One {
		t.Fatalf("mana = %d, want 60 — channel cancel keeps the cost", w.Abilities.Mana[ar])
	}
	if w.Abilities.ReadyAt[ar][1] == 0 {
		t.Fatal("cooldown must be running after a channel cancel")
	}
}

// Edge 3: insufficient mana — deterministic order failure.
func TestAbilityInsufficientMana(t *testing.T) {
	w, tb, caster, victim, _ := abilityWorld(t)
	ar := w.Abilities.Row(caster)
	w.Abilities.Mana[ar] = 5 * fixed.One // < 20
	var doneArg int64 = -1
	w.RegisterHandler(1, func(w *World, e Event) {
		if e.Kind == EvOrderDone && e.Src == caster {
			doneArg = e.Arg
		}
	})
	w.Subscribe(EvOrderDone, 1)
	w.IssueOrder(caster, Order{Kind: OrderCastAbility, Target: victim, Data: abilityRef(t, tb, "firebolt")}, false)
	w.Step()
	t.Logf("mana=%d doneArg=%d", w.Abilities.Mana[ar], doneArg)
	if doneArg != 0 {
		t.Fatalf("done arg = %d, want 0 (failed)", doneArg)
	}
	if w.Abilities.Mana[ar] != 5*fixed.One {
		t.Fatal("failed cast must not touch mana")
	}
}

// Edge 4: cooldown exact boundary — recast one tick early fails,
// exactly at ReadyAt succeeds.
func TestAbilityCooldownBoundary(t *testing.T) {
	w, tb, caster, victim, trace := abilityWorld(t)
	ar := w.Abilities.Row(caster)
	ref := abilityRef(t, tb, "firebolt")
	w.IssueOrder(caster, Order{Kind: OrderCastAbility, Target: victim, Data: ref}, false)
	for w.Tick() < 21 { // full first cast: ReadyAt = 38
		w.Step()
	}
	ready := w.Abilities.ReadyAt[ar][0]
	if ready != 38 {
		t.Fatalf("setup: ReadyAt = %d, want 38", ready)
	}
	var lastDone int64 = -1
	w.RegisterHandler(1, func(w *World, e Event) {
		if e.Kind == EvOrderDone && e.Src == caster {
			lastDone = e.Arg
		}
	})
	w.Subscribe(EvOrderDone, 1)
	// recast attempt resolving at tick ready-1 = 37: must fail
	for w.Tick() < ready-2 {
		w.Step()
	}
	lastDone = -1
	w.IssueOrder(caster, Order{Kind: OrderCastAbility, Target: victim, Data: ref}, false)
	w.Step() // tick 37
	earlyFails := 0
	if lastDone == 0 {
		earlyFails = 1
	}
	// recast at exactly ReadyAt = 38: must enter castpoint
	w.IssueOrder(caster, Order{Kind: OrderCastAbility, Target: victim, Data: ref}, false)
	w.Step() // tick 38
	state := CastStateName(w.Abilities.CastState[ar][0])
	t.Logf("recast@%d: fails=%d; recast@%d: state=%s", ready-1, earlyFails, ready, state)
	for _, l := range *trace {
		t.Logf("%s", l)
	}
	if earlyFails != 1 {
		t.Fatalf("recast at ReadyAt-1 must fail deterministically (fails=%d)", earlyFails)
	}
	if state != "castpoint" {
		t.Fatalf("recast at ReadyAt must start: state=%s", state)
	}
}

// Range gate: a mobile caster out of cast range walks in, halts, then
// casts.
func TestAbilityWalkIntoCastRange(t *testing.T) {
	resetEffectExecs()
	t.Cleanup(resetEffectExecs)
	RegisterCoreEffectExecs()
	tb := abilityTables(t)
	w := NewWorld(Caps{})
	w.BindDamageMatrix(tb.Coeff)
	w.BindEffects(tb.Effects)
	w.BindAbilityDefs(tb.Abilities)
	caster := atkUnit(t, w, 0, fixed.Vec2{X: 1000 * fixed.One, Y: 1000 * fixed.One}, 25*fixed.One)
	victim := atkUnit(t, w, 1, fixed.Vec2{X: 1800 * fixed.One, Y: 1000 * fixed.One}, 0) // 800 away, range 500
	w.Abilities.Add(w.Ents, caster)
	ar := w.Abilities.Row(caster)
	w.Abilities.Mana[ar] = 100 * fixed.One
	w.Abilities.MaxMana[ar] = 100 * fixed.One
	w.SetAbility(caster, 0, 0)
	w.IssueOrder(caster, Order{Kind: OrderCastAbility, Target: victim, Data: 1}, false)
	cast := false
	w.OnCastTransition = func(tick uint32, id EntityID, slot int, from, to uint8) {
		if to == CastPoint {
			cast = true
			t.Logf("t%d entered castpoint at distance %d", tick, func() int64 {
				p := w.Transforms.Pos[w.Transforms.Row(caster)]
				q := w.Transforms.Pos[w.Transforms.Row(victim)]
				return q.Sub(p).X.Floor()
			}())
		}
	}
	for i := 0; i < 60 && !cast; i++ {
		w.Step()
	}
	if !cast {
		t.Fatal("caster never walked into cast range")
	}
	if w.Healths.Life[w.Healths.Row(victim)] == 100*fixed.One {
		for i := 0; i < 15; i++ {
			w.Step()
		}
	}
	if w.Healths.Life[w.Healths.Row(victim)] != 70*fixed.One {
		t.Fatalf("victim life = %d, want 70", w.Healths.Life[w.Healths.Row(victim)])
	}
}

// Mana regen accumulates per tick and clamps at max.
func TestAbilityManaRegenClamp(t *testing.T) {
	w, _, caster, _, _ := abilityWorld(t)
	ar := w.Abilities.Row(caster)
	w.Abilities.Mana[ar] = 99 * fixed.One
	w.Abilities.ManaRegen[ar] = fixed.One / 2 // 0.5/tick
	w.Step()
	if w.Abilities.Mana[ar] != 99*fixed.One+fixed.One/2 {
		t.Fatalf("regen tick: mana = %d", w.Abilities.Mana[ar])
	}
	for i := 0; i < 10; i++ {
		w.Step()
	}
	if w.Abilities.Mana[ar] != 100*fixed.One {
		t.Fatalf("mana = %d, want clamped at max 100", w.Abilities.Mana[ar])
	}
}

// R-GC-1: idle ability rows + an active cast allocate nothing.
func TestAbilityCastAllocs(t *testing.T) {
	w, tb, caster, victim, _ := abilityWorld(t)
	w.OnCastTransition = nil
	w.Abilities.ManaRegen[w.Abilities.Row(caster)] = fixed.One
	w.IssueOrder(caster, Order{Kind: OrderCastAbility, Target: victim, Data: abilityRef(t, tb, "firebolt")}, false)
	allocs := testing.AllocsPerRun(300, func() { w.Step() })
	if allocs != 0 {
		t.Fatalf("Step with cast machine allocates %v/run, want 0 (R-GC-1)", allocs)
	}
}

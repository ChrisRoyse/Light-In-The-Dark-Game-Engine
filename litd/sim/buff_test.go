package sim

// Buff state-machine tests (#162). Buff types load through the REAL
// data loader; the FSV SoT is the instance pool, the derived-stat
// cache raw values, and the event/damage traces.

import (
	"fmt"
	"testing"
	"testing/fstest"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
)

// buffTables: poison (40t, stack-count max 3, periodic 5 dmg / 10t,
// dispellable), slow (20t, refresh, move-speed ×0.5), haste (20t,
// independent, move-speed ×1.5), ironskin (40t, refresh, armor +10,
// dispellable), frenzy (20t, refresh, attack-cooldown ×0.5), strong
// (20t, strongest-wins).
func buffTables(t *testing.T) *data.Tables {
	t.Helper()
	fsys := fstest.MapFS{
		"combat/damage-table.toml": &fstest.MapFile{Data: []byte("attack-types = [\"magic\"]\narmor-types = [\"none\"]\n[coefficients]\nmagic = [1000]\n")},
		"buffs/core.toml": &fstest.MapFile{Data: []byte(`
[[buff]]
id = "poison"
duration = 2.0
stacking = "stack-count"
max-stacks = 3
period = 0.5
dispellable = true
[[buff.effects]]
prim = "damage"
amount = 5
attack-type = "magic"

[[buff]]
id = "slow"
duration = 1.0
stacking = "refresh"
[[buff.mod]]
stat = "move-speed"
permille = 500

[[buff]]
id = "haste"
duration = 1.0
stacking = "independent"
[[buff.mod]]
stat = "move-speed"
permille = 1500

[[buff]]
id = "ironskin"
duration = 2.0
stacking = "refresh"
dispellable = true
[[buff.mod]]
stat = "armor"
add = 10

[[buff]]
id = "frenzy"
duration = 1.0
stacking = "refresh"
[[buff.mod]]
stat = "attack-cooldown"
permille = 500

[[buff]]
id = "strong"
duration = 1.0
stacking = "strongest-wins"
`)},
	}
	tb, err := data.Load(fsys)
	if err != nil {
		t.Fatalf("buff tables must load: %v", err)
	}
	return tb
}

// buffTypeIdx resolves an ID to its sorted-table index.
func buffTypeIdx(t *testing.T, tb *data.Tables, id string) int {
	t.Helper()
	for i := range tb.BuffTypes {
		if tb.BuffTypes[i].ID == id {
			return i
		}
	}
	t.Fatalf("buff %q not in table", id)
	return -1
}

// buffWorld: bound world plus carrier (team 0) and enemy (team 1).
func buffWorld(t *testing.T) (*World, *data.Tables, EntityID, EntityID) {
	t.Helper()
	resetEffectExecs()
	t.Cleanup(resetEffectExecs)
	RegisterCoreEffectExecs()
	tb := buffTables(t)
	w := NewWorld(Caps{})
	if err := w.BindDamageMatrix(tb.Coeff); err != nil {
		t.Fatal(err)
	}
	if err := w.BindEffects(tb.Effects); err != nil {
		t.Fatal(err)
	}
	if !w.BindBuffTypes(tb.BuffTypes) {
		t.Fatal("BindBuffTypes failed")
	}
	carrier := atkUnit(t, w, 0, fixed.Vec2{X: 1000 * fixed.One, Y: 1000 * fixed.One}, 0)
	enemy := atkUnit(t, w, 1, fixed.Vec2{X: 1080 * fixed.One, Y: 1000 * fixed.One}, 0)
	return w, tb, carrier, enemy
}

// dumpInstances renders the target's live rows (pool-index order) —
// the FSV instance table.
func dumpInstances(w *World, target EntityID) []string {
	var out []string
	p := w.Buffs
	for i := int32(0); int(i) < p.Cap(); i++ {
		if p.live[i] && p.rows[i].Target == target {
			r := &p.rows[i]
			out = append(out, fmt.Sprintf("slot%d type=%d stacks=%d remain=%d clock=%d",
				i, r.BuffID, r.Stacks, r.RemainingTicks, r.PeriodicClock))
		}
	}
	return out
}

// Edge: all four stacking rules resolve reapplication correctly —
// refresh resets duration on ONE instance, stack-count increments to
// the cap, independent multiplies instances, strongest-wins keeps the
// larger stack count.
func TestBuffStackingRules(t *testing.T) {
	w, tb, carrier, _ := buffWorld(t)
	slow := buffTypeIdx(t, tb, "slow")
	poison := buffTypeIdx(t, tb, "poison")
	haste := buffTypeIdx(t, tb, "haste")
	strong := buffTypeIdx(t, tb, "strong")

	// refresh: duration snaps back to 20 on ONE instance
	w.ApplyBuff(carrier, carrier, slow, 1)
	for i := 0; i < 5; i++ {
		w.Step()
	}
	before := dumpInstances(w, carrier)
	w.ApplyBuff(carrier, carrier, slow, 1)
	after := dumpInstances(w, carrier)
	t.Logf("refresh: before=%v after=%v", before, after)
	if len(after) != 1 {
		t.Fatalf("refresh duplicated the instance: %v", after)
	}
	row := w.Buffs.Row(0)
	if row.RemainingTicks != 20 || row.Stacks != 1 {
		t.Errorf("refresh: remain=%d stacks=%d, want 20/1", row.RemainingTicks, row.Stacks)
	}

	// stack-count: 1 → 3 → capped at 3, single instance
	w.ApplyBuff(carrier, carrier, poison, 1)
	w.ApplyBuff(carrier, carrier, poison, 2)
	w.ApplyBuff(carrier, carrier, poison, 2) // would be 5: cap 3
	t.Logf("stack-count: %v", dumpInstances(w, carrier))
	found := 0
	for i := int32(0); int(i) < w.Buffs.Cap(); i++ {
		if w.Buffs.live[i] && w.Buffs.rows[i].BuffID == uint16(poison) {
			found++
			if got := w.Buffs.rows[i].Stacks; got != 3 {
				t.Errorf("stack-count stacks = %d, want 3 (capped)", got)
			}
		}
	}
	if found != 1 {
		t.Errorf("stack-count instances = %d, want 1", found)
	}

	// independent: every application its own instance
	w.ApplyBuff(carrier, carrier, haste, 1)
	w.ApplyBuff(carrier, carrier, haste, 1)
	found = 0
	for i := int32(0); int(i) < w.Buffs.Cap(); i++ {
		if w.Buffs.live[i] && w.Buffs.rows[i].BuffID == uint16(haste) {
			found++
		}
	}
	t.Logf("independent: %v", dumpInstances(w, carrier))
	if found != 2 {
		t.Errorf("independent instances = %d, want 2", found)
	}

	// strongest-wins: 2 stays over 1, 3 replaces 2
	w.ApplyBuff(carrier, carrier, strong, 2)
	w.ApplyBuff(carrier, carrier, strong, 1)
	var st *BuffInstance
	for i := int32(0); int(i) < w.Buffs.Cap(); i++ {
		if w.Buffs.live[i] && w.Buffs.rows[i].BuffID == uint16(strong) {
			st = &w.Buffs.rows[i]
		}
	}
	if st == nil || st.Stacks != 2 {
		t.Fatalf("strongest-wins after weaker: %+v, want stacks 2", st)
	}
	w.ApplyBuff(carrier, carrier, strong, 3)
	if st.Stacks != 3 {
		t.Errorf("strongest-wins after stronger: stacks=%d, want 3", st.Stacks)
	}
	t.Logf("strongest-wins: %v", dumpInstances(w, carrier))
}

// Edge: two multiplicative modifiers fold to the SAME derived value
// regardless of application order — the cache folds in canonical
// (BuffID, pool index) order, not application order.
func TestBuffFoldOrderIndependence(t *testing.T) {
	w, tb, a, b := buffWorld(t)
	slow := buffTypeIdx(t, tb, "slow")
	haste := buffTypeIdx(t, tb, "haste")
	base := 2 * fixed.One

	w.ApplyBuff(a, a, slow, 1)
	w.ApplyBuff(a, a, haste, 1)
	w.ApplyBuff(b, b, haste, 1)
	w.ApplyBuff(b, b, slow, 1)
	va := w.BuffedMoveSpeed(a, base)
	vb := w.BuffedMoveSpeed(b, base)
	t.Logf("base=%d slow→haste=%d haste→slow=%d", int64(base), int64(va), int64(vb))
	if va != vb {
		t.Fatalf("fold order leaked: %d != %d", int64(va), int64(vb))
	}
	// ×0.5 then ×1.5 of 2.0 = 1.5 exactly in fixed point
	if want := fixed.One + fixed.One/2; va != want {
		t.Errorf("derived speed = %d, want %d", int64(va), int64(want))
	}
}

// Edge: periodic interval 10t applied at tick 7 fires at 7, 17, 27 —
// the phase is fixed at application, and the application tick itself
// fires (the periodic pass runs after scripts/abilities, before the
// damage apply pass).
func TestBuffPeriodicPhase(t *testing.T) {
	w, tb, _, enemy := buffWorld(t)
	poison := buffTypeIdx(t, tb, "poison")

	var fireTicks []uint32
	w.RegisterHandler(1, func(w *World, e Event) {
		if e.Kind == EvUnitDamaged && e.Dst == enemy {
			fireTicks = append(fireTicks, w.Tick())
		}
	})
	w.Subscribe(EvUnitDamaged, 1)
	w.OnScriptPhase = func(tick uint32) {
		if tick == 7 {
			w.ApplyBuff(enemy, enemy, poison, 1)
		}
	}
	hr := w.Healths.Row(enemy)
	t.Logf("BEFORE: life=%d", w.Healths.Life[hr].Floor())
	for w.Tick() < 30 {
		w.Step()
	}
	t.Logf("AFTER: life=%d fireTicks=%v instances=%v",
		w.Healths.Life[hr].Floor(), fireTicks, dumpInstances(w, enemy))
	want := []uint32{7, 17, 27}
	if len(fireTicks) != len(want) {
		t.Fatalf("fired at %v, want %v", fireTicks, want)
	}
	for i := range want {
		if fireTicks[i] != want[i] {
			t.Fatalf("fired at %v, want %v", fireTicks, want)
		}
	}
	if got := w.Healths.Life[hr].Floor(); got != 85 {
		t.Errorf("life = %d, want 100 − 3×5 = 85", got)
	}
}

// Edge: dispel on the buff's natural expiry tick — ONE EvBuffExpired,
// one free, no double-free assert.
func TestBuffExpiryDispelSameTick(t *testing.T) {
	w, tb, carrier, _ := buffWorld(t)
	iron := buffTypeIdx(t, tb, "ironskin") // 40t, dispellable
	var asserts []string
	w.Buffs.DebugAssert = func(msg string) { asserts = append(asserts, msg) }
	expired := 0
	w.RegisterHandler(1, func(w *World, e Event) {
		if e.Kind == EvBuffExpired {
			expired++
		}
	})
	w.Subscribe(EvBuffExpired, 1)
	w.OnScriptPhase = func(tick uint32) {
		if tick == 1 {
			w.ApplyBuff(carrier, carrier, iron, 1)
		}
		if tick == 40 { // natural expiry lands in THIS tick's sweep
			w.Dispel(carrier)
		}
	}
	for w.Tick() < 42 { // +1 tick: phase-7 events dispatch next tick
		w.Step()
	}
	t.Logf("expired events=%d, pool live=%d, asserts=%v", expired, w.Buffs.Live(), asserts)
	if expired != 1 {
		t.Errorf("EvBuffExpired fired %d times, want exactly 1", expired)
	}
	if w.Buffs.Live() != 0 {
		t.Errorf("pool live = %d, want 0", w.Buffs.Live())
	}
	if len(asserts) != 0 {
		t.Errorf("pool asserts fired: %v", asserts)
	}
}

// Derived stats reach their consumers: a slowed unit covers half the
// ground; +10 armor mitigates through the LUT; a frenzied weapon's
// cooldown halves. Expiry restores every base value bit-exactly.
func TestBuffStatConsumers(t *testing.T) {
	w, tb, carrier, enemy := buffWorld(t)
	slow := buffTypeIdx(t, tb, "slow")
	iron := buffTypeIdx(t, tb, "ironskin")
	frenzy := buffTypeIdx(t, tb, "frenzy")

	// move-speed: base 2 wu/tick, slowed ×0.5 → 1 wu/tick of progress
	if !w.Movements.Add(w.Ents, w.Transforms, carrier, 2*fixed.One, 65535) {
		t.Fatal("movement add")
	}
	w.ApplyBuff(carrier, carrier, slow, 1)
	tr := w.Transforms.Row(carrier)
	startX := w.Transforms.Pos[tr].X
	w.StartMoveTo(carrier, fixed.Vec2{X: startX + 100*fixed.One, Y: w.Transforms.Pos[tr].Y})
	w.Step()
	delta := w.Transforms.Pos[tr].X.Sub(startX)
	t.Logf("slowed step: delta=%d (1 wu = %d)", int64(delta), int64(fixed.One))
	if delta != fixed.One {
		t.Errorf("slowed delta = %d, want exactly %d (×0.5 of 2)", int64(delta), int64(fixed.One))
	}

	// armor: 40 magic vs armor 0 = 40; vs buffed +10 armor = 40/1.6 = 25
	w.ApplyBuff(enemy, enemy, iron, 1)
	if got := w.BuffedArmor(enemy, 0); got != 10 {
		t.Fatalf("BuffedArmor = %d, want 10", got)
	}
	hr := w.Healths.Row(enemy)
	before := w.Healths.Life[hr]
	w.QueueDamage(DamagePacket{Source: carrier, Target: enemy, Amount: 40 * fixed.One})
	w.Step()
	taken := before.Sub(w.Healths.Life[hr])
	t.Logf("armor: life %d→%d, taken=%d", before.Floor(), w.Healths.Life[hr].Floor(), taken.Floor())
	if taken.Floor() != 25 {
		t.Errorf("mitigated damage = %d, want 25 (LUT at armor 10)", taken.Floor())
	}

	// attack cooldown: base 27t, frenzy ×0.5 → 13t
	w.ApplyBuff(carrier, carrier, frenzy, 1)
	if got := w.BuffedCooldown(carrier, 27); got != 13 {
		t.Errorf("BuffedCooldown = %d, want floor(27×0.5)=13", got)
	}

	// expiry restores identity: run out every duration
	for w.Tick() < 60 {
		w.Step()
	}
	if v := w.BuffedMoveSpeed(carrier, 2*fixed.One); v != 2*fixed.One {
		t.Errorf("post-expiry speed = %d, want base", int64(v))
	}
	if v := w.BuffedArmor(enemy, 0); v != 0 {
		t.Errorf("post-expiry armor = %d, want 0", v)
	}
	if v := w.BuffedCooldown(carrier, 27); v != 27 {
		t.Errorf("post-expiry cooldown = %d, want 27", v)
	}
	t.Logf("post-expiry identity confirmed; pool live=%d", w.Buffs.Live())
}

// R-GC-1: a tick with live periodic buffs, expiring instances, and
// cache recomputes allocates nothing.
func TestBuffTickAllocs(t *testing.T) {
	w, tb, carrier, enemy := buffWorld(t)
	poison := buffTypeIdx(t, tb, "poison")
	slow := buffTypeIdx(t, tb, "slow")
	w.OnScriptPhase = func(tick uint32) {
		// constant churn: reapply every 5 ticks so expiry/recompute paths run
		if tick%5 == 0 {
			w.ApplyBuff(enemy, carrier, poison, 1)
			w.ApplyBuff(carrier, carrier, slow, 1)
		}
	}
	w.Step() // warm-up
	avg := testing.AllocsPerRun(200, func() { w.Step() })
	if avg != 0 {
		t.Fatalf("allocs/tick = %v, want 0 (R-GC-1)", avg)
	}
	t.Logf("allocs/tick = %v over 200 ticks, pool live=%d", avg, w.Buffs.Live())
}

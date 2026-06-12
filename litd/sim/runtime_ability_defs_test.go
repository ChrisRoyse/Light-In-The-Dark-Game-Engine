package sim

import (
	"bytes"
	"fmt"
	"strings"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/statehash"
)

func TestRegisterAbilityDefCastUsesRuntimeFields(t *testing.T) {
	tb := runtimeAbilitySetup(t)
	w := runtimeAbilityWorld(t, tb, Caps{})
	caster, victim := runtimeAbilityUnits(t, w)
	def := runtimeBoltDef(t, tb, "runtime-bolt")
	before := runtimeAbilityDefsDump(w)
	ref, ok := w.RegisterAbilityDef(def)
	if !ok {
		t.Fatal("RegisterAbilityDef failed")
	}
	if !w.SetAbilityRef(caster, 2, ref) {
		t.Fatal("SetAbilityRef failed")
	}
	ar := w.Abilities.Row(caster)
	beforeMana := w.Abilities.Mana[ar]
	beforeReady := w.Abilities.ReadyAt[ar][2]
	beforeLife := w.Healths.Life[w.Healths.Row(victim)]
	w.IssueOrder(caster, Order{Kind: OrderCastAbility, Target: victim, Data: ref}, false)
	w.Step()
	afterMana := w.Abilities.Mana[ar]
	afterReady := w.Abilities.ReadyAt[ar][2]
	afterLife := w.Healths.Life[w.Healths.Row(victim)]
	t.Logf("FSV runtime ability register BEFORE defs=%s", before)
	t.Logf("FSV runtime ability register AFTER defs=%s ref=%d", runtimeAbilityDefsDump(w), ref)
	t.Logf("FSV runtime ability cast: mana %d->%d ReadyAt %d->%d victimLife %d->%d tick=%d",
		beforeMana, afterMana, beforeReady, afterReady, beforeLife, afterLife, w.Tick())
	if ref != uint16(len(tb.Abilities)+1) {
		t.Fatalf("ref=%d, want static len + 1 = %d", ref, len(tb.Abilities)+1)
	}
	if afterMana != beforeMana-fixed.FromInt(def.ManaCost) {
		t.Fatalf("mana after cast=%d, want %d", afterMana, beforeMana-fixed.FromInt(def.ManaCost))
	}
	if afterReady != w.Tick()+uint32(def.CooldownTicks) {
		t.Fatalf("ReadyAt=%d, want effect tick %d + cooldown %d", afterReady, w.Tick(), def.CooldownTicks)
	}
	if afterLife != beforeLife-30*fixed.One {
		t.Fatalf("victim life=%d, want %d", afterLife, beforeLife-30*fixed.One)
	}
}

func TestRegisterAbilityDefInvalidRejected(t *testing.T) {
	tb := runtimeAbilitySetup(t)
	w := runtimeAbilityWorld(t, tb, Caps{})
	valid := runtimeBoltDef(t, tb, "runtime-valid")
	ref, ok := w.RegisterAbilityDef(valid)
	if !ok || ref == 0 {
		t.Fatal("valid setup registration failed")
	}
	before := runtimeAbilityDefsDump(w)
	invalid := []data.Ability{
		{ID: "negative-cost", ManaCost: -1},
		{ID: "zero-range-targeted", Effects: runtimeEffectList(t, tb, "firebolt")},
		{ID: "unknown-effect", Effects: data.EffectList{Off: 999, Len: 1}, CastRange: fixed.FromInt(500)},
		{ID: "runtime-valid"},
	}
	for _, def := range invalid {
		gotRef, gotOK := w.RegisterAbilityDef(def)
		t.Logf("FSV invalid runtime ability def id=%q ok=%v ref=%d before=%s after=%s",
			def.ID, gotOK, gotRef, before, runtimeAbilityDefsDump(w))
		if gotOK || gotRef != 0 {
			t.Fatalf("invalid def %q accepted with ref %d", def.ID, gotRef)
		}
		if after := runtimeAbilityDefsDump(w); after != before {
			t.Fatalf("invalid def %q mutated table:\nbefore %s\nafter  %s", def.ID, before, after)
		}
	}
}

func TestHashRuntimeAbilityDefOrder(t *testing.T) {
	tb := runtimeAbilitySetup(t)
	a := runtimeAbilityWorld(t, tb, Caps{})
	b := runtimeAbilityWorld(t, tb, Caps{})
	c := runtimeAbilityWorld(t, tb, Caps{})
	defA := data.Ability{ID: "order-a", ManaCost: 1, CooldownTicks: 2}
	defB := data.Ability{ID: "order-b", ManaCost: 3, CooldownTicks: 4}
	if _, ok := a.RegisterAbilityDef(defA); !ok {
		t.Fatal("a defA failed")
	}
	if _, ok := a.RegisterAbilityDef(defB); !ok {
		t.Fatal("a defB failed")
	}
	if _, ok := b.RegisterAbilityDef(defA); !ok {
		t.Fatal("b defA failed")
	}
	if _, ok := b.RegisterAbilityDef(defB); !ok {
		t.Fatal("b defB failed")
	}
	if _, ok := c.RegisterAbilityDef(defB); !ok {
		t.Fatal("c defB failed")
	}
	if _, ok := c.RegisterAbilityDef(defA); !ok {
		t.Fatal("c defA failed")
	}
	sa, sb, sc := runtimeAbilityHash(t, a), runtimeAbilityHash(t, b), runtimeAbilityHash(t, c)
	idx := hashSystemIndex(t, "abilitydefs")
	t.Logf("FSV runtime ability hash same order A: defs=%s top=%016x abilitydefs=%016x", runtimeAbilityDefsDump(a), sa.Top, sa.Subs[idx])
	t.Logf("FSV runtime ability hash same order B: defs=%s top=%016x abilitydefs=%016x", runtimeAbilityDefsDump(b), sb.Top, sb.Subs[idx])
	t.Logf("FSV runtime ability hash different order C: defs=%s top=%016x abilitydefs=%016x", runtimeAbilityDefsDump(c), sc.Top, sc.Subs[idx])
	if sa.Top != sb.Top || sa.Subs[idx] != sb.Subs[idx] {
		t.Fatal("identical runtime ability registration order diverged")
	}
	if sa.Subs[idx] == sc.Subs[idx] {
		t.Fatal("different runtime ability registration order did not diverge in abilitydefs")
	}
}

func TestRegisterAbilityDefCapExhaustion(t *testing.T) {
	tb := runtimeAbilitySetup(t)
	w := runtimeAbilityWorld(t, tb, Caps{RuntimeAbilityDefs: 1})
	before := runtimeAbilityDefsDump(w)
	ref1, ok1 := w.RegisterAbilityDef(data.Ability{ID: "cap-one", ManaCost: 1})
	mid := runtimeAbilityDefsDump(w)
	ref2, ok2 := w.RegisterAbilityDef(data.Ability{ID: "cap-two", ManaCost: 2})
	after := runtimeAbilityDefsDump(w)
	t.Logf("FSV runtime ability cap BEFORE: %s", before)
	t.Logf("FSV runtime ability cap FIRST: ok=%v ref=%d %s", ok1, ref1, mid)
	t.Logf("FSV runtime ability cap SECOND: ok=%v ref=%d %s", ok2, ref2, after)
	if !ok1 || ref1 == 0 {
		t.Fatal("first runtime def in cap-1 world failed")
	}
	if ok2 || ref2 != 0 {
		t.Fatalf("cap exhaustion accepted second def ok=%v ref=%d", ok2, ref2)
	}
	if after != mid {
		t.Fatalf("cap exhaustion mutated table:\nmid   %s\nafter %s", mid, after)
	}
}

func TestRegisterAbilityDefDuringStepRejected(t *testing.T) {
	tb := runtimeAbilitySetup(t)
	w := runtimeAbilityWorld(t, tb, Caps{})
	before := runtimeAbilityDefsDump(w)
	var ref uint16
	var ok bool
	w.OnCombatPhase = func(tick uint32) {
		ref, ok = w.RegisterAbilityDef(data.Ability{ID: "mid-step", ManaCost: 1})
	}
	w.Step()
	after := runtimeAbilityDefsDump(w)
	t.Logf("FSV runtime ability mid-step BEFORE: %s", before)
	t.Logf("FSV runtime ability mid-step AFTER: ok=%v ref=%d %s", ok, ref, after)
	if ok || ref != 0 {
		t.Fatalf("mid-step RegisterAbilityDef accepted ok=%v ref=%d", ok, ref)
	}
	if after != before {
		t.Fatalf("mid-step RegisterAbilityDef mutated table:\nbefore %s\nafter  %s", before, after)
	}
}

func TestRuntimeAbilityDefSaveLoadRoundTripAndCast(t *testing.T) {
	tb := runtimeAbilitySetup(t)
	src := runtimeAbilityWorld(t, tb, Caps{})
	caster, victim := runtimeAbilityUnits(t, src)
	def := runtimeBoltDef(t, tb, "runtime-save-bolt")
	ref, ok := src.RegisterAbilityDef(def)
	if !ok {
		t.Fatal("RegisterAbilityDef failed")
	}
	if !src.SetAbilityRef(caster, 2, ref) {
		t.Fatal("SetAbilityRef failed")
	}
	beforeRows := runtimeAbilityDefsDump(src)
	before := runtimeAbilityHash(t, src)
	var buf bytes.Buffer
	if err := src.SaveState(&buf, tb.Fingerprint); err != nil {
		t.Fatal(err)
	}
	saved := append([]byte(nil), buf.Bytes()...)
	off := abilityDefSectionOffsetForTest(src, saved)
	sectionLen := abilityDefSectionLenForTest(src)

	dst := runtimeAbilityWorld(t, tb, Caps{})
	if err := dst.LoadState(bytes.NewReader(saved), tb.Fingerprint); err != nil {
		t.Fatal(err)
	}
	afterRows := runtimeAbilityDefsDump(dst)
	after := runtimeAbilityHash(t, dst)
	idx := hashSystemIndex(t, "abilitydefs")
	var buf2 bytes.Buffer
	if err := dst.SaveState(&buf2, tb.Fingerprint); err != nil {
		t.Fatal(err)
	}
	if !bytes.Equal(saved, buf2.Bytes()) {
		t.Fatal("runtime ability re-save is not byte-identical")
	}
	ar := dst.Abilities.Row(caster)
	beforeMana := dst.Abilities.Mana[ar]
	beforeReady := dst.Abilities.ReadyAt[ar][2]
	beforeLife := dst.Healths.Life[dst.Healths.Row(victim)]
	dst.IssueOrder(caster, Order{Kind: OrderCastAbility, Target: victim, Data: ref}, false)
	dst.Step()
	afterMana := dst.Abilities.Mana[ar]
	afterReady := dst.Abilities.ReadyAt[ar][2]
	afterLife := dst.Healths.Life[dst.Healths.Row(victim)]
	t.Logf("FSV runtime ability save SOURCE: defs=%s top=%016x abilitydefs=%016x", beforeRows, before.Top, before.Subs[idx])
	t.Logf("FSV runtime ability save section offset=%d len=%d hex=% x", off, sectionLen, saved[off:off+sectionLen])
	t.Logf("FSV runtime ability save LOADED: defs=%s top=%016x abilitydefs=%016x", afterRows, after.Top, after.Subs[idx])
	t.Logf("FSV runtime ability post-load cast: mana %d->%d ReadyAt %d->%d victimLife %d->%d tick=%d",
		beforeMana, afterMana, beforeReady, afterReady, beforeLife, afterLife, dst.Tick())
	if before.Top != after.Top || beforeRows != afterRows {
		t.Fatalf("runtime ability save/load changed state: diff=%v\nbefore %s\nafter  %s", snapDiff(t, &before, &after), beforeRows, afterRows)
	}
	if afterMana != beforeMana-fixed.FromInt(def.ManaCost) || afterReady != dst.Tick()+uint32(def.CooldownTicks) ||
		afterLife != beforeLife-30*fixed.One {
		t.Fatal("post-load runtime ability cast did not use registered mana/cooldown/effect")
	}
}

func runtimeAbilitySetup(t *testing.T) *data.Tables {
	t.Helper()
	resetEffectExecs()
	t.Cleanup(resetEffectExecs)
	RegisterCoreEffectExecs()
	return abilityTables(t)
}

func runtimeAbilityWorld(t *testing.T, tb *data.Tables, caps Caps) *World {
	t.Helper()
	w := NewWorld(caps)
	if err := w.BindDamageMatrix(tb.Coeff); err != nil {
		t.Fatal(err)
	}
	if err := w.BindEffects(tb.Effects); err != nil {
		t.Fatal(err)
	}
	if !w.BindAbilityDefs(tb.Abilities) {
		t.Fatal("BindAbilityDefs failed")
	}
	return w
}

func runtimeAbilityUnits(t *testing.T, w *World) (EntityID, EntityID) {
	t.Helper()
	caster := atkUnit(t, w, 0, fixed.Vec2{X: 1000 * fixed.One, Y: 1000 * fixed.One}, 0)
	victim := atkUnit(t, w, 1, fixed.Vec2{X: 1100 * fixed.One, Y: 1000 * fixed.One}, 0)
	if !w.Abilities.Add(w.Ents, caster) {
		t.Fatal("ability row add failed")
	}
	ar := w.Abilities.Row(caster)
	w.Abilities.Mana[ar] = 100 * fixed.One
	w.Abilities.MaxMana[ar] = 100 * fixed.One
	return caster, victim
}

func runtimeBoltDef(t *testing.T, tb *data.Tables, id string) data.Ability {
	t.Helper()
	return data.Ability{
		ID:             id,
		Name:           "Runtime Bolt",
		Effects:        runtimeEffectList(t, tb, "firebolt"),
		ManaCost:       7,
		CooldownTicks:  13,
		CastPointTicks: 0,
		BackswingTicks: 0,
		CastRange:      fixed.FromInt(500),
	}
}

func runtimeEffectList(t *testing.T, tb *data.Tables, id string) data.EffectList {
	t.Helper()
	for i := range tb.Abilities {
		if tb.Abilities[i].ID == id {
			return tb.Abilities[i].Effects
		}
	}
	t.Fatalf("ability %q not found", id)
	return data.EffectList{}
}

func runtimeAbilityHash(t *testing.T, w *World) statehash.Snapshot {
	t.Helper()
	reg := NewHashRegistry()
	var snap statehash.Snapshot
	w.HashState(reg, &snap)
	return snap
}

func runtimeAbilityDefsDump(w *World) string {
	var b strings.Builder
	fmt.Fprintf(&b, "static=%d runtime=%d total=%d rows=[", len(w.abilityDefs), len(w.runtimeAbilityDefs), w.abilityDefCount())
	for i := range w.runtimeAbilityDefs {
		if i > 0 {
			b.WriteByte(' ')
		}
		def := &w.runtimeAbilityDefs[i]
		fmt.Fprintf(&b, "ref=%d id=%s mana=%d cd=%d cp=%d bs=%d ch=%d range=%d effects={%d,%d}",
			len(w.abilityDefs)+i+1, def.ID, def.ManaCost, def.CooldownTicks, def.CastPointTicks,
			def.BackswingTicks, def.ChannelTicks, int64(def.CastRange), def.Effects.Off, def.Effects.Len)
	}
	b.WriteByte(']')
	return b.String()
}

package sim

// FSV for #478: an ability may bind a trigger (event = its own cast) instead of
// a static effect list. SoT = the victim's HP after a real cast + Game state
// hash. The headline check authors the SAME 30-damage spell two ways and asserts
// identical HP delta AND identical top hash.

import (
	"bytes"
	"testing"
	"testing/fstest"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/statehash"
)

const damageTableTOML = "attack-types = [\"magic\"]\narmor-types = [\"none\"]\n[coefficients]\nmagic = [1000]\n"

// firebolt authored as a static effect list.
const fireboltEffectsTOML = `
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
`

// firebolt authored as a bound trigger (no effects).
const fireboltTriggerTOML = `
[[ability]]
id = "firebolt"
name = "Firebolt"
mana-cost = 20
cooldown = 1.35
cast-point = 0.5
backswing = 0.5
cast-range = 500
trigger = "spell"
`

func loadAbilityTables(t *testing.T, abilityTOML string) *data.Tables {
	t.Helper()
	tb, err := data.Load(fstest.MapFS{
		"combat/damage-table.toml": &fstest.MapFile{Data: []byte(damageTableTOML)},
		"abilities/core.toml":      &fstest.MapFile{Data: []byte(abilityTOML)},
	})
	if err != nil {
		t.Fatalf("tables must load: %v", err)
	}
	return tb
}

// trigSpellWorld builds a caster (firebolt equipped) + victim, AND — in both the
// effect-list and bound-trigger variants — creates the identical "spell" trigger
// (a damage-30 magic Action) and binds the name. Only the ability's authoring
// path differs, so the two worlds are structurally identical for everything the
// state hash covers (static ability defs are fingerprinted data, not hashed).
func trigSpellWorld(t *testing.T, abilityTOML string) (*World, EntityID, EntityID, TriggerID, uint16) {
	t.Helper()
	resetEffectExecs()
	t.Cleanup(resetEffectExecs)
	RegisterCoreEffectExecs()
	tb := loadAbilityTables(t, abilityTOML)
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
	if !w.SetAbility(caster, 0, 0) {
		t.Fatal("SetAbility failed")
	}
	// the bound trigger — identical in both variants.
	ref, err := w.RegisterEffectAction("spell.dmg", EffectActionSpec{
		Prim: data.EPDamage, Params: []EffectActionParam{{"amount", 30}, {"attack-type", 0}},
	})
	if err != nil {
		t.Fatalf("register damage Action: %v", err)
	}
	tr, _ := w.Triggers.New()
	w.Triggers.AddAction(tr, ref)
	if !w.BindTriggerName("spell", tr) {
		t.Fatal("BindTriggerName failed")
	}
	return w, caster, victim, tr, abilityRef(t, tb, "firebolt")
}

func castAndSettle(w *World, caster, victim EntityID, ref uint16) {
	w.IssueOrder(caster, Order{Kind: OrderCastAbility, Target: victim, Data: ref}, false)
	for i := 0; i < 25; i++ {
		w.Step()
	}
}

// TestAbilityTriggerVsDataParityFSV — the headline: the same 30-damage spell
// authored as an effect list and as a bound trigger delivers identical HP AND
// hashes identically.
func TestAbilityTriggerVsDataParityFSV(t *testing.T) {
	reg := NewHashRegistry()

	wD, cD, vD, _, refD := trigSpellWorld(t, fireboltEffectsTOML)
	castAndSettle(wD, cD, vD, refD)
	var sD statehash.Snapshot
	hashD := wD.HashState(reg, &sD).Top
	hpD := victimLife(wD, vD)

	wT, cT, vT, _, refT := trigSpellWorld(t, fireboltTriggerTOML)
	castAndSettle(wT, cT, vT, refT)
	var sT statehash.Snapshot
	hashT := wT.HashState(reg, &sT).Top
	hpT := victimLife(wT, vT)

	t.Logf("FSV #478 parity: data-path victim HP=%d hash=%#016x | trigger-path HP=%d hash=%#016x", hpD, hashD, hpT, hashT)
	if hpD != 70 || hpT != 70 {
		t.Fatalf("HP delta not equal/correct: data=%d trigger=%d (want both 70)", hpD, hpT)
	}
	if hashD != hashT {
		for i := range sD.Subs {
			if sD.Subs[i] != sT.Subs[i] {
				t.Logf("  sub[%d] %q differs: data=%#x trigger=%#x", i, hashSystemName(i), sD.Subs[i], sT.Subs[i])
			}
		}
		t.Fatalf("authoring paths hash differently: data=%#x trigger=%#x", hashD, hashT)
	}
}

// hashSystemName is a tiny lookup for the divergence dump above.
func hashSystemName(i int) string {
	if i >= 0 && i < len(HashSystems) {
		return HashSystems[i]
	}
	return "?"
}

// TestAbilityBothPathsLoadFailsFSV — a row that declares both effects and a
// bound trigger fails to load (loud, fail-closed).
func TestAbilityBothPathsLoadFailsFSV(t *testing.T) {
	_, err := data.Load(fstest.MapFS{
		"combat/damage-table.toml": &fstest.MapFile{Data: []byte(damageTableTOML)},
		"abilities/core.toml": &fstest.MapFile{Data: []byte(`
[[ability]]
id = "firebolt"
name = "Firebolt"
cast-range = 500
trigger = "spell"
[[ability.effects]]
prim = "damage"
amount = 30
attack-type = "magic"
`)},
	})
	t.Logf("FSV #478 both-paths load error: %v", err)
	if err == nil {
		t.Fatal("ability with both effects AND a trigger loaded — must fail closed")
	}
}

// TestAbilityTriggerDisabledNoopFSV — a disabled bound trigger makes the cast a
// no-op effect (documented).
func TestAbilityTriggerDisabledNoopFSV(t *testing.T) {
	w, caster, victim, tr, ref := trigSpellWorld(t, fireboltTriggerTOML)
	w.Triggers.SetEnabled(tr, false)
	castAndSettle(w, caster, victim, ref)
	hp := victimLife(w, victim)
	t.Logf("FSV #478 disabled trigger: victim HP=%d (want 100, cast was a no-op)", hp)
	if hp != 100 {
		t.Fatalf("disabled-trigger cast dealt damage: victim HP=%d, want 100", hp)
	}
}

// TestAbilityTriggerConditionFailsNoopFSV — a bound trigger whose condition is
// false runs no actions: the cast deals nothing.
func TestAbilityTriggerConditionFailsNoopFSV(t *testing.T) {
	w, caster, victim, tr, ref := trigSpellWorld(t, fireboltTriggerTOML)
	condFalse := w.RegisterHandlerID("cond.false", func(*World, Event) bool { return false })
	w.Triggers.SetCondition(tr, w.Cond(condFalse))
	castAndSettle(w, caster, victim, ref)
	hp := victimLife(w, victim)
	t.Logf("FSV #478 false condition: victim HP=%d (want 100, actions gated off)", hp)
	if hp != 100 {
		t.Fatalf("false-condition trigger dealt damage: victim HP=%d, want 100", hp)
	}
}

// TestAbilityTriggerBindingSaveLoadFSV — the name→trigger binding round-trips:
// a re-built world loads the save and reproduces the binding + state hash, and
// the bound spell still delivers its damage post-load.
func TestAbilityTriggerBindingSaveLoadFSV(t *testing.T) {
	reg := NewHashRegistry()
	src, _, _, _, _ := trigSpellWorld(t, fireboltTriggerTOML)
	var ss statehash.Snapshot
	srcHash := src.HashState(reg, &ss).Top

	var buf bytes.Buffer
	if err := src.SaveState(&buf, 0); err != nil {
		t.Fatalf("SaveState: %v", err)
	}

	// a structurally-identical world (same trigger + handler re-registered in
	// setup) loads the save; the binding is restored from the bytes.
	dst, cD, vD, _, refD := trigSpellWorld(t, fireboltTriggerTOML)
	if err := dst.LoadState(bytes.NewReader(buf.Bytes()), 0); err != nil {
		t.Fatalf("LoadState: %v", err)
	}
	var ds statehash.Snapshot
	dstHash := dst.HashState(reg, &ds).Top
	tid, ok := dst.TriggerByName("spell")
	t.Logf("FSV #478 binding save/load: srcHash=%#016x dstHash=%#016x, names=%d resolve(spell)=%v/%d", srcHash, dstHash, dst.NamedTriggerCount(), ok, tid)
	if dstHash != srcHash {
		t.Fatalf("post-load hash %#x != pre-save %#x", dstHash, srcHash)
	}
	if !ok || dst.NamedTriggerCount() != 1 {
		t.Fatalf("binding not restored: count=%d resolve ok=%v", dst.NamedTriggerCount(), ok)
	}
	// the restored binding still drives a real cast.
	castAndSettle(dst, cD, vD, refD)
	if hp := victimLife(dst, vD); hp != 70 {
		t.Fatalf("post-load bound cast dealt wrong damage: victim HP=%d, want 70", hp)
	}
}

// TestAbilityTriggerDeterminismFSV — the trigger path is deterministic: two runs
// produce the identical state hash.
func TestAbilityTriggerDeterminismFSV(t *testing.T) {
	reg := NewHashRegistry()
	run := func() uint64 {
		w, c, v, _, ref := trigSpellWorld(t, fireboltTriggerTOML)
		castAndSettle(w, c, v, ref)
		var s statehash.Snapshot
		return w.HashState(reg, &s).Top
	}
	a, b := run(), run()
	t.Logf("FSV #478 determinism: run1=%#016x run2=%#016x", a, b)
	if a != b {
		t.Fatalf("trigger path nondeterministic: %#x != %#x", a, b)
	}
}

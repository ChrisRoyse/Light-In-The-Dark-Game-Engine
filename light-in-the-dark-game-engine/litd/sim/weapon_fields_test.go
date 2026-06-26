package sim

// FSV for #476: live per-instance weapon overrides. SoT = the weapon-override
// store + the next attack's rolled damage / attack type, plus save/load and the
// state hash.

import (
	"bytes"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/statehash"
)

// weaponWorld: an attacker armed with a deterministic weapon (base 10, no dice,
// attack type 0) and a victim, plus a 2×1 matrix (normal 1000, holy 2000).
func weaponWorld(t *testing.T) (*World, EntityID, EntityID) {
	t.Helper()
	w := NewWorld(Caps{Units: 8})
	if err := w.BindDamageTypes([]string{"normal", "holy"}, []string{"unarmored"}); err != nil {
		t.Fatalf("BindDamageTypes: %v", err)
	}
	if err := w.BindDamageMatrix([][]int32{{1000}, {2000}}); err != nil {
		t.Fatalf("BindDamageMatrix: %v", err)
	}
	mk := func(x int32) EntityID {
		id, ok := w.CreateUnit(fixed.Vec2{X: fixed.FromInt(x), Y: fixed.FromInt(100)}, 0)
		if !ok || !w.Healths.Add(w.Ents, id, 1000*fixed.One, 0, 0, 0) || !w.Combats.Add(w.Ents, id) {
			t.Fatal("spawn failed")
		}
		return id
	}
	attacker, victim := mk(100), mk(140)
	atk := &data.Attack{DamageBase: 10, Dice: 0, Sides: 0, AttackType: 0, CooldownTicks: 30, Range: fixed.FromInt(200)}
	if !w.SetWeapon(attacker, 0, atk, 0, data.EffectList{}) {
		t.Fatal("SetWeapon failed")
	}
	return w, attacker, victim
}

// TestWeaponDamageBaseOverrideFSV — set damage base 10→40; the rolled weapon
// damage reflects 40, and GetUnitWeaponField reports it.
func TestWeaponDamageBaseOverrideFSV(t *testing.T) {
	w, attacker, _ := weaponWorld(t)
	cr := w.Combats.Row(attacker)
	base := w.rollWeapon(cr, 0)
	if !w.SetUnitWeaponField(attacker, 0, WeaponFieldDamageBase, 40) {
		t.Fatal("SetUnitWeaponField(DamageBase,40) returned false")
	}
	got := w.rollWeapon(cr, 0)
	eff, _ := w.GetUnitWeaponField(attacker, 0, WeaponFieldDamageBase)
	t.Logf("FSV #476 base override: rollWeapon %d→%d (=%.0f), Get=%d", base, got, float64(got)/float64(fixed.One), eff)
	if got != 40*fixed.One {
		t.Fatalf("rolled damage = %d, want 40 (override)", got)
	}
	if eff != 40 {
		t.Fatalf("GetUnitWeaponField = %d, want 40", eff)
	}
}

// TestWeaponAttackTypeOverrideFSV — switch the weapon's attack type to holy;
// the next fired packet uses the holy coefficient (2× a normal hit).
func TestWeaponAttackTypeOverrideFSV(t *testing.T) {
	w, attacker, victim := weaponWorld(t)
	holy, _ := w.AttackTypeIndex("holy")
	if !w.SetUnitWeaponField(attacker, 0, WeaponFieldAttackType, int64(holy)) {
		t.Fatal("SetUnitWeaponField(AttackType, holy) returned false")
	}
	hr := w.Healths.Row(victim)
	before := w.Healths.Life[hr]
	cr := w.Combats.Row(attacker)
	w.OnCombatPhase = func(uint32) {
		w.fireWeapon(attacker, victim, cr, 0) // base 10 × holy 2000/1000 = 20
		w.OnCombatPhase = nil
	}
	w.Step()
	delta := before - w.Healths.Life[w.Healths.Row(victim)]
	t.Logf("FSV #476 attack-type holy: base 10 → delta=%d (=%.0f) want=20", delta, float64(delta)/float64(fixed.One))
	if delta != 20*fixed.One {
		t.Fatalf("holy-weapon delta = %d, want 20 (10·2000/1000)", delta)
	}
}

// TestWeaponOverrideFailClosedFSV — invalid slot/field/value rejected.
func TestWeaponOverrideFailClosedFSV(t *testing.T) {
	w, attacker, _ := weaponWorld(t)
	cases := []struct {
		name  string
		slot  int
		field WeaponField
		value int64
	}{
		{"bad slot", 9, WeaponFieldDamageBase, 40},
		{"attack-type out of matrix", 0, WeaponFieldAttackType, 5},
		{"negative base", 0, WeaponFieldDamageBase, -1},
		{"zero cooldown", 0, WeaponFieldCooldown, 0},
		{"empty weapon slot", 1, WeaponFieldDamageBase, 40}, // slot 1 has no weapon
	}
	for _, c := range cases {
		if w.SetUnitWeaponField(attacker, c.slot, c.field, c.value) {
			t.Fatalf("%s: SetUnitWeaponField accepted an invalid override", c.name)
		}
	}
	if w.WeaponOverrides.Count() != 0 {
		t.Fatalf("store has %d rows after all-rejected, want 0", w.WeaponOverrides.Count())
	}
	t.Log("FSV #476 fail-closed: bad slot/type/value/cooldown/empty-slot all rejected, store empty")
}

// TestWeaponOverrideClearRevertsFSV — clearing an override reverts to the data
// default.
func TestWeaponOverrideClearRevertsFSV(t *testing.T) {
	w, attacker, _ := weaponWorld(t)
	cr := w.Combats.Row(attacker)
	w.SetUnitWeaponField(attacker, 0, WeaponFieldDamageBase, 40)
	if w.rollWeapon(cr, 0) != 40*fixed.One {
		t.Fatal("override not applied")
	}
	if !w.ClearUnitWeaponField(attacker, 0, WeaponFieldDamageBase) {
		t.Fatal("ClearUnitWeaponField returned false")
	}
	got := w.rollWeapon(cr, 0)
	eff, _ := w.GetUnitWeaponField(attacker, 0, WeaponFieldDamageBase)
	t.Logf("FSV #476 clear-revert: rollWeapon=%d (=%.0f), Get=%d (data default 10)", got, float64(got)/float64(fixed.One), eff)
	if got != 10*fixed.One || eff != 10 {
		t.Fatalf("after clear: roll=%d get=%d, want 10/10 (reverted)", got, eff)
	}
}

// TestWeaponOverrideHashFSV — an override changes the state hash; clearing it
// restores the original.
func TestWeaponOverrideHashFSV(t *testing.T) {
	reg := NewHashRegistry()
	var s0, s1, s2 statehash.Snapshot
	w, attacker, _ := weaponWorld(t)
	h0 := w.HashState(reg, &s0).Top
	w.SetUnitWeaponField(attacker, 0, WeaponFieldDamageBase, 40)
	h1 := w.HashState(reg, &s1).Top
	w.ClearUnitWeaponField(attacker, 0, WeaponFieldDamageBase)
	h2 := w.HashState(reg, &s2).Top
	t.Logf("FSV #476 hash: base=%#016x override=%#016x cleared=%#016x", h0, h1, h2)
	if h1 == h0 {
		t.Fatal("override did not change the state hash")
	}
	if h2 != h0 {
		t.Fatalf("cleared hash %#x != original %#x — clear must fully revert", h2, h0)
	}
}

// TestWeaponOverrideZeroAllocFSV — the override resolve is zero-alloc on the
// attack hot path, with and without an override present (R-GC-1).
func TestWeaponOverrideZeroAllocFSV(t *testing.T) {
	w, attacker, _ := weaponWorld(t)
	cr := w.Combats.Row(attacker)
	run := func() { _ = w.rollWeapon(cr, 0) }
	run()
	if a := testing.AllocsPerRun(1000, run); a != 0 {
		t.Fatalf("rollWeapon allocs (no override) = %v, want 0", a)
	}
	w.SetUnitWeaponField(attacker, 0, WeaponFieldDamageBase, 40)
	run()
	if a := testing.AllocsPerRun(1000, run); a != 0 {
		t.Fatalf("rollWeapon allocs (with override) = %v, want 0", a)
	}
	t.Log("FSV #476 zero-alloc: rollWeapon resolve 0 allocs/run, base + overridden")
}

// TestWeaponOverrideSaveLoadFSV — a saved override round-trips: the loaded world
// reproduces the re-armed damage and the same hash.
func TestWeaponOverrideSaveLoadFSV(t *testing.T) {
	src, attacker, _ := weaponWorld(t)
	src.SetUnitWeaponField(attacker, 0, WeaponFieldDamageBase, 40)
	src.SetUnitWeaponField(attacker, 0, WeaponFieldAttackType, 1) // holy
	reg := NewHashRegistry()
	var ss, ds statehash.Snapshot
	srcHash := src.HashState(reg, &ss).Top

	var buf bytes.Buffer
	if err := src.SaveState(&buf, 0); err != nil {
		t.Fatalf("SaveState: %v", err)
	}
	// fresh world re-armed at base (no override) then loaded — the save restores
	// the overrides on top of the re-spawned units.
	dst, _, _ := weaponWorld(t)
	if err := dst.LoadState(bytes.NewReader(buf.Bytes()), 0); err != nil {
		t.Fatalf("LoadState: %v", err)
	}
	dstHash := dst.HashState(reg, &ds).Top
	got, _ := dst.GetUnitWeaponField(attacker, 0, WeaponFieldDamageBase)
	roll := dst.rollWeapon(dst.Combats.Row(attacker), 0)
	t.Logf("FSV #476 save/load: srcHash=%#016x dstHash=%#016x Get=%d roll=%.0f", srcHash, dstHash, got, float64(roll)/float64(fixed.One))
	if dstHash != srcHash {
		t.Fatalf("post-load hash %#x != pre-save %#x", dstHash, srcHash)
	}
	if got != 40 || roll != 40*fixed.One {
		t.Fatalf("post-load override Get=%d roll=%v, want 40", got, roll)
	}
}

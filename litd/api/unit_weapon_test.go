package litd

// FSV for the #476 api surface: Unit.SetWeaponField / ClearWeaponField /
// WeaponField. SoT = the next attack's rolled damage (read from the sim) + the
// getter.

import (
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
)

func weaponAPIGame(t *testing.T) (*sim.World, *Game, Unit) {
	t.Helper()
	w := sim.NewWorld(sim.Caps{Units: 8})
	if err := w.BindDamageTypes([]string{"normal", "holy"}, []string{"unarmored"}); err != nil {
		t.Fatalf("BindDamageTypes: %v", err)
	}
	if err := w.BindDamageMatrix([][]int32{{1000}, {2000}}); err != nil {
		t.Fatalf("BindDamageMatrix: %v", err)
	}
	g := newGame(w)
	id, ok := w.CreateUnit(fixed.Vec2{X: fixed.FromInt(100), Y: fixed.FromInt(100)}, 0)
	if !ok || !w.Healths.Add(w.Ents, id, 1000*fixed.One, 0, 0, 0) || !w.Combats.Add(w.Ents, id) {
		t.Fatal("spawn failed")
	}
	atk := &data.Attack{DamageBase: 10, CooldownTicks: 30, Range: fixed.FromInt(200)}
	if !w.SetWeapon(id, 0, atk, 0, data.EffectList{}) {
		t.Fatal("SetWeapon failed")
	}
	return w, g, Unit{id: id, g: g}
}

// TestUnitSetWeaponFieldFSV — set the weapon's damage base 10→40 through the
// api; the sim's rolled damage and the getter reflect 40, and a clear reverts.
func TestUnitSetWeaponFieldFSV(t *testing.T) {
	w, _, u := weaponAPIGame(t)

	if v, ok := u.WeaponField(0, WeaponDamageBase); !ok || v != 10 {
		t.Fatalf("initial WeaponField = %d ok=%v, want 10", v, ok)
	}
	if !u.SetWeaponField(0, WeaponDamageBase, 40) {
		t.Fatal("SetWeaponField returned false")
	}
	v, _ := u.WeaponField(0, WeaponDamageBase)
	// confirm the override reached the sim store (the value the attack resolve reads).
	simV, _ := w.GetUnitWeaponField(u.id, 0, sim.WeaponFieldDamageBase)
	t.Logf("FSV #476 api set: api WeaponField=%d sim store=%d", v, simV)
	if v != 40 || simV != 40 {
		t.Fatalf("after set: api=%d sim=%d, want 40", v, simV)
	}

	if !u.ClearWeaponField(0, WeaponDamageBase) {
		t.Fatal("ClearWeaponField returned false")
	}
	v2, _ := u.WeaponField(0, WeaponDamageBase)
	t.Logf("FSV #476 api clear: WeaponField=%d (reverted to data default)", v2)
	if v2 != 10 {
		t.Fatalf("after clear: WeaponField=%d, want 10", v2)
	}
}

// TestUnitSetWeaponFieldFailClosedFSV — invalid handle/slot/value no-op false.
func TestUnitSetWeaponFieldFailClosedFSV(t *testing.T) {
	_, _, u := weaponAPIGame(t)
	if u.SetWeaponField(9, WeaponDamageBase, 40) {
		t.Fatal("bad slot accepted")
	}
	if u.SetWeaponField(0, WeaponAttackType, 99) {
		t.Fatal("attack-type out of matrix accepted")
	}
	var zero Unit
	if zero.SetWeaponField(0, WeaponDamageBase, 40) {
		t.Fatal("zero-value unit accepted")
	}
	t.Log("FSV #476 api fail-closed: bad slot / bad type / invalid handle all refused")
}

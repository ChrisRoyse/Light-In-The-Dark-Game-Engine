package sim

// FSV for #472: data-driven, extensible attack/armor type tables. SoT = the
// declared type tables + matrix dims, and a damage result using a NEW type
// hand-computed (X+X=Y), plus the fail-closed validation paths.

import (
	"strings"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
)

// typedWorld binds named tables + a matrix and spawns a victim with the given
// armor type/value (value 0 → armor mult exactly 1.0, so the only mitigation is
// the per-mille coefficient — a clean hand-check).
func typedWorld(t *testing.T, attack, armor []string, matrix [][]int32, victimArmorType uint8) (*World, EntityID, EntityID) {
	t.Helper()
	w := NewWorld(Caps{})
	if err := w.BindDamageTypes(attack, armor); err != nil {
		t.Fatalf("BindDamageTypes: %v", err)
	}
	if err := w.BindDamageMatrix(matrix); err != nil {
		t.Fatalf("BindDamageMatrix: %v", err)
	}
	mk := func(x int32, at uint8) EntityID {
		id, ok := w.CreateUnit(fixed.Vec2{X: fixed.FromInt(x), Y: fixed.FromInt(100)}, 0)
		if !ok || !w.Healths.Add(w.Ents, id, 100*fixed.One, 0, 0, at) || !w.Combats.Add(w.Ents, id) {
			t.Fatal("spawn failed")
		}
		return id
	}
	victim := mk(100, victimArmorType)
	attacker := mk(200, 0)
	return w, victim, attacker
}

// TestDamageTypesNewTypeHandCheckFSV — add a 6th attack type "holy" and fire it
// at a "heavy" target; the post-mitigation HP delta matches the hand-computed
// value from the new matrix row.
func TestDamageTypesNewTypeHandCheckFSV(t *testing.T) {
	attack := []string{"normal", "piercing", "siege", "magic", "chaos", "holy"}
	armor := []string{"unarmored", "light", "medium", "heavy", "fortified"}
	// matrix: 6 rows × 5 cols. Only the holy row (idx 5) matters here:
	// holy vs heavy (col 3) = 500 per-mille = 50%.
	matrix := [][]int32{
		{1000, 1000, 1500, 1000, 700},  // normal
		{1500, 2000, 750, 1000, 350},   // piercing
		{1500, 1000, 500, 1000, 1500},  // siege
		{1000, 1250, 750, 2000, 350},   // magic
		{1000, 1000, 1000, 1000, 1000}, // chaos
		{2000, 1500, 1000, 500, 250},   // holy (NEW)
	}
	holy, ok := nameIndex(attack, "holy")
	if !ok || holy != 5 {
		t.Fatalf("holy index = %d ok=%v, want 5", holy, ok)
	}
	heavy, ok := nameIndex(armor, "heavy")
	if !ok || heavy != 3 {
		t.Fatalf("heavy index = %d ok=%v, want 3", heavy, ok)
	}
	w, victim, attacker := typedWorld(t, attack, armor, matrix, heavy)

	hr := w.Healths.Row(victim)
	before := w.Healths.Life[hr]
	// 40 raw holy damage vs heavy: 40 * 500/1000 = 20; armor value 0 → 1.0 mult.
	stepWithPackets(w, DamagePacket{Source: attacker, Target: victim, Amount: 40 * fixed.One, AttackType: holy})
	after := w.Healths.Life[w.Healths.Row(victim)]

	wantDelta := 20 * fixed.One
	t.Logf("FSV #472 holy-vs-heavy: types=%dx%d coeff[holy][heavy]=%d before=%d after=%d delta=%d wantDelta=%d",
		len(attack), len(armor), matrix[holy][heavy], before, after, before-after, wantDelta)
	if before-after != wantDelta {
		t.Fatalf("HP delta = %d, want %d (hand-check 40*500/1000=20)", before-after, wantDelta)
	}
}

// TestDamageTypesDimsMismatchFailsClosedFSV — a matrix whose dims disagree with
// the declared type tables is refused (both axes).
func TestDamageTypesDimsMismatchFailsClosedFSV(t *testing.T) {
	w := NewWorld(Caps{})
	if err := w.BindDamageTypes([]string{"a", "b"}, []string{"x", "y"}); err != nil {
		t.Fatalf("BindDamageTypes: %v", err)
	}
	// 3 rows vs 2 declared attack types.
	err := w.BindDamageMatrix([][]int32{{1, 1}, {1, 1}, {1, 1}})
	t.Logf("FSV #472 row mismatch: err=%v", err)
	if err == nil || !strings.Contains(err.Error(), "attack types declared") {
		t.Fatalf("3 rows vs 2 attack types: err=%v, want a rows-mismatch error", err)
	}
	// 2 rows but 3 cols vs 2 declared armor types.
	err = w.BindDamageMatrix([][]int32{{1, 1, 1}, {1, 1, 1}})
	t.Logf("FSV #472 col mismatch: err=%v", err)
	if err == nil || !strings.Contains(err.Error(), "armor types declared") {
		t.Fatalf("3 cols vs 2 armor types: err=%v, want a cols-mismatch error", err)
	}
}

// TestDamageTypesUnknownRuntimeIDDroppedFSV — an out-of-range attack-type id at
// runtime drops the packet, counted, leaving HP untouched.
func TestDamageTypesUnknownRuntimeIDDroppedFSV(t *testing.T) {
	attack := []string{"normal", "holy"}
	armor := []string{"unarmored", "heavy"}
	matrix := [][]int32{{1000, 1000}, {2000, 500}}
	w, victim, attacker := typedWorld(t, attack, armor, matrix, 1)

	hr := w.Healths.Row(victim)
	before := w.Healths.Life[hr]
	beforeDropped := w.dmgDropped
	stepWithPackets(w, DamagePacket{Source: attacker, Target: victim, Amount: 40 * fixed.One, AttackType: 99})
	after := w.Healths.Life[w.Healths.Row(victim)]

	t.Logf("FSV #472 unknown id 99: before=%d after=%d dropped %d→%d", before, after, beforeDropped, w.dmgDropped)
	if after != before {
		t.Fatalf("HP changed (%d→%d) for an unknown attack-type id — must drop", before, after)
	}
	if w.dmgDropped != beforeDropped+1 {
		t.Fatalf("dmgDropped = %d, want %d (the dropped packet must be counted)", w.dmgDropped, beforeDropped+1)
	}
}

// TestDamageTypesOrderIsIndexFSV — index is table order; reordering the names
// re-keys the matrix deterministically (no hidden canonical order).
func TestDamageTypesOrderIsIndexFSV(t *testing.T) {
	w := NewWorld(Caps{})
	if err := w.BindDamageTypes([]string{"holy", "normal"}, []string{"heavy", "unarmored"}); err != nil {
		t.Fatalf("BindDamageTypes: %v", err)
	}
	for name, want := range map[string]uint8{"holy": 0, "normal": 1} {
		got, ok := w.AttackTypeIndex(name)
		if !ok || got != want {
			t.Fatalf("AttackTypeIndex(%q) = %d ok=%v, want %d", name, got, ok, want)
		}
	}
	if got, ok := w.ArmorTypeIndex("unarmored"); !ok || got != 1 {
		t.Fatalf("ArmorTypeIndex(unarmored) = %d ok=%v, want 1 (reordered)", got, ok)
	}
	if w.AttackTypeName(0) != "holy" || w.ArmorTypeName(0) != "heavy" {
		t.Fatalf("name round-trip: attack[0]=%q armor[0]=%q, want holy/heavy", w.AttackTypeName(0), w.ArmorTypeName(0))
	}
	t.Log("FSV #472: index == declared table order; reorder re-keys the matrix")
}

// TestDamageTypesValidationFSV — fail-closed on empty / duplicate / unknown.
func TestDamageTypesValidationFSV(t *testing.T) {
	w := NewWorld(Caps{})
	if err := w.BindDamageTypes(nil, []string{"x"}); err == nil {
		t.Fatal("empty attack table accepted")
	}
	if err := w.BindDamageTypes([]string{"a", "a"}, []string{"x"}); err == nil || !strings.Contains(err.Error(), "duplicate") {
		t.Fatalf("duplicate attack name: err=%v, want duplicate error", err)
	}
	if err := w.BindDamageTypes([]string{"a", ""}, []string{"x"}); err == nil || !strings.Contains(err.Error(), "empty") {
		t.Fatalf("empty name: err=%v, want empty-name error", err)
	}
	// Unknown / unbound lookups fail closed.
	if _, ok := w.AttackTypeIndex("nope"); ok {
		t.Fatal("unknown attack-type name returned ok=true")
	}
	if n := w.ArmorTypeName(200); n != "" {
		t.Fatalf("out-of-range ArmorTypeName = %q, want \"\"", n)
	}
	t.Log("FSV #472 validation: empty/duplicate/empty-name refused; unknown lookups fail closed")
}

// TestDamageTypesBaseUnchangedFSV — binding the shipped 5×5 names leaves base
// behavior identical: normal(0) vs unarmored(0) at 1000 per-mille = full damage.
func TestDamageTypesBaseUnchangedFSV(t *testing.T) {
	attack := []string{"normal", "piercing", "siege", "magic", "chaos"}
	armor := []string{"unarmored", "light", "medium", "heavy", "fortified"}
	matrix := [][]int32{
		{1000, 1000, 1500, 1000, 700},
		{1500, 2000, 750, 1000, 350},
		{1500, 1000, 500, 1000, 1500},
		{1000, 1250, 750, 2000, 350},
		{1000, 1000, 1000, 1000, 1000},
	}
	w, victim, attacker := typedWorld(t, attack, armor, matrix, 0) // unarmored
	hr := w.Healths.Row(victim)
	before := w.Healths.Life[hr]
	stepWithPackets(w, DamagePacket{Source: attacker, Target: victim, Amount: 30 * fixed.One, AttackType: 0})
	after := w.Healths.Life[w.Healths.Row(victim)]
	if before-after != 30*fixed.One {
		t.Fatalf("normal-vs-unarmored delta = %d, want 30 (1000 per-mille, base unchanged)", before-after)
	}
	t.Logf("FSV #472 base unchanged: normal-vs-unarmored 30 raw → 30 applied (full)")
}

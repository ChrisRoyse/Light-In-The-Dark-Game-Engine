package litd

// FSV for the #472 public surface: DefineDamageTypes declares the named tables,
// AttackTypeID/ArmorTypeID resolve names to matrix indices (the seam combat
// conditions build on), and DefineCombat validates the matrix dims against the
// declared types. SoT = the returned indices/ok flags and the bind errors.

import "testing"

func damageTypeGame(t *testing.T) *Game {
	t.Helper()
	g, err := NewGame(GameOptions{MaxUnits: 8, Seed: 5})
	if err != nil {
		t.Fatalf("NewGame: %v", err)
	}
	return g
}

// TestDefineDamageTypesAndLookupFSV — declared names resolve to their table
// order; unknown names fail closed.
func TestDefineDamageTypesAndLookupFSV(t *testing.T) {
	g := damageTypeGame(t)
	if err := g.DefineDamageTypes(
		[]string{"normal", "piercing", "siege", "magic", "chaos", "holy"},
		[]string{"unarmored", "light", "medium", "heavy", "fortified"},
	); err != nil {
		t.Fatalf("DefineDamageTypes: %v", err)
	}
	for name, want := range map[string]int{"normal": 0, "holy": 5} {
		got, ok := g.AttackTypeID(name)
		if !ok || got != want {
			t.Fatalf("AttackTypeID(%q) = %d ok=%v, want %d", name, got, ok, want)
		}
	}
	if got, ok := g.ArmorTypeID("heavy"); !ok || got != 3 {
		t.Fatalf("ArmorTypeID(heavy) = %d ok=%v, want 3", got, ok)
	}
	if _, ok := g.AttackTypeID("nonexistent"); ok {
		t.Fatal("unknown attack-type name returned ok=true — must fail closed")
	}
	t.Log("FSV #472 api: names resolve to table order; unknown → ok=false")
}

// TestDefineCombatDimsMatchTypesFSV — once types are declared, a matrix sized
// for them binds; a mis-sized one is rejected, loudly, by DefineCombat.
func TestDefineCombatDimsMatchTypesFSV(t *testing.T) {
	g := damageTypeGame(t)
	if err := g.DefineDamageTypes([]string{"normal", "holy"}, []string{"unarmored", "heavy"}); err != nil {
		t.Fatalf("DefineDamageTypes: %v", err)
	}
	// Correct 2×2 binds.
	if err := g.DefineCombat([][]int{{1000, 1000}, {2000, 500}}); err != nil {
		t.Fatalf("DefineCombat 2x2: %v", err)
	}
	// 3 rows vs 2 attack types → rejected.
	err := g.DefineCombat([][]int{{1000, 1000}, {2000, 500}, {1, 1}})
	t.Logf("FSV #472 api dims-mismatch: err=%v", err)
	if err == nil {
		t.Fatal("DefineCombat accepted a matrix mis-sized vs the declared types")
	}
}

// TestDefineDamageTypesFailClosedFSV — empty/duplicate tables are refused.
func TestDefineDamageTypesFailClosedFSV(t *testing.T) {
	g := damageTypeGame(t)
	if err := g.DefineDamageTypes(nil, []string{"x"}); err == nil {
		t.Fatal("empty attack table accepted")
	}
	if err := g.DefineDamageTypes([]string{"a", "a"}, []string{"x"}); err == nil {
		t.Fatal("duplicate attack-type name accepted")
	}
	t.Log("FSV #472 api: empty/duplicate type tables refused")
}

package sim

import "testing"

// #569 — intern tables. SoT = the id↔string mapping and its stability.

func TestInternStableAndAppendOnly(t *testing.T) {
	s := NewKVStore(16)
	a := s.InternKey("enemyCount")
	b := s.InternKey("score")
	if a == 0 || b == 0 || a == b {
		t.Fatalf("ids must be nonzero + distinct: a=%d b=%d", a, b)
	}
	// Re-interning returns the SAME id (stable).
	if s.InternKey("enemyCount") != a {
		t.Fatal("re-intern changed id")
	}
	// KeyID is read-only: known key resolves, unseen key is 0.
	if s.KeyID("score") != b || s.KeyID("never") != 0 {
		t.Fatal("KeyID wrong")
	}
	if str, ok := s.KeyString(a); !ok || str != "enemyCount" {
		t.Fatalf("KeyString(%d) = %q,%v", a, str, ok)
	}
	if _, ok := s.KeyString(0); ok {
		t.Fatal("id 0 resolved")
	}
}

func TestInternKeyAndStringSeparateSpaces(t *testing.T) {
	s := NewKVStore(16)
	k := s.InternKey("weapon")  // a KEY named "weapon"
	v := s.InternStr("weapon")  // a string VALUE "weapon"
	// Both get id 1 in their own table; the spaces are independent.
	if got, _ := s.KeyString(k); got != "weapon" {
		t.Fatal("key table wrong")
	}
	if got, _ := s.StrValue(v); got != "weapon" {
		t.Fatal("string table wrong")
	}
	// Interning more keys does not perturb the string-value ids.
	s.InternKey("other")
	if got, _ := s.StrValue(v); got != "weapon" {
		t.Fatal("string ids perturbed by key interning")
	}
}

func TestInternRebuildIndex(t *testing.T) {
	s := NewKVStore(16)
	id1 := s.InternKey("a")
	id2 := s.InternKey("b")
	id3 := s.InternKey("c")
	// Simulate a load: drop the derived index, rebuild from list.
	s.keys.rebuildIndex()
	if s.KeyID("a") != id1 || s.KeyID("b") != id2 || s.KeyID("c") != id3 {
		t.Fatal("rebuildIndex produced different ids")
	}
	// A fresh intern after rebuild continues the id sequence.
	if s.InternKey("d") != id3+1 {
		t.Fatal("intern after rebuild did not continue ids")
	}
}

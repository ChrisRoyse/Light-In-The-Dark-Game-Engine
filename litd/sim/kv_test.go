package sim

import "testing"

// #568 — KVStore sorted-array core. SoT = the Owner/Key/Type/Val columns
// read directly (sortedInvariant + locate).

func kvSortedInvariant(t *testing.T, s *KVStore) {
	t.Helper()
	for i := int32(1); i < s.count; i++ {
		if !kvLess(s.Owner[i-1], s.Key[i-1], s.Owner[i], s.Key[i]) {
			t.Fatalf("sort invariant broken at row %d: (%d,%d) !< (%d,%d)",
				i, s.Owner[i-1], s.Key[i-1], s.Owner[i], s.Key[i])
		}
	}
}

func TestKVOwnerPacking(t *testing.T) {
	o := makeOwner(KVScopePlayer, 5)
	if ownerScope(o) != KVScopePlayer || ownerEntity(o) != 5 {
		t.Fatalf("owner pack/unpack: scope=%d ent=%d", ownerScope(o), ownerEntity(o))
	}
	// Scope dominates ordering: entity-scope owner < global < player.
	e := makeOwner(KVScopeEntity, 999999)
	g := makeOwner(KVScopeGlobal, 0)
	if !(e < g && g < makeOwner(KVScopePlayer, 0)) {
		t.Fatal("scope does not dominate owner sort order")
	}
}

func TestKVSetGetSortedAndUpsert(t *testing.T) {
	s := NewKVStore(64)
	gl := makeOwner(KVScopeGlobal, 0)
	// Insert out of key order; the store must keep sorted.
	s.set(gl, 30, KVInt, 300, 0)
	s.set(gl, 10, KVInt, 100, 0)
	s.set(gl, 20, KVInt, 200, 0)
	kvSortedInvariant(t, s)
	if s.count != 3 {
		t.Fatalf("count = %d, want 3", s.count)
	}
	// Get returns the stored value.
	if _, v, _, ok := s.get(gl, 20); !ok || v != 200 {
		t.Fatalf("get(20) = %d,%v", v, ok)
	}
	// Upsert overwrites in place (no new row), and may change the type.
	s.set(gl, 20, KVFixed, 999, 0)
	if s.count != 3 {
		t.Fatalf("upsert grew count to %d", s.count)
	}
	if typ, v, _, _ := s.get(gl, 20); typ != KVFixed || v != 999 {
		t.Fatalf("upsert: typ=%d v=%d, want KVFixed/999", typ, v)
	}
}

func TestKVVec2TwoColumns(t *testing.T) {
	s := NewKVStore(16)
	o := makeOwner(KVScopeEntity, 42)
	s.set(o, 1, KVVec2, 11, 22)
	typ, v, v2, ok := s.get(o, 1)
	if !ok || typ != KVVec2 || v != 11 || v2 != 22 {
		t.Fatalf("vec2 round-trip: typ=%d v=%d v2=%d ok=%v", typ, v, v2, ok)
	}
}

func TestKVDeleteKeepsSorted(t *testing.T) {
	s := NewKVStore(64)
	o := makeOwner(KVScopeGlobal, 0)
	for k := uint32(1); k <= 5; k++ {
		s.set(o, k, KVInt, int64(k*10), 0)
	}
	if !s.del(o, 3) {
		t.Fatal("delete returned false")
	}
	if s.has(o, 3) {
		t.Fatal("key present after delete")
	}
	if s.count != 4 {
		t.Fatalf("count after delete = %d, want 4", s.count)
	}
	kvSortedInvariant(t, s)
	if s.del(o, 99) {
		t.Fatal("delete of absent returned true")
	}
}

func TestKVAbsentReturnsZero(t *testing.T) {
	s := NewKVStore(8)
	o := makeOwner(KVScopeGlobal, 0)
	if _, _, _, ok := s.get(o, 7); ok {
		t.Fatal("get of absent returned ok")
	}
	if s.has(o, 7) {
		t.Fatal("has of absent returned true")
	}
}

func TestKVExhaustion(t *testing.T) {
	const cap = 4
	s := NewKVStore(cap)
	o := makeOwner(KVScopeGlobal, 0)
	for k := uint32(1); k <= cap; k++ {
		if !s.set(o, k, KVInt, 1, 0) {
			t.Fatalf("set %d failed below cap", k)
		}
	}
	if s.set(o, 99, KVInt, 1, 0) {
		t.Fatal("set past cap returned true")
	}
	if s.Dropped != 1 {
		t.Fatalf("Dropped = %d, want 1", s.Dropped)
	}
	// Upsert of an existing key still works at full capacity.
	if !s.set(o, 1, KVInt, 7, 0) {
		t.Fatal("upsert at full cap failed")
	}
}

func TestKVMultiOwnerContiguous(t *testing.T) {
	s := NewKVStore(64)
	a := makeOwner(KVScopeEntity, 1)
	b := makeOwner(KVScopeEntity, 2)
	s.set(b, 1, KVInt, 1, 0)
	s.set(a, 2, KVInt, 1, 0)
	s.set(a, 1, KVInt, 1, 0)
	s.set(b, 2, KVInt, 1, 0)
	kvSortedInvariant(t, s)
	// owner a's pairs occupy a contiguous prefix run.
	if s.Owner[0] != a || s.Owner[1] != a || s.Owner[2] != b || s.Owner[3] != b {
		t.Fatalf("owners not contiguous: %v", s.Owner[:4])
	}
}

func TestKVZeroAlloc(t *testing.T) {
	s := NewKVStore(256)
	o := makeOwner(KVScopeGlobal, 0)
	for k := uint32(0); k < 64; k++ {
		s.set(o, k, KVInt, 1, 0)
	}
	avg := testing.AllocsPerRun(1000, func() {
		s.set(o, 32, KVInt, 5, 0) // upsert
		s.get(o, 32)
		s.has(o, 10)
	})
	if avg != 0 {
		t.Fatalf("kv churn allocated %.2f objs/op, want 0", avg)
	}
}

func TestNewWorldWiresKV(t *testing.T) {
	w := NewWorld(Caps{})
	if w.KV == nil || w.KV.Cap() != EngineCaps.KVPairs {
		t.Fatalf("KV not wired: %v", w.KV)
	}
}

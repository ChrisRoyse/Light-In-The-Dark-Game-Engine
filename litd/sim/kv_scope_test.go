package sim

import "testing"

// #570 — scoped ops. SoT = values read back + the sorted columns after
// clearOwner/delete.

func TestKVScopesIsolated(t *testing.T) {
	s := NewKVStore(64)
	k := s.InternKey("gold")
	ent := makeOwner(KVScopeEntity, 7)
	glob := makeOwner(KVScopeGlobal, 0)
	p1 := makeOwner(KVScopePlayer, 1)
	s.KVSet(ent, k, KVInt, 10, 0)
	s.KVSet(glob, k, KVInt, 20, 0)
	s.KVSet(p1, k, KVInt, 30, 0)
	// Same key id, three scopes — three independent values.
	if _, v, _, _ := s.KVGet(ent, k); v != 10 {
		t.Fatalf("entity scope = %d, want 10", v)
	}
	if _, v, _, _ := s.KVGet(glob, k); v != 20 {
		t.Fatalf("global scope = %d, want 20", v)
	}
	if _, v, _, _ := s.KVGet(p1, k); v != 30 {
		t.Fatalf("player scope = %d, want 30", v)
	}
}

func TestKVEachOwnerKeyOrder(t *testing.T) {
	s := NewKVStore(64)
	o := makeOwner(KVScopeEntity, 5)
	other := makeOwner(KVScopeEntity, 6)
	s.KVSet(other, 1, KVInt, 999, 0) // noise from another owner
	for _, k := range []uint32{30, 10, 20} {
		s.KVSet(o, k, KVInt, int64(k), 0)
	}
	var keys []uint32
	s.KVEachOwner(o, func(key uint32, _ KVType, _, _ int64) { keys = append(keys, key) })
	if len(keys) != 3 || keys[0] != 10 || keys[1] != 20 || keys[2] != 30 {
		t.Fatalf("EachOwner visited %v, want [10 20 30] in key order (other owner excluded)", keys)
	}
}

func TestKVClearOwner(t *testing.T) {
	s := NewKVStore(64)
	a := makeOwner(KVScopeEntity, 1)
	b := makeOwner(KVScopeEntity, 2)
	for k := uint32(1); k <= 5; k++ {
		s.KVSet(a, k, KVInt, 1, 0)
	}
	s.KVSet(b, 1, KVInt, 2, 0)
	s.KVSet(b, 2, KVInt, 2, 0)
	n := s.KVClearOwner(a)
	if n != 5 {
		t.Fatalf("ClearOwner removed %d, want 5", n)
	}
	if s.Count() != 2 {
		t.Fatalf("count after clear = %d, want 2 (owner b survives)", s.Count())
	}
	if s.KVHas(a, 1) {
		t.Fatal("owner a key present after clear")
	}
	if !s.KVHas(b, 1) || !s.KVHas(b, 2) {
		t.Fatal("owner b pairs lost in clear")
	}
	kvSortedInvariant(t, s)
	// Clearing an absent owner is a 0 no-op.
	if s.KVClearOwner(makeOwner(KVScopeEntity, 99)) != 0 {
		t.Fatal("clear of absent owner removed rows")
	}
}

func TestKVScopeZeroAlloc(t *testing.T) {
	s := NewKVStore(256)
	o := makeOwner(KVScopeGlobal, 0)
	for k := uint32(0); k < 32; k++ {
		s.KVSet(o, k, KVInt, 1, 0)
	}
	avg := testing.AllocsPerRun(1000, func() {
		s.KVSet(o, 16, KVInt, 9, 0)
		s.KVGet(o, 16)
		s.KVEachOwner(o, func(uint32, KVType, int64, int64) {})
	})
	if avg != 0 {
		t.Fatalf("scoped op churn allocated %.2f objs/op, want 0", avg)
	}
}

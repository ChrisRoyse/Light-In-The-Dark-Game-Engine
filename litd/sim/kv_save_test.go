package sim

import (
	"bytes"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/statehash"
)

// #572 — kv hash section + save block + dead-owner prune. SoT = the "kv"
// sub-hash and the rebuilt store columns/intern tables.

func armKVPopulation(w *World) {
	kv := w.KV
	kGold := kv.InternKey("gold")
	kName := kv.InternKey("name")
	kPos := kv.InternKey("home")
	weapon := kv.InternStr("sword")
	ent := makeOwner(KVScopeEntity, 7)
	glob := makeOwner(KVScopeGlobal, 0)
	p1 := makeOwner(KVScopePlayer, 1)
	kv.KVSet(glob, kGold, KVInt, 500, 0)
	kv.KVSet(p1, kGold, KVInt, 99, 0)
	kv.KVSet(ent, kName, KVString, int64(weapon), 0)
	kv.KVSet(ent, kPos, KVVec2, 123, 456)
	kv.KVSet(ent, kGold, KVFixed, 777, 0)
}

func TestKVSaveRoundTripHash(t *testing.T) {
	src := NewWorld(Caps{Units: 8, KVPairs: 256})
	armKVPopulation(src)

	reg := NewHashRegistry()
	var before statehash.Snapshot
	src.HashState(reg, &before)

	var buf bytes.Buffer
	if err := src.SaveState(&buf, 0); err != nil {
		t.Fatalf("SaveState: %v", err)
	}
	saved := append([]byte(nil), buf.Bytes()...)

	dst := NewWorld(Caps{Units: 8, KVPairs: 256})
	if err := dst.LoadState(bytes.NewReader(saved), 0); err != nil {
		t.Fatalf("LoadState: %v", err)
	}
	var after statehash.Snapshot
	dst.HashState(reg, &after)

	ki := hashSystemIndex(t, "kv")
	if before.Subs[ki] != after.Subs[ki] {
		t.Fatalf("kv sub-hash differs: %016x -> %016x", before.Subs[ki], after.Subs[ki])
	}
	if before.Top != after.Top {
		t.Fatalf("top differs; diverged: %v", snapDiff(t, &before, &after))
	}
	// SoT: a string value resolves to the same string post-load.
	ent := makeOwner(KVScopeEntity, 7)
	kName := dst.KV.KeyID("name")
	if _, v, _, ok := dst.KV.KVGet(ent, kName); !ok {
		t.Fatal("name key missing after load")
	} else if s, _ := dst.KV.StrValue(uint32(v)); s != "sword" {
		t.Fatalf("string value after load = %q, want sword", s)
	}
	if dst.KV.Count() != src.KV.Count() {
		t.Fatalf("count %d != %d", dst.KV.Count(), src.KV.Count())
	}
}

func TestKVHashDeterminismAndLocalization(t *testing.T) {
	mk := func() *World {
		w := NewWorld(Caps{Units: 8, KVPairs: 256})
		armKVPopulation(w)
		return w
	}
	reg := NewHashRegistry()
	var a, b statehash.Snapshot
	mk().HashState(reg, &a)
	mk().HashState(reg, &b)
	if a.Top != b.Top {
		t.Fatalf("identical kv ops diverged: %v", snapDiff(t, &a, &b))
	}
	w2 := mk()
	w2.KV.KVSet(makeOwner(KVScopeGlobal, 0), w2.KV.InternKey("extra"), KVInt, 1, 0)
	var c statehash.Snapshot
	w2.HashState(reg, &c)
	ki := hashSystemIndex(t, "kv")
	if a.Subs[ki] == c.Subs[ki] {
		t.Fatal("added pair did not change the kv sub-hash")
	}
	for i := range a.Subs {
		if i != ki && a.Subs[i] != c.Subs[i] {
			t.Fatalf("non-kv sub %d changed — leak outside kv", i)
		}
	}
}

// Dead-owner prune: an entity's KV pairs vanish when it dies, while
// global/player scope survives.
func TestKVDeadOwnerPrune(t *testing.T) {
	w, u := queryWorld(t)
	kv := w.KV
	kHP := kv.InternKey("hp")
	entOwner := makeOwner(KVScopeEntity, uint64(u[0]))
	glob := makeOwner(KVScopeGlobal, 0)
	kv.KVSet(entOwner, kHP, KVInt, 42, 0)
	kv.KVSet(glob, kHP, KVInt, 7, 0)
	if !kv.KVHas(entOwner, kHP) {
		t.Fatal("setup: entity pair missing")
	}
	w.KillUnit(u[0])
	w.Step() // phase 7 prune
	if kv.KVHas(entOwner, kHP) {
		t.Fatal("dead entity's KV pair survived prune")
	}
	if !kv.KVHas(glob, kHP) {
		t.Fatal("global-scope pair wrongly pruned")
	}
}

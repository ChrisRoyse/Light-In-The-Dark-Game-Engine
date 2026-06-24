package sim

import (
	"bytes"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/statehash"
)

// #574 — the KV-store acceptance suite: the cross-cutting properties the
// per-feature tests (#568–#573) don't assert together — a recorded
// determinism golden, save/resume hash parity, steady-state zero-alloc,
// and the UserDataStore→KV migration (#571). Per-feature behavior lives
// in kv_test.go, kv_scope_test.go, kv_intern_test.go, kv_save_test.go.

// kvScenario drives a deterministic mixed sequence across all scopes,
// every value type, the intern tables, deletes, and a clear — the state
// most likely to expose a serialization or ordering bug.
func kvScenario(w *World) {
	kv := w.KV
	kHp := kv.InternKey("hp")
	kName := kv.InternKey("name")
	kPos := kv.InternKey("home")
	kFlag := kv.InternKey("boss")
	kRatio := kv.InternKey("ratio")
	sword := kv.InternStr("sword")
	shield := kv.InternStr("shield")

	for e := uint64(1); e <= 5; e++ {
		o := makeOwner(KVScopeEntity, e)
		kv.KVSet(o, kHp, KVInt, int64(e*100), 0)
		kv.KVSet(o, kRatio, KVFixed, int64(e)*7, 0)
		kv.KVSet(o, kFlag, KVBool, int64(e%2), 0)
		if e%2 == 0 {
			kv.KVSet(o, kName, KVString, int64(sword), 0)
		} else {
			kv.KVSet(o, kName, KVString, int64(shield), 0)
		}
		kv.KVSet(o, kPos, KVVec2, int64(e), int64(e*2))
	}
	// global + player scope
	kv.KVSet(makeOwner(KVScopeGlobal, 0), kHp, KVInt, 9999, 0)
	kv.KVSet(makeOwner(KVScopePlayer, 1), kHp, KVInt, 42, 0)
	kv.KVSet(makeOwner(KVScopePlayer, 2), kName, KVString, int64(sword), 0)
	// deletes + a clear exercise shift-down + run removal
	kv.KVDelete(makeOwner(KVScopeEntity, 3), kFlag)
	kv.KVClearOwner(makeOwner(KVScopeEntity, 5))
	// type change: last-write-wins
	kv.KVSet(makeOwner(KVScopeEntity, 1), kHp, KVFixed, 123, 0)
}

func kvTopHash(w *World, reg *statehash.Registry) uint64 {
	var s statehash.Snapshot
	w.HashState(reg, &s)
	return s.Top
}

func TestKVScenarioGolden(t *testing.T) {
	w := NewWorld(Caps{Units: 16, KVPairs: 512})
	kvScenario(w)
	const golden = uint64(0x76160b54e252e0e5) // recorded 2026-06-23 (#574)
	got := kvTopHash(w, NewHashRegistry())
	if golden != 0 && got != golden {
		t.Fatalf("kv golden hash %016x != recorded %016x (intended? update golden)", got, golden)
	}
	t.Logf("kv scenario golden = %#016x", got)
}

func TestKVTwoRunDeterminism(t *testing.T) {
	mk := func() uint64 {
		w := NewWorld(Caps{Units: 16, KVPairs: 512})
		kvScenario(w)
		return kvTopHash(w, NewHashRegistry())
	}
	if a, b := mk(), mk(); a != b {
		t.Fatalf("two kv scenario runs diverged: %016x != %016x", a, b)
	}
}

func TestKVSaveResumeParity(t *testing.T) {
	caps := Caps{Units: 16, KVPairs: 512}
	second := func(w *World) {
		kv := w.KV
		k := kv.InternKey("late")
		for e := uint64(6); e <= 9; e++ {
			kv.KVSet(makeOwner(KVScopeEntity, e), k, KVInt, int64(e), 0)
		}
		kv.KVDelete(makeOwner(KVScopeEntity, 7), k)
	}
	wu := NewWorld(caps)
	kvScenario(wu)
	second(wu)
	want := kvTopHash(wu, NewHashRegistry())

	ws := NewWorld(caps)
	kvScenario(ws)
	var buf bytes.Buffer
	if err := ws.SaveState(&buf, 0); err != nil {
		t.Fatalf("SaveState: %v", err)
	}
	wl := NewWorld(caps)
	if err := wl.LoadState(bytes.NewReader(buf.Bytes()), 0); err != nil {
		t.Fatalf("LoadState: %v", err)
	}
	second(wl)
	if got := kvTopHash(wl, NewHashRegistry()); got != want {
		t.Fatalf("kv save/resume hash %016x != unbroken %016x", got, want)
	}
	t.Logf("FSV #574: kv save/resume parity holds; hash %#016x", want)
}

func TestKVSteadyStateZeroAlloc(t *testing.T) {
	w := NewWorld(Caps{Units: 16, KVPairs: 512})
	o := makeOwner(KVScopeGlobal, 0)
	k := w.KV.InternKey("k")
	str := w.KV.InternStr("v")
	for i := uint32(0); i < 32; i++ {
		w.KV.KVSet(o, w.KV.InternKey("pre"+string(rune('a'+i%26))), KVInt, 1, 0)
	}
	avg := testing.AllocsPerRun(1000, func() {
		w.KV.KVSet(o, k, KVInt, 5, 0)
		w.KV.KVGet(o, k)
		w.KV.KVSet(o, k, KVString, int64(str), 0) // type change in place
		w.KV.KVHas(o, k)
	})
	if avg != 0 {
		t.Fatalf("kv steady-state churn allocated %.2f objs/op, want 0", avg)
	}
}

// Migration (#571): SetUserData persists across save/load via the KV
// reserved-key shim, with no dedicated userdata store anywhere.
func TestKVUserDataMigrationSaveLoad(t *testing.T) {
	src := NewWorld(Caps{Units: 16, KVPairs: 256})
	id, ok := src.CreateUnit(CellCenter(1), 0)
	if !ok {
		t.Fatal("spawn failed")
	}
	if !src.SetUserData(id, 0x7FFFFFFF) {
		t.Fatal("SetUserData failed")
	}
	var buf bytes.Buffer
	if err := src.SaveState(&buf, 0); err != nil {
		t.Fatalf("SaveState: %v", err)
	}
	dst := NewWorld(Caps{Units: 16, KVPairs: 256})
	if err := dst.LoadState(bytes.NewReader(buf.Bytes()), 0); err != nil {
		t.Fatalf("LoadState: %v", err)
	}
	if got := dst.UserData(id); got != 0x7FFFFFFF {
		t.Fatalf("userdata after load = %d, want max-int32 (migration broke)", got)
	}
}

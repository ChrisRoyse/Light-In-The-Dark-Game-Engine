package sim

// FSV for #477: per-world runtime effect-primitive registry. SoT = the
// registered set (names, hashed) + the effect of running a custom primitive
// (source HP before/after) + save/load re-bind.

import (
	"bytes"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/statehash"
)

// regWorld: one unit at 500/1000 life (below max, room to heal).
func regWorld(t *testing.T) (*World, EntityID) {
	t.Helper()
	w := NewWorld(Caps{Units: 8})
	id, ok := w.CreateUnit(fixed.Vec2{X: fixed.FromInt(100), Y: fixed.FromInt(100)}, 0)
	if !ok || !w.Healths.Add(w.Ents, id, 1000*fixed.One, 0, 0, 0) {
		t.Fatal("spawn failed")
	}
	w.Healths.Life[w.Healths.Row(id)] = 500 * fixed.One
	return w, id
}

// lifesteal builds a runtime effect that heals its source by a fixed amount —
// the configured "fraction" is baked into the closure (S1: a named Action).
func lifesteal(heal fixed.F64) RuntimeEffectExec {
	return func(w *World, ctx EffectCtx) { w.HealUnit(ctx.Source, heal) }
}

// TestEffectRegisterAndRunFSV — register a custom "lifesteal" effect in setup,
// run it, and hand-check the source healed by the configured amount.
func TestEffectRegisterAndRunFSV(t *testing.T) {
	w, src := regWorld(t)
	id, ok := w.RegisterEffect("lifesteal", lifesteal(200*fixed.One))
	if !ok || id != 0 {
		t.Fatalf("RegisterEffect = (%d,%v), want (0,true)", id, ok)
	}
	hr := w.Healths.Row(src)
	before := w.Healths.Life[hr]
	if !w.RunRegisteredEffect(id, EffectCtxFor(src, 0)) {
		t.Fatal("RunRegisteredEffect returned false")
	}
	after := w.Healths.Life[hr]
	name, _ := w.RegisteredEffectName(id)
	t.Logf("FSV #477 run: registry=[%q] (count=%d), source life %.0f→%.0f", name, w.RegisteredEffectCount(), float64(before)/float64(fixed.One), float64(after)/float64(fixed.One))
	if after != 700*fixed.One {
		t.Fatalf("source life = %v, want 700 (500 + 200 lifesteal)", after)
	}
}

// TestEffectRegisterFailClosedFSV — registration after the match starts
// ticking, an empty/duplicate name, and a nil exec are all refused.
func TestEffectRegisterFailClosedFSV(t *testing.T) {
	w, _ := regWorld(t)
	if _, ok := w.RegisterEffect("", lifesteal(1)); ok {
		t.Fatal("empty name accepted")
	}
	if _, ok := w.RegisterEffect("nilexec", nil); ok {
		t.Fatal("nil exec accepted")
	}
	if _, ok := w.RegisterEffect("dup", lifesteal(1)); !ok {
		t.Fatal("first register failed")
	}
	if _, ok := w.RegisterEffect("dup", lifesteal(2)); ok {
		t.Fatal("duplicate name accepted")
	}
	// inside a tick phase: fail-closed (order must not vary by callback timing).
	w.OnCombatPhase = func(uint32) {
		if _, ok := w.RegisterEffect("mid-tick", lifesteal(1)); ok {
			t.Error("registration during a tick phase accepted")
		}
		w.OnCombatPhase = nil
	}
	w.Step()
	// after the first Step (tick > 0): frozen.
	if _, ok := w.RegisterEffect("post-step", lifesteal(1)); ok {
		t.Fatal("registration after first Step accepted")
	}
	if w.RegisteredEffectCount() != 1 {
		t.Fatalf("registry has %d entries, want 1 (only 'dup' survived)", w.RegisteredEffectCount())
	}
	t.Log("FSV #477 fail-closed: empty/nil/duplicate/mid-tick/post-step all refused")
}

// TestEffectRegistryHashAgreesFSV — two worlds that register the same set hash
// identically; a different order or set diverges; empty matches the base game.
func TestEffectRegistryHashAgreesFSV(t *testing.T) {
	reg := NewHashRegistry()
	top := func(build func(w *World)) uint64 {
		w, _ := regWorld(t)
		build(w)
		var s statehash.Snapshot
		return w.HashState(reg, &s).Top
	}
	base := top(func(*World) {})
	ab1 := top(func(w *World) { w.RegisterEffect("a", lifesteal(1)); w.RegisterEffect("b", lifesteal(2)) })
	ab2 := top(func(w *World) { w.RegisterEffect("a", lifesteal(9)); w.RegisterEffect("b", lifesteal(8)) })
	ba := top(func(w *World) { w.RegisterEffect("b", lifesteal(1)); w.RegisterEffect("a", lifesteal(2)) })
	t.Logf("FSV #477 hash: base=%#016x ab=%#016x (exec-independent) ba=%#016x", base, ab1, ba)
	if ab1 != ab2 {
		t.Fatalf("same names hash differently (%#x vs %#x) — only names are the contract", ab1, ab2)
	}
	if ab1 == base {
		t.Fatal("a registered set did not change the hash")
	}
	if ab1 == ba {
		t.Fatal("registration order must change the hash (ids are positional)")
	}
}

// TestEffectRegistrySaveLoadFSV — a registered set round-trips: a re-bound world
// (same registration in setup) loads, and the custom effect still heals.
func TestEffectRegistrySaveLoadFSV(t *testing.T) {
	src, srcID := regWorld(t)
	src.RegisterEffect("lifesteal", lifesteal(200*fixed.One))
	reg := NewHashRegistry()
	var ss, ds statehash.Snapshot
	srcHash := src.HashState(reg, &ss).Top

	var buf bytes.Buffer
	if err := src.SaveState(&buf, 0); err != nil {
		t.Fatalf("SaveState: %v", err)
	}

	// re-bound world: register the SAME set in setup, then load.
	dst, dstID := regWorld(t)
	id, _ := dst.RegisterEffect("lifesteal", lifesteal(200*fixed.One))
	if err := dst.LoadState(bytes.NewReader(buf.Bytes()), 0); err != nil {
		t.Fatalf("LoadState (matched re-bind): %v", err)
	}
	dstHash := dst.HashState(reg, &ds).Top
	hr := dst.Healths.Row(dstID)
	before := dst.Healths.Life[hr]
	dst.RunRegisteredEffect(id, EffectCtxFor(dstID, 0))
	after := dst.Healths.Life[hr]
	t.Logf("FSV #477 save/load: srcHash=%#016x dstHash=%#016x, post-load heal %.0f→%.0f", srcHash, dstHash, float64(before)/float64(fixed.One), float64(after)/float64(fixed.One))
	if dstHash != srcHash {
		t.Fatalf("post-load hash %#x != pre-save %#x", dstHash, srcHash)
	}
	if after-before != 200*fixed.One {
		t.Fatalf("post-load lifesteal healed %v, want 200 — re-bound exec did not run", after-before)
	}
	_ = srcID
}

// TestEffectRegistryLoadMismatchFSV — loading into a world that re-bound a
// different set (or none) fails closed.
func TestEffectRegistryLoadMismatchFSV(t *testing.T) {
	src, _ := regWorld(t)
	src.RegisterEffect("lifesteal", lifesteal(200*fixed.One))
	var buf bytes.Buffer
	if err := src.SaveState(&buf, 0); err != nil {
		t.Fatalf("SaveState: %v", err)
	}
	saved := buf.Bytes()

	// (a) re-bound a different name → fail closed.
	wDiff, _ := regWorld(t)
	wDiff.RegisterEffect("manaburn", lifesteal(1))
	if err := wDiff.LoadState(bytes.NewReader(saved), 0); err == nil {
		t.Fatal("load accepted a mismatched re-bound name")
	}
	// (b) re-bound nothing → fail closed (count mismatch).
	wNone, _ := regWorld(t)
	if err := wNone.LoadState(bytes.NewReader(saved), 0); err == nil {
		t.Fatal("load accepted an empty re-bind against a 1-effect save")
	}
	t.Log("FSV #477 load mismatch: wrong name and empty re-bind both fail closed")
}

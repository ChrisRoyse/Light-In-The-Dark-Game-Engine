package sim

// #523 save/load double-apply guard. SoT = the restored unit's Healths.Life.
//
// The hazard: the buff derived-stat cache is REBUILT on load (save.go zeros it
// then re-derives per carrier). If that rebuild ran the live current-rises delta,
// it would re-add the +max-life bonus on top of the already-saved current life —
// corrupting every damaged buffed unit across a save/load. The load path uses
// restoreBuffCache (rebuild + seed, no pool adjust) precisely to prevent this.
//
// Synthetic case (X+X=Y): unit 100/100, +50 max-life buff -> 150/150 (current
// rises), damaged to 80/150, SAVE. A correct LOAD restores exactly 80/150. A
// double-apply would show 130/150.

import (
	"bytes"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
)

func maxLifeSaveDefs() []data.BuffType {
	return []data.BuffType{
		{ID: "bigbody", DurationTicks: 10000, Stacking: data.StackRefresh, MaxStacks: 1,
			Mods: []data.StatMod{{Stat: data.StatMaxLife, Add: int64(50) << 32, Permille: 1000}}},
	}
}

func TestMaxLifeSaveLoadNoDoubleApplyFSV(t *testing.T) {
	// --- source world: buffed, damaged ---
	src := NewWorld(Caps{Units: 8})
	if !src.BindBuffTypes(maxLifeSaveDefs()) {
		t.Fatal("src BindBuffTypes failed")
	}
	id, ok := src.CreateUnit(fixed.Vec2{X: fixed.FromInt(64), Y: fixed.FromInt(64)}, 0)
	if !ok || !src.Healths.Add(src.Ents, id, fixed.FromInt(100), 0, 0, 0) {
		t.Fatal("create/health failed")
	}
	if !src.ApplyBuff(id, id, 0, 1) {
		t.Fatal("ApplyBuff failed")
	}
	src.Step() // settle to a tick boundary (drain create/apply events) so save is legal
	// current rose with the cap → full at 150.
	if got := src.Healths.Life[src.Healths.Row(id)]; got != fixed.FromInt(150) {
		t.Fatalf("pre-save: life=%v, want 150 (current-rises)", got)
	}
	// damage to 80/150.
	src.Healths.Life[src.Healths.Row(id)] = fixed.FromInt(80)
	t.Logf("BEFORE save: life=80 cap=150 (buffed, damaged)")

	var buf bytes.Buffer
	if err := src.SaveState(&buf, 0); err != nil {
		t.Fatalf("SaveState: %v", err)
	}

	// --- destination world: identical construction, load the bytes ---
	dst := NewWorld(Caps{Units: 8})
	if !dst.BindBuffTypes(maxLifeSaveDefs()) {
		t.Fatal("dst BindBuffTypes failed")
	}
	if err := dst.LoadState(bytes.NewReader(buf.Bytes()), 0); err != nil {
		t.Fatalf("LoadState: %v", err)
	}

	hr := dst.Healths.Row(id)
	if hr < 0 {
		t.Fatal("restored unit has no health row")
	}
	life := dst.Healths.Life[hr]
	cap := dst.BuffedMaxLife(id, dst.Healths.MaxLife[hr])
	t.Logf("AFTER load: life=%v cap=%v (want 80/150, NOT 130/150)", life, cap)
	if life != fixed.FromInt(80) {
		t.Fatalf("DOUBLE-APPLY: restored life=%v, want 80 (#523 load must not re-add the +50 bonus)", life)
	}
	if cap != fixed.FromInt(150) {
		t.Fatalf("restored cap=%v, want 150 (buff not restored)", cap)
	}

	// And the applied-bonus bookkeeping was seeded, so a subsequent LIVE
	// recompute (e.g. a second buff change) does NOT drift the pool.
	dst.recomputeBuffStats(id)
	if life2 := dst.Healths.Life[hr]; life2 != fixed.FromInt(80) {
		t.Fatalf("DRIFT: live recompute after load moved life %v -> %v", life, life2)
	}
	t.Logf("post-load recompute stable at 80 — applied bonus seeded correctly")
}

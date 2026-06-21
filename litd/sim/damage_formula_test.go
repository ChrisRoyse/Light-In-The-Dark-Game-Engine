package sim

// FSV for #473: the staged, overridable damage-formula pipeline. SoT = target
// HP after apply + the stage-name trace + Game state hash; override behavior is
// hand-computed (X+X=Y) and verified against the real applied damage.

import (
	"bytes"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/statehash"
)

// formulaWorld: a 1×1 coefficient matrix (coeff 1000 = 100%, so the only base
// mitigation is the armor stage) + a victim with the given armor value at full
// 100 life, and an attacker.
func formulaWorld(t *testing.T, armorValue int16) (*World, EntityID, EntityID) {
	t.Helper()
	w := NewWorld(Caps{})
	if err := w.BindDamageMatrix([][]int32{{1000}}); err != nil {
		t.Fatalf("bind matrix: %v", err)
	}
	mk := func(x int32, av int16) EntityID {
		id, ok := w.CreateUnit(fixed.Vec2{X: fixed.FromInt(x), Y: fixed.FromInt(100)}, 0)
		if !ok || !w.Healths.Add(w.Ents, id, 100*fixed.One, 0, av, 0) || !w.Combats.Add(w.Ents, id) {
			t.Fatal("spawn failed")
		}
		return id
	}
	return w, mk(100, armorValue), mk(200, 0)
}

func applyOne(w *World, src, dst EntityID, raw fixed.F64) {
	w.OnCombatPhase = func(uint32) {
		w.QueueDamage(DamagePacket{Source: src, Target: dst, Amount: raw})
		w.OnCombatPhase = nil
	}
	w.Step()
}

// TestFormulaBaseHandCheckFSV — the base pipeline reproduces coeff×armor: 100
// raw, coeff 1000‰, armor 5 → 100/(1+5·0.06) ≈ 76.9.
func TestFormulaBaseHandCheckFSV(t *testing.T) {
	w, victim, attacker := formulaWorld(t, 5)
	names := w.FormulaStageNames()
	t.Logf("FSV #473 stage trace: %v", names)
	if len(names) != 5 || names[0] != StageCoeff || names[4] != StageClamp {
		t.Fatalf("base stage order = %v, want [coeff-lookup armor-reduction handicap script-modifier clamp]", names)
	}
	// Independent expected: 100 · armorMult[5] (the same fixed-point LUT the old
	// inline path used — golden-stable, so this also proves base == pre-#473).
	want := (100 * fixed.One).Mul(armorMult[5-ArmorLUTMin])
	hr := w.Healths.Row(victim)
	before := w.Healths.Life[hr]
	applyOne(w, attacker, victim, 100*fixed.One)
	delta := before - w.Healths.Life[w.Healths.Row(victim)]
	t.Logf("FSV #473 base: armor 5, raw 100 → delta=%d (≈%d) want=%d", delta, delta/fixed.One, want)
	if delta != want {
		t.Fatalf("base delta = %d, want %d", delta, want)
	}
}

// flat50 replaces the armor stage with a flat 50% reduction.
func flat50(c *DamageCtx) { c.Amount = c.Amount.Div(fixed.FromInt(2)) }

// TestFormulaReplaceStageFSV — replace just armor-reduction with flat 50%; 100
// raw at coeff 1000 → 50.
func TestFormulaReplaceStageFSV(t *testing.T) {
	w, victim, attacker := formulaWorld(t, 5) // armor 5 now irrelevant (stage replaced)
	if err := w.ReplaceStage(StageArmor, flat50); err != nil {
		t.Fatalf("ReplaceStage: %v", err)
	}
	hr := w.Healths.Row(victim)
	before := w.Healths.Life[hr]
	applyOne(w, attacker, victim, 100*fixed.One)
	delta := before - w.Healths.Life[w.Healths.Row(victim)]
	t.Logf("FSV #473 replace armor→flat50: raw 100 → delta=%d want=%d", delta, 50*fixed.One)
	if delta != 50*fixed.One {
		t.Fatalf("flat50 delta = %d, want %d (100·coeff1000·0.5)", delta, 50*fixed.One)
	}
}

// TestFormulaReplaceWholeIdentityFSV — replace the whole formula with a raw
// passthrough; HP delta = raw.
func TestFormulaReplaceWholeIdentityFSV(t *testing.T) {
	w, victim, attacker := formulaWorld(t, 5)
	identity := func(c *DamageCtx) { c.Amount = c.Raw }
	if err := w.SetDamageFormula([]DamageStage{{"identity", identity}}); err != nil {
		t.Fatalf("SetDamageFormula: %v", err)
	}
	hr := w.Healths.Row(victim)
	before := w.Healths.Life[hr]
	applyOne(w, attacker, victim, 73*fixed.One)
	delta := before - w.Healths.Life[w.Healths.Row(victim)]
	t.Logf("FSV #473 identity formula: raw 73 → delta=%d", delta)
	if delta != 73*fixed.One {
		t.Fatalf("identity delta = %d, want raw 73", delta)
	}
}

// TestFormulaNegativeClampedFSV — a stage that drives the amount negative is
// floored at 0 (fail-closed: damage never heals), even with no clamp stage.
func TestFormulaNegativeClampedFSV(t *testing.T) {
	w, victim, attacker := formulaWorld(t, 0)
	negate := func(c *DamageCtx) { c.Amount = (-100) * fixed.One }
	if err := w.SetDamageFormula([]DamageStage{{"negate", negate}}); err != nil {
		t.Fatalf("SetDamageFormula: %v", err)
	}
	hr := w.Healths.Row(victim)
	before := w.Healths.Life[hr]
	applyOne(w, attacker, victim, 40*fixed.One)
	after := w.Healths.Life[w.Healths.Row(victim)]
	t.Logf("FSV #473 negative clamp: before=%d after=%d (no heal)", before, after)
	if after != before {
		t.Fatalf("negative amount changed HP (%d→%d) — must floor at 0", before, after)
	}
}

// TestFormulaOverrideHashesFSV — an override changes Game.StateHash (identity
// bound into the hash), while two base worlds hash identically.
func TestFormulaOverrideHashesFSV(t *testing.T) {
	reg := NewHashRegistry()
	var sa, sb, sc statehash.Snapshot
	wa, _, _ := formulaWorld(t, 0)
	wb, _, _ := formulaWorld(t, 0)
	wc, _, _ := formulaWorld(t, 0)
	if err := wc.ReplaceStage(StageArmor, flat50); err != nil {
		t.Fatalf("ReplaceStage: %v", err)
	}
	ha := wa.HashState(reg, &sa).Top
	hb := wb.HashState(reg, &sb).Top
	hc := wc.HashState(reg, &sc).Top
	t.Logf("FSV #473 hash: base=%#016x base2=%#016x override=%#016x", ha, hb, hc)
	if ha != hb {
		t.Fatalf("two base worlds diverge: %#x != %#x", ha, hb)
	}
	if hc == ha {
		t.Fatal("override did not change the state hash — identity not bound")
	}
}

// TestFormulaOverrideSaveLoadFSV — a saved override round-trips: a world that
// re-binds the same override loads + reproduces the same damage; a base world
// that did NOT re-bind fails closed.
func TestFormulaOverrideSaveLoadFSV(t *testing.T) {
	src, _, _ := formulaWorld(t, 0)
	if err := src.ReplaceStage(StageArmor, flat50); err != nil {
		t.Fatalf("ReplaceStage: %v", err)
	}
	var buf bytes.Buffer
	if err := src.SaveState(&buf, 0); err != nil {
		t.Fatalf("SaveState: %v", err)
	}
	saved := buf.Bytes()

	// Re-bound world: same override re-applied in setup → load succeeds.
	dst, victim, attacker := formulaWorld(t, 0)
	if err := dst.ReplaceStage(StageArmor, flat50); err != nil {
		t.Fatalf("re-bind ReplaceStage: %v", err)
	}
	if err := dst.LoadState(bytes.NewReader(saved), 0); err != nil {
		t.Fatalf("LoadState into re-bound world: %v", err)
	}
	hr := dst.Healths.Row(victim)
	before := dst.Healths.Life[hr]
	applyOne(dst, attacker, victim, 100*fixed.One)
	delta := before - dst.Healths.Life[dst.Healths.Row(victim)]
	t.Logf("FSV #473 save/load override: post-load raw 100 → delta=%d want=%d", delta, 50*fixed.One)
	if delta != 50*fixed.One {
		t.Fatalf("post-load delta = %d, want 50 (flat50 reproduced)", delta)
	}

	// Base world that did NOT re-bind the override → fail closed.
	base, _, _ := formulaWorld(t, 0)
	err := base.LoadState(bytes.NewReader(saved), 0)
	t.Logf("FSV #473 save/load mismatch: err=%v", err)
	if err == nil {
		t.Fatal("loading an overridden save into a base world must fail closed")
	}
}

// TestFormulaZeroAllocFSV — the pipeline runs zero-alloc at steady state
// (R-GC-1), base and overridden.
func TestFormulaZeroAllocFSV(t *testing.T) {
	w, victim, attacker := formulaWorld(t, 5)
	run := func() { _ = w.runDamageFormula(attacker, victim, 0, 0, 5, 0, 100*fixed.One) }
	run() // warm
	if a := testing.AllocsPerRun(1000, run); a != 0 {
		t.Fatalf("base pipeline allocs = %v, want 0", a)
	}
	if err := w.ReplaceStage(StageArmor, flat50); err != nil {
		t.Fatalf("ReplaceStage: %v", err)
	}
	run()
	if a := testing.AllocsPerRun(1000, run); a != 0 {
		t.Fatalf("overridden pipeline allocs = %v, want 0", a)
	}
	t.Log("FSV #473 zero-alloc: base + overridden pipeline both 0 allocs/run")
}

// TestFormulaValidationFSV — empty list, unknown stage, nil Fn all refused.
func TestFormulaValidationFSV(t *testing.T) {
	w, _, _ := formulaWorld(t, 0)
	if err := w.SetDamageFormula(nil); err == nil {
		t.Fatal("empty formula accepted")
	}
	if err := w.SetDamageFormula([]DamageStage{{"", flat50}}); err == nil {
		t.Fatal("empty stage name accepted")
	}
	if err := w.ReplaceStage("nonexistent", flat50); err == nil {
		t.Fatal("unknown stage name accepted")
	}
	if err := w.ReplaceStage(StageArmor, nil); err == nil {
		t.Fatal("nil Fn accepted")
	}
	t.Log("FSV #473 validation: empty/unknown/nil refused")
}

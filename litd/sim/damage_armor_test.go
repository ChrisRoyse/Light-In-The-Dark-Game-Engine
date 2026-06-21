package sim

// FSV for #474: armor reduction as a replaceable stage backed by a configurable
// coefficient. SoT = the per-world armor multiplier table over [-20,100] and
// real damage results; the default must reproduce the historical LUT exactly.

import (
	"bytes"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/statehash"
)

// TestArmorDefaultMatchesHistoricalLUTFSV — a default world's multiplier table
// is byte-identical to the shipped buildArmorLUTk(0.06) for all 121 values.
func TestArmorDefaultMatchesHistoricalLUTFSV(t *testing.T) {
	w := NewWorld(Caps{})
	ref := buildArmorLUTk(defaultArmorK)
	mism := 0
	for a := ArmorLUTMin; a <= ArmorLUTMax; a++ {
		if w.ArmorMultiplier(a) != ref[a-ArmorLUTMin] {
			mism++
		}
	}
	// print 5 sample rows incl. a<0
	for _, a := range []int{-20, -17, 0, 5, 100} {
		t.Logf("FSV #474 default armor[%d] = %d (≈%.4f)", a, w.ArmorMultiplier(a), float64(w.ArmorMultiplier(a))/float64(fixed.One))
	}
	if mism != 0 {
		t.Fatalf("%d of 121 armor multipliers diverge from the historical LUT", mism)
	}
	t.Log("FSV #474: all 121 default multipliers byte-identical to buildArmorLUTk(0.06)")
}

// TestArmorCoefficientHandCheckFSV — coefficient 0.10 → armor 5 multiplier
// 1/(1+5·0.10) = 1/1.5 ≈ 0.6667.
func TestArmorCoefficientHandCheckFSV(t *testing.T) {
	w := NewWorld(Caps{})
	k := fixed.FromInt(10).Div(fixed.FromInt(100)) // 0.10
	if err := w.SetArmorCoefficient(k); err != nil {
		t.Fatalf("SetArmorCoefficient: %v", err)
	}
	// independent expected: 1/(1 + 5·0.10), same fixed-point ops
	want := fixed.One.Div(fixed.One.Add(fixed.FromInt(5).Mul(k)))
	got := w.ArmorMultiplier(5)
	t.Logf("FSV #474 k=0.10 armor[5] = %d (≈%.4f) want=%d", got, float64(got)/float64(fixed.One), want)
	if got != want {
		t.Fatalf("armor[5] @k=0.10 = %d, want %d (1/1.5)", got, want)
	}
}

// TestArmorEdgesFSV — armor 0 → 1.0; armor −17 piecewise (#330); out-of-bounds
// clamps; all independent of the positive-branch coefficient.
func TestArmorEdgesFSV(t *testing.T) {
	w := NewWorld(Caps{})
	if err := w.SetArmorCoefficient(fixed.FromInt(20).Div(fixed.FromInt(100))); err != nil { // 0.20
		t.Fatalf("SetArmorCoefficient: %v", err)
	}
	// armor 0 → exactly 1.0 (1/(1+0)).
	if w.ArmorMultiplier(0) != fixed.One {
		t.Fatalf("armor[0] = %d, want 1.0", w.ArmorMultiplier(0))
	}
	// armor −17 (the #330 divergence point) → 2 − 0.94^17, independent of k.
	p94 := fixed.FromInt(94).Div(fixed.FromInt(100))
	pow := fixed.One
	for i := 0; i < 17; i++ {
		pow = pow.Mul(p94)
	}
	wantNeg := fixed.FromInt(2).Sub(pow)
	t.Logf("FSV #474 armor[-17] = %d (≈%.4f) want=%d (piecewise, k-independent)", w.ArmorMultiplier(-17), float64(w.ArmorMultiplier(-17))/float64(fixed.One), wantNeg)
	if w.ArmorMultiplier(-17) != wantNeg {
		t.Fatalf("armor[-17] = %d, want %d (#330 piecewise must survive a coefficient change)", w.ArmorMultiplier(-17), wantNeg)
	}
	// out-of-bounds clamps to the edges.
	if w.ArmorMultiplier(500) != w.ArmorMultiplier(ArmorLUTMax) {
		t.Fatal("armor 500 did not clamp to LUT max")
	}
	if w.ArmorMultiplier(-50) != w.ArmorMultiplier(ArmorLUTMin) {
		t.Fatal("armor −50 did not clamp to LUT min")
	}
	t.Log("FSV #474 edges: armor 0→1.0, −17 piecewise preserved, out-of-bounds clamped")
}

// TestArmorCoefficientDamageFSV — a real hit at armor 5 with k=0.10, coeff
// 1000‰, 100 raw → 100/1.5 ≈ 66.67 applied.
func TestArmorCoefficientDamageFSV(t *testing.T) {
	w, victim, attacker := formulaWorld(t, 5)
	k := fixed.FromInt(10).Div(fixed.FromInt(100))
	if err := w.SetArmorCoefficient(k); err != nil {
		t.Fatalf("SetArmorCoefficient: %v", err)
	}
	want := (100 * fixed.One).Mul(w.ArmorMultiplier(5))
	hr := w.Healths.Row(victim)
	before := w.Healths.Life[hr]
	applyOne(w, attacker, victim, 100*fixed.One)
	delta := before - w.Healths.Life[w.Healths.Row(victim)]
	t.Logf("FSV #474 k=0.10 damage: raw 100 armor 5 → delta=%d (≈%d) want=%d", delta, delta/fixed.One, want)
	if delta != want {
		t.Fatalf("delta = %d, want %d", delta, want)
	}
}

// TestArmorCoefficientHashAndSaveFSV — a non-default coefficient changes the
// state hash and round-trips through save/load (re-bind reproduces; a default
// world fails closed).
func TestArmorCoefficientHashAndSaveFSV(t *testing.T) {
	reg := NewHashRegistry()
	var sa, sb, sc statehash.Snapshot
	k := fixed.FromInt(10).Div(fixed.FromInt(100))

	wa := NewWorld(Caps{})            // default
	wb := NewWorld(Caps{})            // default
	wc := NewWorld(Caps{})            // override
	if err := wc.SetArmorCoefficient(k); err != nil {
		t.Fatalf("SetArmorCoefficient: %v", err)
	}
	ha := wa.HashState(reg, &sa).Top
	hb := wb.HashState(reg, &sb).Top
	hc := wc.HashState(reg, &sc).Top
	t.Logf("FSV #474 hash: default=%#016x default2=%#016x k0.10=%#016x", ha, hb, hc)
	if ha != hb {
		t.Fatalf("two default worlds diverge: %#x != %#x", ha, hb)
	}
	if hc == ha {
		t.Fatal("armor-coefficient override did not change the state hash")
	}

	// save the override, re-bind, load → reproduces.
	var buf bytes.Buffer
	if err := wc.SaveState(&buf, 0); err != nil {
		t.Fatalf("SaveState: %v", err)
	}
	saved := buf.Bytes()
	dst := NewWorld(Caps{})
	if err := dst.SetArmorCoefficient(k); err != nil {
		t.Fatalf("re-bind: %v", err)
	}
	if err := dst.LoadState(bytes.NewReader(saved), 0); err != nil {
		t.Fatalf("LoadState into re-bound world: %v", err)
	}
	if dst.ArmorMultiplier(5) != wc.ArmorMultiplier(5) {
		t.Fatal("post-load armor multiplier diverged")
	}
	// default world (no re-bind) → fail closed.
	base := NewWorld(Caps{})
	if err := base.LoadState(bytes.NewReader(saved), 0); err == nil {
		t.Fatal("loading an armor-override save into a default world must fail closed")
	}
	t.Log("FSV #474: override hashes + round-trips; default load fails closed")
}

// TestArmorCoefficientValidationFSV — non-positive coefficient refused.
func TestArmorCoefficientValidationFSV(t *testing.T) {
	w := NewWorld(Caps{})
	if err := w.SetArmorCoefficient(0); err == nil {
		t.Fatal("zero coefficient accepted")
	}
	if err := w.SetArmorCoefficient(fixed.FromInt(-1)); err == nil {
		t.Fatal("negative coefficient accepted")
	}
	t.Log("FSV #474 validation: non-positive coefficient refused")
}

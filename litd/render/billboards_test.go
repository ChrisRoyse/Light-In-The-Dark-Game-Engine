package render

import (
	"testing"

	"github.com/g3n/engine/math32"
)

// TestHealthColorRampFSV — the green→red fill ramp at known fills (X+X=Y).
func TestHealthColorRampFSV(t *testing.T) {
	cases := []struct {
		fill    float32
		r, g, b float32
	}{
		{1.0, 0, 1, 0},    // full → green
		{0.75, 0.5, 1, 0}, // green, red rising
		{0.5, 1, 1, 0},    // yellow
		{0.25, 1, 0.5, 0}, // red, green falling
		{0.0, 1, 0, 0},    // empty → red
		{-1, 1, 0, 0},     // clamp below
		{2, 0, 1, 0},      // clamp above
	}
	for _, c := range cases {
		got := HealthColor(c.fill)
		t.Logf("FSV ramp fill=%.2f -> (%.2f,%.2f,%.2f)", c.fill, got.R, got.G, got.B)
		if math32.Abs(got.R-c.r) > 1e-5 || math32.Abs(got.G-c.g) > 1e-5 || got.B != c.b || got.A != 1 {
			t.Fatalf("fill %.2f -> (%.2f,%.2f,%.2f) want (%.2f,%.2f,%.2f)", c.fill, got.R, got.G, got.B, c.r, c.g, c.b)
		}
	}
}

func TestHealthBarPoolFSV(t *testing.T) {
	p := NewHealthBarPool(3)
	// fill = hp/max, exact.
	id, ok := p.Acquire(math32.Vector3{Y: 100}, 75, 100)
	if !ok || p.At(id).Fill != 0.75 {
		t.Fatalf("acquire fill=%.3f want 0.75 ok=%v", p.At(id).Fill, ok)
	}
	if p.At(id).Color != (RGBA{0.5, 1, 0, 1}) {
		t.Fatalf("0.75 color=%+v want (0.5,1,0,1)", p.At(id).Color)
	}
	// maxHP=0 → empty, no divide-by-zero.
	id0, _ := p.Acquire(math32.Vector3{}, 50, 0)
	if p.At(id0).Fill != 0 {
		t.Fatalf("maxHP=0 fill=%.3f want 0", p.At(id0).Fill)
	}
	// over-max clamps to 1.
	id1, _ := p.Acquire(math32.Vector3{}, 999, 100)
	if p.At(id1).Fill != 1 {
		t.Fatalf("over-max fill=%.3f want 1", p.At(id1).Fill)
	}
	// pool full → deny.
	if _, ok := p.Acquire(math32.Vector3{}, 1, 1); ok {
		t.Fatal("acquire past capacity must fail")
	}
	if p.ActiveCount() != 3 {
		t.Fatalf("active=%d want 3", p.ActiveCount())
	}
	// SetFill updates fill+color uniforms in place (no rewrite).
	p.SetFill(id, 25, 100)
	if p.At(id).Fill != 0.25 || p.At(id).Color != (RGBA{1, 0.5, 0, 1}) {
		t.Fatalf("SetFill fill=%.3f color=%+v want 0.25 (1,0.5,0,1)", p.At(id).Fill, p.At(id).Color)
	}
	t.Logf("FSV bar pool: 0.75→(0.5,1,0), max0→fill0, over→1, full denied, SetFill→0.25 (1,0.5,0)")
}

func TestHealthBarZeroAllocFSV(t *testing.T) {
	p := NewHealthBarPool(500)
	for i := 0; i < 500; i++ {
		p.Acquire(math32.Vector3{Y: 10}, float32(i%100), 100)
	}
	// Steady-state SetFill churn allocates nothing.
	allocs := testing.AllocsPerRun(200, func() {
		for i := 0; i < 500; i++ {
			p.SetFill(i, float32((i*7)%100), 100)
		}
	})
	t.Logf("FSV health-bar SetFill allocs/op=%v (500 bars)", allocs)
	if allocs != 0 {
		t.Fatalf("SetFill allocates %v/op, want 0", allocs)
	}
}

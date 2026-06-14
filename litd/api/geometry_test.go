package litd

import (
	"testing"
)

// TestRectConstructorsFSV — NewRect and RectAround produce the expected
// bounds, with corner order normalized. SoT: the struct fields and the
// derived getters, checked against hand-computed values.
func TestRectConstructorsFSV(t *testing.T) {
	// Known input: corners (10,20)-(30,60). Expected min (10,20), max
	// (30,60), center (20,40), width 20, height 40.
	r := NewRect(Vec2{10, 20}, Vec2{30, 60})
	t.Logf("FSV NewRect((10,20),(30,60)) = %+v min=%v max=%v center=%v w=%.1f h=%.1f",
		r, r.Min(), r.Max(), r.Center(), r.Width(), r.Height())
	if r.Min() != (Vec2{10, 20}) || r.Max() != (Vec2{30, 60}) {
		t.Fatalf("min/max = %v/%v, want (10,20)/(30,60)", r.Min(), r.Max())
	}
	if r.Center() != (Vec2{20, 40}) {
		t.Fatalf("center = %v, want (20,40)", r.Center())
	}
	if r.Width() != 20 || r.Height() != 40 {
		t.Fatalf("w/h = %.1f/%.1f, want 20/40", r.Width(), r.Height())
	}

	// Swapped corners must normalize to the same rectangle.
	sw := NewRect(Vec2{30, 60}, Vec2{10, 20})
	t.Logf("FSV NewRect swapped corners = min=%v max=%v (want identical)", sw.Min(), sw.Max())
	if sw.Min() != r.Min() || sw.Max() != r.Max() {
		t.Fatalf("swapped-corner rect not normalized: %v/%v", sw.Min(), sw.Max())
	}

	// RectAround: center (0,0), w=100, h=40 → min (-50,-20), max (50,20).
	ra := RectAround(Vec2{0, 0}, 100, 40)
	t.Logf("FSV RectAround((0,0),100,40) = min=%v max=%v center=%v", ra.Min(), ra.Max(), ra.Center())
	if ra.Min() != (Vec2{-50, -20}) || ra.Max() != (Vec2{50, 20}) {
		t.Fatalf("RectAround bounds = %v/%v, want (-50,-20)/(50,20)", ra.Min(), ra.Max())
	}
	if ra.Center() != (Vec2{0, 0}) {
		t.Fatalf("RectAround center = %v, want (0,0)", ra.Center())
	}
	// Negative size treated as magnitude.
	if neg := RectAround(Vec2{0, 0}, -100, -40); neg.Min() != ra.Min() || neg.Max() != ra.Max() {
		t.Fatalf("RectAround negative-size = %v/%v, want same as positive", neg.Min(), neg.Max())
	}
}

// TestRectContainsEdgesFSV — Contains with the mandatory edge audit:
// interior, exact corner/edge (inclusive), just-outside, and a
// degenerate zero-area rect. SoT: the boolean for each known point.
func TestRectContainsEdgesFSV(t *testing.T) {
	r := NewRect(Vec2{0, 0}, Vec2{10, 10})
	cases := []struct {
		name string
		p    Vec2
		want bool
	}{
		{"interior", Vec2{5, 5}, true},
		{"min corner (inclusive)", Vec2{0, 0}, true},
		{"max corner (inclusive)", Vec2{10, 10}, true},
		{"edge midpoint", Vec2{0, 5}, true},
		{"just outside +x", Vec2{10.0001, 5}, false},
		{"just outside -y", Vec2{5, -0.0001}, false},
		{"far outside", Vec2{100, 100}, false},
	}
	for _, c := range cases {
		got := r.Contains(c.p)
		t.Logf("FSV Contains %-24s p=%v -> %v (want %v)", c.name, c.p, got, c.want)
		if got != c.want {
			t.Fatalf("Contains(%v) [%s] = %v, want %v", c.p, c.name, got, c.want)
		}
	}

	// Degenerate zero-area rect: only its single point is contained.
	pt := NewRect(Vec2{3, 3}, Vec2{3, 3})
	t.Logf("FSV degenerate rect w=%.0f h=%.0f contains(3,3)=%v contains(3.1,3)=%v",
		pt.Width(), pt.Height(), pt.Contains(Vec2{3, 3}), pt.Contains(Vec2{3.1, 3}))
	if !pt.Contains(Vec2{3, 3}) || pt.Contains(Vec2{3.1, 3}) {
		t.Fatalf("degenerate rect containment wrong")
	}
}

// TestRectOffsetAndVecAddFSV — Offset (MoveRectTo family) translates the
// rect as a value; Vec2.Add (OffsetLocation) translates a point. SoT:
// hand-computed translated bounds and point.
func TestRectOffsetAndVecAddFSV(t *testing.T) {
	r := NewRect(Vec2{0, 0}, Vec2{10, 10})
	moved := r.Offset(Vec2{5, -3})
	t.Logf("FSV Offset (5,-3): min %v->%v max %v->%v (original unchanged: min=%v)",
		r.Min(), moved.Min(), r.Max(), moved.Max(), r.Min())
	if moved.Min() != (Vec2{5, -3}) || moved.Max() != (Vec2{15, 7}) {
		t.Fatalf("Offset bounds = %v/%v, want (5,-3)/(15,7)", moved.Min(), moved.Max())
	}
	// Value semantics: the original is untouched.
	if r.Min() != (Vec2{0, 0}) || r.Max() != (Vec2{10, 10}) {
		t.Fatalf("Offset mutated the source rect: %v/%v", r.Min(), r.Max())
	}
	// Recentering via Offset by (newCenter - center) — the MoveRectTo idiom.
	want := Vec2{50, 50}
	rec := r.Offset(want.Sub(r.Center()))
	t.Logf("FSV recenter to (50,50): center=%v", rec.Center())
	if rec.Center() != want {
		t.Fatalf("recentered center = %v, want (50,50)", rec.Center())
	}

	// Vec2.Add (OffsetLocation): (10,20) + (3,4) = (13,24).
	if sum := (Vec2{10, 20}).Add(Vec2{3, 4}); sum != (Vec2{13, 24}) {
		t.Fatalf("Vec2.Add = %v, want (13,24)", sum)
	}
}

// TestGeometryZeroAllocFSV — R-API-2: the value-type geometry math
// allocates nothing. SoT: testing.AllocsPerRun over the constructors and
// accessors.
func TestGeometryZeroAllocFSV(t *testing.T) {
	var sink bool
	var vsink Vec2
	allocs := testing.AllocsPerRun(1000, func() {
		r := NewRect(Vec2{1, 2}, Vec2{9, 8})
		r = r.Offset(Vec2{1, 1})
		r = RectAround(r.Center(), r.Width(), r.Height())
		vsink = r.Center().Add(r.Min())
		sink = r.Contains(vsink)
	})
	t.Logf("FSV geometry allocs/run = %.2f (sink=%v vsink=%v)", allocs, sink, vsink)
	if allocs != 0 {
		t.Fatalf("geometry allocs/run = %.2f, want 0 (R-API-2)", allocs)
	}
}

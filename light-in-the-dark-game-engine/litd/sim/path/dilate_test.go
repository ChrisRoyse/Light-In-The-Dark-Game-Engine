package path

import (
	"strings"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/prng"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/statehash"
)

var groundKey = func(radiusCells int32) LayerKey {
	return LayerKey{Required: Walkable, Blocked: OccupiedStatic | OccupiedDynamic, RadiusCells: radiusCells}
}

// dumpLayer renders a window of a dilated layer: '.' = center may
// stand, 'X' = blocked for this class.
func dumpLayer(l *Layer, r Rect) string {
	var b strings.Builder
	for y := r.Y; y < r.Y+r.H; y++ {
		for x := r.X; x < r.X+r.W; x++ {
			if l.CenterClear(x, y) {
				b.WriteByte('.')
			} else {
				b.WriteByte('X')
			}
		}
		b.WriteByte('\n')
	}
	return b.String()
}

func layerHash(l *Layer) uint64 {
	h := statehash.New()
	l.HashInto(h)
	return h.Sum64()
}

// The world-radius → cell-margin table the classes bake from.
func TestDilateCellRadiusTable(t *testing.T) {
	want := map[int32]int32{8: 0, 16: 1, 24: 1, 32: 1, 48: 2}
	for r := int32(8); r <= 48; r += 8 {
		got := CellRadius(r)
		t.Logf("world radius %2d -> %d cell ring(s)", r, got)
		if w, ok := want[r]; ok && got != w {
			t.Errorf("CellRadius(%d) = %d, want %d", r, got, w)
		}
	}
}

// Edge 1: a 1-cell gap in a wall is passable for class-8 (0 rings)
// and blocked for class-16 (1 ring).
func TestDilateOneCellGapByClass(t *testing.T) {
	g := walkableGrid()
	// vertical wall at x=3, y=0..6, with a gap at y=3
	for y := int32(0); y < 7; y++ {
		if y != 3 {
			g.StampStatic(Rect{3, y, 1, 1})
		}
	}
	d := NewDilatedSet(g, []LayerKey{groundKey(CellRadius(8)), groundKey(CellRadius(16))})

	win := Rect{1, 0, 5, 7}
	t.Logf("base grid (# wall, 0 walkable):\n%s", dump(g, win))
	t.Logf("class-8 layer (0 rings):\n%s", dumpLayer(d.Layer(0), win))
	t.Logf("class-16 layer (1 ring):\n%s", dumpLayer(d.Layer(1), win))

	if !d.Layer(0).CenterClear(3, 3) {
		t.Errorf("class-8 must stand in the 1-cell gap")
	}
	if d.Layer(1).CenterClear(3, 3) {
		t.Errorf("class-16 must NOT stand in the 1-cell gap")
	}
	// (1,3) — open ground off the wall AND off the map border (border
	// cells are blocked for a 1-ring class by the edge rule)
	if !d.Layer(1).CenterClear(1, 3) {
		t.Errorf("class-16 must stand in open ground away from the wall")
	}
}

// Edge 2: stamping one cell dirties exactly the radius-margin region
// of each layer — cells outside it are untouched bits.
func TestDilateIncrementalRegionExact(t *testing.T) {
	g := walkableGrid()
	d := NewDilatedSet(g, []LayerKey{groundKey(1)}) // 1-ring class
	before := make([]uint64, bitsetWords)
	copy(before, d.Layer(0).bits)

	d.StampStatic(Rect{100, 100, 1, 1})

	var changed []int32
	for w := 0; w < bitsetWords; w++ {
		diff := before[w] ^ d.Layer(0).bits[w]
		for b := 0; b < 64; b++ {
			if diff&(1<<uint(b)) != 0 {
				changed = append(changed, int32(w*64+b))
			}
		}
	}
	t.Logf("changed cells after 1-cell stamp at (100,100), 1-ring layer:")
	for _, c := range changed {
		t.Logf("  cell (%d,%d)", c%GridSize, c/GridSize)
	}
	if len(changed) != 9 {
		t.Fatalf("expected exactly the 3x3 radius-margin region (9 cells), got %d", len(changed))
	}
	for _, c := range changed {
		x, y := c%GridSize, c/GridSize
		if x < 99 || x > 101 || y < 99 || y > 101 {
			t.Fatalf("cell (%d,%d) outside the radius margin changed", x, y)
		}
	}
}

// Edge 3: the map edge counts as blocked — a 1-ring class cannot
// stand on border cells even on a fully walkable map.
func TestDilateMapEdgeBlocked(t *testing.T) {
	g := walkableGrid()
	d := NewDilatedSet(g, []LayerKey{groundKey(1)})
	t.Logf("corner dump, 1-ring layer (X = blocked):\n%s", dumpLayer(d.Layer(0), Rect{0, 0, 5, 5}))
	if d.Layer(0).CenterClear(0, 0) || d.Layer(0).CenterClear(3, 0) || d.Layer(0).CenterClear(0, 3) {
		t.Fatalf("border cells must be blocked for a 1-ring class")
	}
	if !d.Layer(0).CenterClear(1, 1) {
		t.Fatalf("(1,1) must be clear: its full 3x3 neighborhood is on-map and walkable")
	}
}

// Edge 4: 100 seeded random stamps applied incrementally produce
// layers bit-identical to a from-scratch full recompute.
func TestDilateIncrementalMatchesFull(t *testing.T) {
	g := walkableGrid()
	keys := []LayerKey{groundKey(0), groundKey(1), groundKey(2)}
	d := NewDilatedSet(g, keys)

	rng := prng.New(0xD11A7E, 0)
	for i := 0; i < 100; i++ {
		x := int32(rng.Uint32() % (GridSize - 4))
		y := int32(rng.Uint32() % (GridSize - 4))
		w := int32(rng.Uint32()%4) + 1
		h := int32(rng.Uint32()%4) + 1
		if rng.Uint32()&1 == 0 {
			d.StampStatic(Rect{x, y, w, h})
		} else {
			d.ClearStatic(Rect{x, y, w, h})
		}
	}
	incHashes := make([]uint64, len(keys))
	for i := range keys {
		incHashes[i] = layerHash(d.Layer(i))
	}

	// from-scratch set over the same final grid state
	fresh := NewDilatedSet(g, keys)
	for i := range keys {
		fullHash := layerHash(fresh.Layer(i))
		t.Logf("layer %d (radius %d): incremental=%016x full=%016x", i, keys[i].RadiusCells, incHashes[i], fullHash)
		if incHashes[i] != fullHash {
			t.Fatalf("layer %d diverged between incremental and full recompute", i)
		}
	}
}

// Cliff legality is baked per layer: a disc may not overhang a cliff
// face — cells beside a level discontinuity are blocked for 1-ring
// classes, and a ramp restores standability.
func TestDilateCliffBaked(t *testing.T) {
	g := walkableGrid()
	for y := int32(0); y < GridSize; y++ {
		for x := int32(200); x < GridSize; x++ {
			g.SetCliffLevel(x, y, 1)
		}
	}
	g.SetRamp(200, 50, 0) // joins 0 and 1 at one spot
	d := NewDilatedSet(g, []LayerKey{groundKey(1)})

	t.Logf("cliff seam at x=200, row 10 (no ramp) layer dump x=197..203:\n%s", dumpLayer(d.Layer(0), Rect{197, 10, 7, 1}))
	t.Logf("ramp row 50 layer dump x=197..203:\n%s", dumpLayer(d.Layer(0), Rect{197, 50, 7, 1}))
	if d.Layer(0).CenterClear(199, 10) || d.Layer(0).CenterClear(200, 10) {
		t.Fatalf("cells straddling the cliff seam must be blocked for a 1-ring class")
	}
	if !d.Layer(0).CenterClear(198, 10) || !d.Layer(0).CenterClear(201, 10) {
		t.Fatalf("cells fully on one level must be clear")
	}
	if !d.Layer(0).CenterClear(200, 50) {
		t.Fatalf("the ramp cell joins both levels — a 1-ring center there must be legal")
	}
}

// Fail closed: duplicate keys and bad rects panic.
func TestDilateFailClosed(t *testing.T) {
	g := walkableGrid()
	for name, f := range map[string]func(){
		"dup-key": func() { NewDilatedSet(g, []LayerKey{groundKey(1), groundKey(1)}) },
		"bad-rect": func() {
			d := NewDilatedSet(g, []LayerKey{groundKey(1)})
			d.RecomputeRect(Rect{-1, 0, 2, 2})
		},
		"neg-radius": func() { CellRadius(-1) },
	} {
		func() {
			defer func() {
				if recover() == nil {
					t.Errorf("%s did not panic", name)
				}
			}()
			f()
		}()
	}
}

// The R-GC gate: incremental recompute after a stamp allocates zero.
func BenchmarkDilateIncremental(b *testing.B) {
	g := walkableGrid()
	d := NewDilatedSet(g, []LayerKey{groundKey(0), groundKey(1), groundKey(2)})
	b.ReportAllocs()
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		r := Rect{int32(100 + i%64), 100, 3, 3}
		d.StampStatic(r)
		d.ClearStatic(r)
	}
}

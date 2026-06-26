package path

import (
	"fmt"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/statehash"
)

// Per-collision-class dilated layers (pathfinding.md §2.2): for each
// (flag mask × collision class) combination actually present in the
// loaded roster, a bitmap marks cells where a unit CENTER of that
// class may legally stand. "May stand" is conservative: every cell
// the class's disc could touch must be flag-clear AND cliff-compatible
// with the center cell; off-map counts as blocked. This turns "can
// this unit fit through that gap" into a single bit test.

// CellRadius converts a collision radius in world units to the
// dilation margin in pathing cells (cell = 32 world units). The disc
// sits at the cell center and a boundary touch counts as crossing
// (conservative, fail closed): radius 8 → 0 rings, 16 → 1, 24/32 → 1,
// 48 → 2 — so a class-8 unit threads a 1-cell gap a class-16 cannot,
// reproducing the WC3 size-class feel.
func CellRadius(worldRadius int32) int32 {
	if worldRadius < 0 {
		panic(fmt.Sprintf("path: negative collision radius %d", worldRadius))
	}
	return (worldRadius + 16) / 32
}

// LayerKey identifies one dilated layer: the pathing-mask flag test
// (Required bits must all be set, Blocked bits must all be clear) and
// the dilation margin in cells. Layers exist only for keys passed to
// NewDilatedSet — the combos present in the roster, never the full
// cross product (§4.1).
type LayerKey struct {
	Required    Flags
	Blocked     Flags
	RadiusCells int32
}

const bitsetWords = GridSize * GridSize / 64

// Layer is one dilated bitmap: bit set = center of this class may
// stand here.
type Layer struct {
	Key  LayerKey
	bits []uint64 // 32 KB, allocated once
}

// CenterClear reports whether a unit center of this layer's class may
// stand at (x, y).
func (l *Layer) CenterClear(x, y int32) bool {
	i := idx(x, y)
	return l.bits[i>>6]&(1<<(uint(i)&63)) != 0
}

func (l *Layer) set(i int32, clear bool) {
	if clear {
		l.bits[i>>6] |= 1 << (uint(i) & 63)
	} else {
		l.bits[i>>6] &^= 1 << (uint(i) & 63)
	}
}

// HashInto writes the layer bitmap into a hasher in fixed word order
// (divergence localization for the full-vs-incremental contract).
func (l *Layer) HashInto(h *statehash.Hasher) {
	for i := 0; i < bitsetWords; i++ {
		h.WriteU64(l.bits[i])
	}
}

// DilatedSet owns every dilated layer of one match, bound to its
// Grid. All layers allocate at construction; incremental recompute
// never allocates (R-GC-1).
type DilatedSet struct {
	g      *Grid
	layers []Layer
}

// NewDilatedSet allocates one layer per key and computes them fully.
// Duplicate keys panic — two layers answering the same query would
// drift apart silently.
func NewDilatedSet(g *Grid, keys []LayerKey) *DilatedSet {
	d := &DilatedSet{g: g, layers: make([]Layer, len(keys))}
	for i, k := range keys {
		if k.RadiusCells < 0 || k.RadiusCells > GridSize {
			panic(fmt.Sprintf("path: layer radius %d out of range", k.RadiusCells))
		}
		for j := 0; j < i; j++ {
			if d.layers[j].Key == k {
				panic(fmt.Sprintf("path: duplicate layer key %+v", k))
			}
		}
		d.layers[i] = Layer{Key: k, bits: make([]uint64, bitsetWords)}
	}
	d.RecomputeAll()
	return d
}

// Layers returns the layer count (fixed after construction).
func (d *DilatedSet) Layers() int { return len(d.layers) }

// Layer returns layer i in construction order.
func (d *DilatedSet) Layer(i int) *Layer { return &d.layers[i] }

// PreallocatedBytes reports the bytes held by all layer bitmaps.
func (d *DilatedSet) PreallocatedBytes() int { return len(d.layers) * bitsetWords * 8 }

// centerClearAt evaluates the dilation predicate for one cell of one
// layer directly against the grid: every cell within the radius
// square must exist (off-map = blocked), pass the flag mask, and be
// cliff-compatible with the center (level spans intersect — a disc
// never overhangs a cliff face).
func (d *DilatedSet) centerClearAt(k LayerKey, cx, cy int32) bool {
	clo, chi := d.g.levelSpan(cx, cy)
	for dy := -k.RadiusCells; dy <= k.RadiusCells; dy++ {
		for dx := -k.RadiusCells; dx <= k.RadiusCells; dx++ {
			x, y := cx+dx, cy+dy
			if !InBounds(x, y) {
				return false
			}
			f := d.g.flags[y*GridSize+x]
			if f&k.Required != k.Required || f&k.Blocked != 0 {
				return false
			}
			lo, hi := d.g.levelSpan(x, y)
			if lo > chi || clo > hi {
				return false
			}
		}
	}
	return true
}

// RecomputeAll rebakes every layer over the whole grid (map load).
func (d *DilatedSet) RecomputeAll() {
	d.RecomputeRect(Rect{0, 0, GridSize, GridSize})
}

// RecomputeRect rebakes the cells whose dilation predicate can have
// changed when grid state inside r changed: r expanded by each
// layer's radius margin, clamped to the map. Deterministic — fixed
// cell order, no allocation.
func (d *DilatedSet) RecomputeRect(r Rect) {
	if r.W < 0 || r.H < 0 || !InBounds(r.X, r.Y) || (r.W > 0 && r.H > 0 && !InBounds(r.X+r.W-1, r.Y+r.H-1)) {
		panic(fmt.Sprintf("path: recompute rect %+v out of bounds", r))
	}
	for li := range d.layers {
		l := &d.layers[li]
		m := l.Key.RadiusCells
		x0, y0 := r.X-m, r.Y-m
		x1, y1 := r.X+r.W+m, r.Y+r.H+m
		if x0 < 0 {
			x0 = 0
		}
		if y0 < 0 {
			y0 = 0
		}
		if x1 > GridSize {
			x1 = GridSize
		}
		if y1 > GridSize {
			y1 = GridSize
		}
		for y := y0; y < y1; y++ {
			for x := x0; x < x1; x++ {
				l.set(y*GridSize+x, d.centerClearAt(l.Key, x, y))
			}
		}
	}
}

// StampStatic stamps the footprint on the grid AND incrementally
// rebakes the dilated layers around it — the §2.3 stamp path systems
// should use once a DilatedSet exists.
func (d *DilatedSet) StampStatic(r Rect) {
	d.g.StampStatic(r)
	d.RecomputeRect(r)
}

// ClearStatic clears the footprint and rebakes around it.
func (d *DilatedSet) ClearStatic(r Rect) {
	d.g.ClearStatic(r)
	d.RecomputeRect(r)
}

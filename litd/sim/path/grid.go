// Package path implements the WC3-style pathing grid (pathfinding.md
// §2, R-SIM-5). The sim sees this grid, never the terrain mesh. All
// math is integer; the grid is part of the deterministic hashed state.
package path

import (
	"fmt"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/statehash"
)

// GridSize is the v1 pathing grid side: a 128×128-terrain-cell map at
// 4× pathing resolution (pathfinding.md §2.1).
const GridSize = 512

// Flags is the per-cell flag byte (pathfinding.md §2.1 table).
type Flags uint8

const (
	Walkable        Flags = 1 << 0 // ground units may occupy
	Flyable         Flags = 1 << 1 // air units may occupy
	Buildable       Flags = 1 << 2 // structures may be placed
	Blight          Flags = 1 << 3 // faction build rules, reserved
	OccupiedStatic  Flags = 1 << 4 // stamped by building/destructable
	OccupiedDynamic Flags = 1 << 5 // reserved by a unit (§5)
)

// Cliff byte layout: bit 7 marks a ramp; bits 0–6 hold the base cliff
// level L. A ramp authored between levels L and L+1 stores L with the
// ramp bit set and therefore carries BOTH levels — to the sim a ramp
// is simply a cell that joins level L and L+1 (D-2026-06-11-7).
const (
	rampBit   = 0x80
	levelMask = 0x7F
)

// MaxCliffLevel is the highest storable base level (7 bits).
const MaxCliffLevel = levelMask - 1 // a ramp at L joins L+1, so L+1 must fit

// Grid is the pathing grid of one match: one flag byte and one cliff
// byte per cell, both allocated exactly once at map load (R-GC-2;
// 262,144 cells = 256 KB each per pathfinding.md §7).
type Grid struct {
	flags []Flags
	cliff []uint8
}

// NewGrid allocates the two 256 KB layers. Cells start zero: no flags
// set, cliff level 0, no ramps — the bake writes the real map.
func NewGrid() *Grid {
	return &Grid{
		flags: make([]Flags, GridSize*GridSize),
		cliff: make([]uint8, GridSize*GridSize),
	}
}

// PreallocatedBytes reports the bytes held by the fixed layers — the
// load-time sanity number (flags + cliff).
func (g *Grid) PreallocatedBytes() int { return len(g.flags) + len(g.cliff) }

// InBounds reports whether (x, y) is a grid cell.
func InBounds(x, y int32) bool {
	return x >= 0 && x < GridSize && y >= 0 && y < GridSize
}

// idx converts coordinates to the cell index, panicking out of bounds
// (fail closed — a pathing query outside the grid is a sim bug, never
// a condition to absorb silently).
func idx(x, y int32) int32 {
	if !InBounds(x, y) {
		panic(fmt.Sprintf("path: cell (%d,%d) out of bounds [0,%d)", x, y, GridSize))
	}
	return y*GridSize + x
}

// FlagsAt returns the flag byte of a cell.
func (g *Grid) FlagsAt(x, y int32) Flags { return g.flags[idx(x, y)] }

// SetFlags overwrites the flag byte of a cell (bake-time use).
func (g *Grid) SetFlags(x, y int32, f Flags) { g.flags[idx(x, y)] = f }

// OrFlags sets the given flag bits on a cell.
func (g *Grid) OrFlags(x, y int32, f Flags) { g.flags[idx(x, y)] |= f }

// ClearFlags clears the given flag bits on a cell.
func (g *Grid) ClearFlags(x, y int32, f Flags) { g.flags[idx(x, y)] &^= f }

// CliffLevel returns the base cliff level of a cell. For a ramp this
// is the lower of the two levels it joins.
func (g *Grid) CliffLevel(x, y int32) uint8 { return g.cliff[idx(x, y)] & levelMask }

// IsRamp reports whether the cell is a ramp (joins CliffLevel and
// CliffLevel+1).
func (g *Grid) IsRamp(x, y int32) bool { return g.cliff[idx(x, y)]&rampBit != 0 }

// SetCliffLevel bakes a plain (non-ramp) cliff level.
func (g *Grid) SetCliffLevel(x, y int32, level uint8) {
	if level > MaxCliffLevel {
		panic(fmt.Sprintf("path: cliff level %d exceeds max %d", level, MaxCliffLevel))
	}
	g.cliff[idx(x, y)] = level
}

// SetRamp bakes a ramp joining lower and lower+1.
func (g *Grid) SetRamp(x, y int32, lower uint8) {
	if lower > MaxCliffLevel {
		panic(fmt.Sprintf("path: ramp base level %d exceeds max %d", lower, MaxCliffLevel))
	}
	g.cliff[idx(x, y)] = lower | rampBit
}

// levelSpan returns the inclusive [lo, hi] cliff-level span a cell
// occupies: [L, L] for plain cells, [L, L+1] for ramps.
func (g *Grid) levelSpan(x, y int32) (lo, hi uint8) {
	c := g.cliff[idx(x, y)]
	lo = c & levelMask
	hi = lo
	if c&rampBit != 0 {
		hi = lo + 1
	}
	return lo, hi
}

// AdjacencyLegal implements the cliff rule (pathfinding.md §2.1):
// movement between two adjacent cells is legal only if they share a
// cliff level or at least one of them is a ramp joining the levels in
// question. Both clauses reduce to "the cells' level spans intersect"
// — a ramp carries {L, L+1}, a plain cell {L} — which also makes
// L→L+2 through a single ramp illegal by construction. Within-level
// height variation does not exist here at all: smooth height is
// cosmetic, only the discrete level is gameplay. Cliff faces need no
// blocking flags — they are the level inequality.
func (g *Grid) AdjacencyLegal(ax, ay, bx, by int32) bool {
	alo, ahi := g.levelSpan(ax, ay)
	blo, bhi := g.levelSpan(bx, by)
	return alo <= bhi && blo <= ahi
}

// CellWalkable reports whether a ground unit may occupy the cell:
// Walkable baked on and no static/dynamic occupancy stamp.
func (g *Grid) CellWalkable(x, y int32) bool {
	f := g.flags[idx(x, y)]
	return f&Walkable != 0 && f&(OccupiedStatic|OccupiedDynamic) == 0
}

// StepLegal is the full ground-movement test between two adjacent
// cells: destination occupiable and the cliff rule satisfied.
func (g *Grid) StepLegal(ax, ay, bx, by int32) bool {
	return g.CellWalkable(bx, by) && g.AdjacencyLegal(ax, ay, bx, by)
}

// Rect is a cell-space rectangle [X, X+W) × [Y, Y+H), the footprint
// shape buildings and destructables stamp (pathfinding.md §2.3).
type Rect struct {
	X, Y, W, H int32
}

// forEach panics if any part of the rect is out of bounds (a stamp
// half off the map is a bake bug, fail closed), then visits each cell.
func (r Rect) forEach(visit func(x, y int32)) {
	if r.W < 0 || r.H < 0 || !InBounds(r.X, r.Y) || (r.W > 0 && r.H > 0 && !InBounds(r.X+r.W-1, r.Y+r.H-1)) {
		panic(fmt.Sprintf("path: rect %+v out of bounds", r))
	}
	for y := r.Y; y < r.Y+r.H; y++ {
		for x := r.X; x < r.X+r.W; x++ {
			visit(x, y)
		}
	}
}

// StampStatic marks a footprint OccupiedStatic — building placement
// completing or a map-load destructable (tree) bake. Deterministic:
// stamping happens in the tick's pathing phase in request order.
func (g *Grid) StampStatic(r Rect) {
	r.forEach(func(x, y int32) { g.flags[idx(x, y)] |= OccupiedStatic })
}

// ClearStatic removes a footprint stamp — building death/cancel or a
// destructable destroyed (tree-cutting): the cells become occupiable
// again with no other state change.
func (g *Grid) ClearStatic(r Rect) {
	r.forEach(func(x, y int32) { g.flags[idx(x, y)] &^= OccupiedStatic })
}

// HashInto writes the grid's hashed state — the full flag layer
// (static bake + occupancy stamps) and the full cliff layer — into a
// statehash sub-hash in fixed cell order (pathfinding.md §2.1: cliff
// levels and dynamic flags are hashed state).
func (g *Grid) HashInto(h *statehash.Hasher) {
	const cells = GridSize * GridSize
	h.WriteU32(uint32(cells))
	for i := 0; i < cells; i++ {
		h.WriteU8(uint8(g.flags[i]))
	}
	for i := 0; i < cells; i++ {
		h.WriteU8(g.cliff[i])
	}
}

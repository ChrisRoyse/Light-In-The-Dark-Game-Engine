package sim

// Region store (regions-rects-locations.md; #241/#371). A region is a
// script-created set of cells on the fixed 32-wu grid (the same grid the
// pathing layer uses, path.GridSize cells per axis). It is the
// trigger-area capability: scripts add rects/cells, then test point or
// unit containment. Containment is gameplay state — a script can branch
// on it — so the store serializes and feeds the determinism hash.
//
// This file is the containment core. Enter/leave events
// (EvRegionEnter/Leave during the movement phase) are a separate
// capability tracked on #371; nothing here tracks per-unit membership.
//
// Cells: a region owns a bitset over GridSize*GridSize cells. The bitset
// is lazily allocated, so an empty region (and the common no-region map)
// costs no cell memory. Handles are (id, generation): a slot is reused
// after RemoveRegion only under a fresh generation, so a stale handle to
// a removed region is detectably invalid, never aliased (R-API-5).

import (
	"math/bits"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim/path"
)

const (
	// regionCellShift: 2^5 = 32 world units per region cell (WC3 region
	// granularity; regions-rects-locations.md hazard 2).
	regionCellShift = 5
	// regionGridSize is the region grid side (cells per axis) — the same
	// 32-wu grid the pathing layer divides the 16,384-wu world into.
	regionGridSize = path.GridSize
	// regionCellCount is the total addressable cells.
	regionCellCount = regionGridSize * regionGridSize
	// regionWorldMax is one past the last world coordinate the grid covers.
	regionWorldMax = regionGridSize << regionCellShift
)

// regionBitsetWords is the uint64 count to cover every cell.
const regionBitsetWords = (regionCellCount + 63) / 64

// regionEntry is one region slot. cells is nil until the first add.
type regionEntry struct {
	cells []uint64 // bitset over regionCellCount cells; nil = empty
	gen   uint32
	alive bool
}

// RegionStore owns all regions. Slots recycle through a free list under
// bumped generations.
type RegionStore struct {
	entries []regionEntry
	free    []uint32
}

// NewRegionStore returns an empty store.
func NewRegionStore() *RegionStore { return &RegionStore{} }

// regionCellAxis maps one world coordinate to its clamped cell axis
// index. Off-grid coordinates clamp to the border cell — they stay
// addressable rather than panicking, matching the bucket grid.
func regionCellAxis(v fixed.F64) int32 {
	c := int32(v.Floor() >> regionCellShift)
	if c < 0 {
		return 0
	}
	if c >= regionGridSize {
		return regionGridSize - 1
	}
	return c
}

// regionCellOf returns the cell index for a world position.
func regionCellOf(p fixed.Vec2) int32 {
	return regionCellAxis(p.Y)*regionGridSize + regionCellAxis(p.X)
}

// NewRegion allocates a region and returns its (id, generation).
func (s *RegionStore) NewRegion() (uint32, uint32) {
	if n := len(s.free); n > 0 {
		id := s.free[n-1]
		s.free = s.free[:n-1]
		e := &s.entries[id]
		e.alive = true
		e.cells = nil // generation was already bumped at remove
		return id, e.gen
	}
	s.entries = append(s.entries, regionEntry{gen: 1, alive: true})
	return uint32(len(s.entries) - 1), 1
}

// Remove retires a region, freeing its slot under a bumped generation so
// any outstanding handle becomes stale. False on an invalid handle.
func (s *RegionStore) Remove(id, gen uint32) bool {
	e := s.live(id, gen)
	if e == nil {
		return false
	}
	e.alive = false
	e.cells = nil
	e.gen++
	if e.gen == 0 {
		e.gen = 1
	}
	s.free = append(s.free, id)
	return true
}

// live resolves a handle to its entry, or nil when stale/out of range.
func (s *RegionStore) live(id, gen uint32) *regionEntry {
	if int(id) >= len(s.entries) {
		return nil
	}
	e := &s.entries[id]
	if !e.alive || e.gen != gen {
		return nil
	}
	return e
}

// Alive reports whether (id, gen) names a live region.
func (s *RegionStore) Alive(id, gen uint32) bool { return s.live(id, gen) != nil }

// cellsFor returns the entry's bitset, allocating it on first write.
func (e *regionEntry) cellsForWrite() []uint64 {
	if e.cells == nil {
		e.cells = make([]uint64, regionBitsetWords)
	}
	return e.cells
}

func setBit(b []uint64, cell int32)   { b[cell>>6] |= 1 << uint(cell&63) }
func clearBit(b []uint64, cell int32) { b[cell>>6] &^= 1 << uint(cell&63) }
func hasBit(b []uint64, cell int32) bool {
	return b != nil && b[cell>>6]&(1<<uint(cell&63)) != 0
}

// rectCellBounds returns the inclusive cell index ranges a world rect
// covers, clamped to the grid. ok=false when the rect is fully off-grid
// on either axis after clamping is unnecessary (it never is — clamp
// keeps at least the border cell), so ok is always true here but kept
// for symmetry with future bounds tightening.
func rectCellBounds(minx, miny, maxx, maxy fixed.F64) (cx0, cy0, cx1, cy1 int32) {
	if minx > maxx {
		minx, maxx = maxx, minx
	}
	if miny > maxy {
		miny, maxy = maxy, miny
	}
	cx0, cx1 = regionCellAxis(minx), regionCellAxis(maxx)
	cy0, cy1 = regionCellAxis(miny), regionCellAxis(maxy)
	return
}

// AddRect marks every cell overlapping the world rect as part of the
// region. No-op on an invalid handle.
func (s *RegionStore) AddRect(id, gen uint32, minx, miny, maxx, maxy fixed.F64) bool {
	e := s.live(id, gen)
	if e == nil {
		return false
	}
	b := e.cellsForWrite()
	cx0, cy0, cx1, cy1 := rectCellBounds(minx, miny, maxx, maxy)
	for cy := cy0; cy <= cy1; cy++ {
		base := cy * regionGridSize
		for cx := cx0; cx <= cx1; cx++ {
			setBit(b, base+cx)
		}
	}
	return true
}

// ClearRect removes every cell overlapping the world rect. No-op on an
// invalid handle or an empty region.
func (s *RegionStore) ClearRect(id, gen uint32, minx, miny, maxx, maxy fixed.F64) bool {
	e := s.live(id, gen)
	if e == nil || e.cells == nil {
		return false
	}
	cx0, cy0, cx1, cy1 := rectCellBounds(minx, miny, maxx, maxy)
	for cy := cy0; cy <= cy1; cy++ {
		base := cy * regionGridSize
		for cx := cx0; cx <= cx1; cx++ {
			clearBit(e.cells, base+cx)
		}
	}
	return true
}

// AddCell marks the single cell containing the world point. No-op on an
// invalid handle.
func (s *RegionStore) AddCell(id, gen uint32, p fixed.Vec2) bool {
	e := s.live(id, gen)
	if e == nil {
		return false
	}
	setBit(e.cellsForWrite(), regionCellOf(p))
	return true
}

// ClearCell removes the single cell containing the world point. No-op on
// an invalid handle or empty region.
func (s *RegionStore) ClearCell(id, gen uint32, p fixed.Vec2) bool {
	e := s.live(id, gen)
	if e == nil || e.cells == nil {
		return false
	}
	clearBit(e.cells, regionCellOf(p))
	return true
}

// ContainsPoint reports whether the world point falls in a region cell.
// False on an invalid handle.
func (s *RegionStore) ContainsPoint(id, gen uint32, p fixed.Vec2) bool {
	e := s.live(id, gen)
	if e == nil {
		return false
	}
	return hasBit(e.cells, regionCellOf(p))
}

// ContainsUnit reports whether the unit's position falls in the region.
// False on an invalid handle or a unit with no transform.
func (w *World) RegionContainsUnit(id, gen uint32, ent EntityID) bool {
	r := w.Transforms.Row(ent)
	if r < 0 {
		return false
	}
	return w.Regions.ContainsPoint(id, gen, w.Transforms.Pos[r])
}

// WorldBounds returns the playable rectangle the grid covers, in world
// units. GetWorldBounds. Deterministic, derived from the fixed grid.
func (w *World) WorldBounds() (minx, miny, maxx, maxy fixed.F64) {
	return 0, 0, fixed.F64(regionWorldMax) << 32, fixed.F64(regionWorldMax) << 32
}

// eachSetCell calls fn for every set cell index in ascending order — the
// canonical traversal for hashing and serialization (deterministic, no
// map iteration).
func (e *regionEntry) eachSetCell(fn func(cell int32)) {
	if e.cells == nil {
		return
	}
	for w := range e.cells {
		word := e.cells[w]
		for word != 0 {
			bit := word & -word
			cell := int32(w<<6) + int32(bits.TrailingZeros64(word))
			fn(cell)
			word ^= bit
		}
	}
}

// popcount returns how many cells the region contains.
func (e *regionEntry) popcount() int {
	n := 0
	for _, w := range e.cells {
		n += bits.OnesCount64(w)
	}
	return n
}

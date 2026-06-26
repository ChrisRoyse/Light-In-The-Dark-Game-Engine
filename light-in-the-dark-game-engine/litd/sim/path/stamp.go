package path

import "fmt"

// Building footprint stamping, placement validation, and cached-path
// invalidation (pathfinding.md §2.3, §6.1 step 6). Structures stamp a
// rectangular OccupiedStatic footprint when placement completes and
// clear it on death/cancel; every stamp/unstamp rebakes the dilated
// layers around the region and invalidates cached paths whose stored
// bounding box intersects it. All stamping runs in the tick's pathing
// phase in deterministic request order — these are plain methods the
// phase calls; nothing here is concurrent.

// Intersects reports whether two cell rects overlap.
func (r Rect) Intersects(o Rect) bool {
	return r.X < o.X+o.W && o.X < r.X+r.W && r.Y < o.Y+o.H && o.Y < r.Y+r.H
}

// contains reports whether the rect contains cell (x, y).
func (r Rect) contains(x, y int32) bool {
	return x >= r.X && x < r.X+r.W && y >= r.Y && y < r.Y+r.H
}

// PathID identifies one cached path: generation (top 8 bits) over
// slot index (low 24), like EntityID — a released slot's reuse bumps
// the generation so stale IDs never alias a new path.
type PathID uint32

func makePathID(gen uint8, slot int32) PathID { return PathID(uint32(gen)<<24 | uint32(slot)) }

// Slot returns the store slot index.
func (id PathID) Slot() int32 { return int32(id & 0xFFFFFF) }

// Gen returns the generation tag.
func (id PathID) Gen() uint8 { return uint8(id >> 24) }

type pathSlot struct {
	bbox   Rect
	cells  []int32 // waypoint buffer, preallocated, len trimmed in place
	gen    uint8
	live   bool
	inFree bool
}

// PathStore holds every cached path of one match in fixed-capacity
// slots: a bounding box for cheap stamp-intersection tests and a
// preallocated waypoint buffer recycled through a LIFO free pool.
// Everything allocates at construction (R-GC-2); acquire, release,
// and invalidation never allocate.
type PathStore struct {
	slots []pathSlot
	free  []int32 // LIFO: deterministic reuse order
}

// NewPathStore allocates capacity path slots with waypointCap-sized
// buffers each.
func NewPathStore(capacity, waypointCap int) *PathStore {
	if capacity <= 0 || capacity > 1<<24 || waypointCap <= 0 {
		panic(fmt.Sprintf("path: bad store dims %d/%d", capacity, waypointCap))
	}
	ps := &PathStore{
		slots: make([]pathSlot, capacity),
		free:  make([]int32, capacity),
	}
	for i := range ps.slots {
		ps.slots[i].cells = make([]int32, 0, waypointCap)
		// LIFO pop from the tail walks slots in ascending order
		ps.free[i] = int32(capacity - 1 - i)
		ps.slots[i].inFree = true
	}
	return ps
}

// Live returns the number of cached paths currently held.
func (ps *PathStore) Live() int { return len(ps.slots) - len(ps.free) }

// Acquire takes a slot for a new path with the given bounding box and
// returns its ID plus the empty waypoint buffer to fill (cap is the
// store's waypointCap). Fails when the pool is exhausted — a gameplay
// outcome, like every other fixed pool.
func (ps *PathStore) Acquire(bbox Rect) (PathID, []int32, bool) {
	if len(ps.free) == 0 {
		return 0, nil, false
	}
	slot := ps.free[len(ps.free)-1]
	ps.free = ps.free[:len(ps.free)-1]
	s := &ps.slots[slot]
	s.bbox = bbox
	s.cells = s.cells[:0]
	s.live = true
	s.inFree = false
	return makePathID(s.gen, slot), s.cells, true
}

// SetWaypoints stores the filled waypoint list (must be the slice
// returned by Acquire, appended within capacity).
func (ps *PathStore) SetWaypoints(id PathID, cells []int32) {
	s := ps.slot(id)
	if cap(cells) != cap(s.cells) {
		panic("path: SetWaypoints with a foreign buffer")
	}
	s.cells = cells
}

// Valid reports whether the ID names a live path (stale generations
// are not valid — R-API-5 semantics).
func (ps *PathStore) Valid(id PathID) bool {
	slot := id.Slot()
	if slot < 0 || int(slot) >= len(ps.slots) {
		return false
	}
	s := &ps.slots[slot]
	return s.live && s.gen == id.Gen()
}

// BBox returns the stored bounding box of a live path.
func (ps *PathStore) BBox(id PathID) Rect { return ps.slot(id).bbox }

// Waypoints returns the stored waypoint cells of a live path.
func (ps *PathStore) Waypoints(id PathID) []int32 { return ps.slot(id).cells }

func (ps *PathStore) slot(id PathID) *pathSlot {
	if !ps.Valid(id) {
		panic(fmt.Sprintf("path: stale or invalid PathID %08x", uint32(id)))
	}
	return &ps.slots[id.Slot()]
}

// Release recycles a path slot back to the pool, bumping the
// generation so the old ID goes stale. Releasing a stale ID is a
// no-op (the death-then-invalidate race resolves quietly).
func (ps *PathStore) Release(id PathID) bool {
	slot := id.Slot()
	if !ps.Valid(id) {
		return false
	}
	s := &ps.slots[slot]
	s.live = false
	s.gen++
	if !s.inFree {
		ps.free = append(ps.free, slot)
		s.inFree = true
	}
	return true
}

// InvalidateRect releases every live path whose bounding box
// intersects r, calling onInvalid with each doomed ID first (the
// movement system re-enqueues a request from the unit's current
// position; the buffer recycles to the pool). Slots are scanned in
// ascending index order — deterministic. Returns the count.
func (ps *PathStore) InvalidateRect(r Rect, onInvalid func(PathID)) int {
	n := 0
	for i := range ps.slots {
		s := &ps.slots[i]
		if !s.live || !s.bbox.Intersects(r) {
			continue
		}
		id := makePathID(s.gen, int32(i))
		if onInvalid != nil {
			onInvalid(id)
		}
		ps.Release(id)
		n++
	}
	return n
}

// Stamper binds the grid's dilated layers to the path cache: every
// footprint stamp/unstamp rebakes the dirty region and invalidates
// intersecting cached paths in one deterministic step.
type Stamper struct {
	D     *DilatedSet
	Paths *PathStore

	// OnMoveOut is the builder-vacates hook: called with the builder's
	// cell and the chosen target cell before the footprint stamps. The
	// orders system turns this into a move order (#144).
	OnMoveOut func(fromX, fromY, toX, toY int32)
	// OnInvalidate observes each cached path doomed by a stamp/clear.
	OnInvalidate func(PathID)
}

// ValidatePlacement tests a footprint against Buildable + occupancy
// (§2.3): every cell must be Buildable with no static or dynamic
// stamp. Cliff levels need no extra test here — a map bakes Buildable
// only where construction is legal.
func (s *Stamper) ValidatePlacement(r Rect) bool {
	ok := true
	r.forEach(func(x, y int32) {
		f := s.D.g.flags[y*GridSize+x]
		if f&Buildable == 0 || f&(OccupiedStatic|OccupiedDynamic) != 0 {
			ok = false
		}
	})
	return ok
}

// moveOutTarget picks the builder's vacate cell: the nearest
// CellWalkable cell outside the footprint, scanned ring by ring
// around the builder in fixed N→NE→E→SE→S→SW→W→NW-compatible order
// (row-major within each ring) — deterministic.
func (s *Stamper) moveOutTarget(r Rect, bx, by int32) (int32, int32, bool) {
	for ring := int32(1); ring < GridSize; ring++ {
		for dy := -ring; dy <= ring; dy++ {
			for dx := -ring; dx <= ring; dx++ {
				if dx > -ring && dx < ring && dy > -ring && dy < ring {
					continue // interior of ring already scanned
				}
				x, y := bx+dx, by+dy
				if !InBounds(x, y) || r.contains(x, y) {
					continue
				}
				if s.D.g.CellWalkable(x, y) {
					return x, y, true
				}
			}
		}
	}
	return 0, 0, false
}

// PlaceBuilding validates the footprint, vacates the builder if it
// stands inside, stamps OccupiedStatic, rebakes the dilated layers,
// and invalidates intersecting cached paths. Returns false (grid
// untouched) when validation fails or the builder has nowhere to go.
func (s *Stamper) PlaceBuilding(r Rect, builderX, builderY int32) bool {
	if !s.ValidatePlacement(r) {
		return false
	}
	if r.contains(builderX, builderY) {
		tx, ty, ok := s.moveOutTarget(r, builderX, builderY)
		if !ok {
			return false // sealed in — refuse the placement, fail closed
		}
		if s.OnMoveOut != nil {
			s.OnMoveOut(builderX, builderY, tx, ty)
		}
	}
	s.D.StampStatic(r)
	s.Paths.InvalidateRect(r, s.OnInvalidate)
	return true
}

// RemoveBuilding clears a footprint (death or cancel mid-build),
// rebakes, and invalidates intersecting cached paths — a cleared
// region changes optimal routes too.
func (s *Stamper) RemoveBuilding(r Rect) {
	s.D.ClearStatic(r)
	s.Paths.InvalidateRect(r, s.OnInvalidate)
}

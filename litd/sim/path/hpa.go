package path

// HPA* hierarchy (pathfinding.md §3.2, MANDATORY per D-2026-06-11-29):
// 16×16-cell sectors over the 512×512 grid (32×32 sectors), border
// portals, an abstract coarse search that yields a corridor of
// sectors, and a fine A* constrained to that corridor. The D-29 spike
// measured a flat long path at ≈5.1 ms / ~15.7k expansions — the
// 10–50× expansion cut here is what keeps worst-case requests from
// eating the whole pathing slice.
//
// Reachability: per-sector local region labels (flood fill, fixed
// scan order) + union-find over (sector, region) nodes connected by
// portals give every cell a global component label. "Unreachable" is
// answered in O(1) by comparing labels BEFORE any search; an
// unreachable goal substitutes the nearest reachable cell (ring scan,
// deterministic order — the WC3 click-on-unwalkable behavior).
//
// Determinism: same (f, h, seq) discipline as the fine A* (§4 rule
// 2) in the coarse heap; sectors, borders, portals, and flood fills
// all iterate in fixed index order. Rebuilds are incremental — only
// sectors a stamp touched (plus facing borders) recompute — and a
// full label sweep (cheap, O(used regions + portals)) follows every
// rebuild, so the labels are always a pure function of grid state.

const (
	// SectorSize is the cell width of one sector (§3.2).
	SectorSize = 16
	// SectorsPerSide = 32: SectorSize × SectorsPerSide == GridSize.
	SectorsPerSide = GridSize / SectorSize
	sectorCount    = SectorsPerSide * SectorsPerSide

	// maxPortalsPerBorder: 16 border cells alternating wall/open give
	// at most 8 maximal runs — the cap is exact, never a truncation.
	maxPortalsPerBorder = SectorSize / 2

	// regionNone marks a blocked cell in the per-cell region map.
	regionNone uint8 = 0xFF
	// maxRegions bounds local regions per sector (checkerboard worst
	// case is 128; 255 fits uint8 with regionNone reserved).
	maxRegions = 255

	// portal borders
	borderEast  = 0
	borderNorth = 1

	// substitutionRadius bounds the nearest-reachable ring scan.
	substitutionRadius = 64
)

// sectorOf returns the sector index of a cell.
func sectorOf(x, y int32) int32 { return (y/SectorSize)*SectorsPerSide + x/SectorSize }

// SectorMask is a 1,024-bit corridor set.
type SectorMask struct {
	bits [sectorCount / 64]uint64
}

func (m *SectorMask) Set(s int32)      { m.bits[s>>6] |= 1 << (uint(s) & 63) }
func (m *SectorMask) Has(s int32) bool { return m.bits[s>>6]&(1<<(uint(s)&63)) != 0 }
func (m *SectorMask) Clear() {
	for i := range m.bits {
		m.bits[i] = 0
	}
}

// Count returns the number of sectors in the mask (corridor FSV).
func (m *SectorMask) Count() int {
	n := 0
	for _, w := range m.bits {
		for ; w != 0; w &= w - 1 {
			n++
		}
	}
	return n
}

// portalSlot is one border portal: the crossing cells on each side.
// aCell is in the owning sector; bCell in the east/north neighbor.
// aCell == -1 marks an empty slot.
type portalSlot struct {
	aCell, bCell int32
}

// FindStage reports which pipeline a FindPath call took (FSV trace).
type FindStage uint8

const (
	StageUnreachable FindStage = iota // labels differ, no substitute found
	StageFineOnly                     // same sector+region: coarse skipped
	StageCoarseFine                   // coarse corridor + constrained fine
)

func (s FindStage) String() string {
	switch s {
	case StageUnreachable:
		return "unreachable"
	case StageFineOnly:
		return "fine-only"
	case StageCoarseFine:
		return "coarse+fine"
	}
	return "?"
}

// FindResult is the FSV surface of one FindPath call.
type FindResult struct {
	Stage             FindStage
	CoarseExpansions  int32
	FineExpansions    int32
	GoalX, GoalY      int32 // actual goal used (after substitution)
	Substituted       bool
	CorridorSectorCnt int
}

// HPA is the hierarchy over one (grid, layer) pair. All memory is
// allocated once in NewHPA (R-GC-2).
type HPA struct {
	g *Grid
	l *Layer

	cellRegion  []uint8 // per cell: local region id within its sector
	regionCount [sectorCount]uint8

	portals     [sectorCount][2][maxPortalsPerBorder]portalSlot
	portalCount [sectorCount][2]uint8

	// component labels per (sector, region) node; -1 = unused
	nodeLabel []int32
	ufParent  []int32 // union-find scratch, same indexing

	// coarse search scratch: 2 nodes per portal slot + virtual goal
	cG, cParent    []int32
	cStamp         []uint32
	cClosed        []bool
	cEpoch         uint32
	cHeap          []openNode
	cSeq           uint32
	corridor       SectorMask
	segMask        SectorMask
	segNodes       []int32 // forward abstract-path scratch
	coarseExpCount int32

	fine *Searcher

	// flood-fill scratch
	ffQueue [SectorSize * SectorSize]int32
}

const (
	coarseNodes  = sectorCount * 2 * maxPortalsPerBorder * 2 // 2 sides per slot
	goalNodeID   = coarseNodes                               // virtual goal
	coarseTotals = coarseNodes + 1
)

// NewHPA builds the full hierarchy for a layer. One-time cost; later
// changes go through RebuildRect.
func NewHPA(g *Grid, l *Layer, fine *Searcher) *HPA {
	if g == nil || l == nil || fine == nil {
		panic("path: NewHPA needs grid, layer, and a fine searcher")
	}
	h := &HPA{
		g:          g,
		l:          l,
		fine:       fine,
		cellRegion: make([]uint8, GridSize*GridSize),
		nodeLabel:  make([]int32, sectorCount*int(maxRegions)),
		ufParent:   make([]int32, sectorCount*int(maxRegions)),
		cG:         make([]int32, coarseTotals),
		cParent:    make([]int32, coarseTotals),
		cStamp:     make([]uint32, coarseTotals),
		cClosed:    make([]bool, coarseTotals),
		cHeap:      make([]openNode, 0, coarseNodes),
		segNodes:   make([]int32, coarseNodes),
	}
	h.RebuildAll()
	return h
}

// RebuildAll recomputes every sector's regions, every portal, and the
// component labels.
func (h *HPA) RebuildAll() {
	for s := int32(0); s < sectorCount; s++ {
		h.floodSector(s)
	}
	for s := int32(0); s < sectorCount; s++ {
		h.rebuildBorder(s, borderEast)
		h.rebuildBorder(s, borderNorth)
	}
	h.sweepLabels()
}

// RebuildRect recomputes only the sectors a stamp touched (dilated
// one cell for border effects) plus the borders facing them, then
// re-sweeps labels. Deterministic: sector index order.
func (h *HPA) RebuildRect(r Rect) {
	x0, y0 := r.X-1, r.Y-1
	x1, y1 := r.X+r.W, r.Y+r.H
	if x0 < 0 {
		x0 = 0
	}
	if y0 < 0 {
		y0 = 0
	}
	if x1 >= GridSize {
		x1 = GridSize - 1
	}
	if y1 >= GridSize {
		y1 = GridSize - 1
	}
	s0x, s0y := x0/SectorSize, y0/SectorSize
	s1x, s1y := x1/SectorSize, y1/SectorSize
	for sy := s0y; sy <= s1y; sy++ {
		for sx := s0x; sx <= s1x; sx++ {
			h.floodSector(sy*SectorsPerSide + sx)
		}
	}
	// borders touching a dirty sector: each dirty sector's own east/
	// north, plus the west/south neighbors' borders facing in.
	for sy := s0y; sy <= s1y; sy++ {
		for sx := s0x; sx <= s1x; sx++ {
			s := sy*SectorsPerSide + sx
			h.rebuildBorder(s, borderEast)
			h.rebuildBorder(s, borderNorth)
			if sx > 0 {
				h.rebuildBorder(s-1, borderEast)
			}
			if sy > 0 {
				h.rebuildBorder(s-SectorsPerSide, borderNorth)
			}
		}
	}
	h.sweepLabels()
}

// floodSector recomputes one sector's local regions: row-major scan,
// BFS flood with the same 8-connectivity + no-corner-cut rule the
// fine A* uses, restricted to the sector's cells. Region ids are
// assigned in discovery order — deterministic.
func (h *HPA) floodSector(s int32) {
	bx := (s % SectorsPerSide) * SectorSize
	by := (s / SectorsPerSide) * SectorSize
	for y := by; y < by+SectorSize; y++ {
		base := y * GridSize
		for x := bx; x < bx+SectorSize; x++ {
			h.cellRegion[base+x] = regionNone
		}
	}
	var next uint8
	capHit := false
	for y := by; y < by+SectorSize; y++ {
		for x := bx; x < bx+SectorSize; x++ {
			c := y*GridSize + x
			if h.cellRegion[c] != regionNone || !h.l.CenterClear(x, y) {
				continue
			}
			if next == maxRegions-1 {
				// cap reached: remaining cells join the last region
				// id rather than silently vanishing (255 local
				// regions needs a hand-built pathology; kept
				// pathable, possibly over-connected — never lost)
				h.cellRegion[c] = next
				capHit = true
				continue
			}
			region := next
			next++
			h.cellRegion[c] = region
			head, tail := 0, 0
			h.ffQueue[tail] = c
			tail++
			for head < tail {
				cur := h.ffQueue[head]
				head++
				cx, cy := cur%GridSize, cur/GridSize
				for i := range neighborOrder {
					d := neighborOrder[i]
					nx, ny := cx+d.dx, cy+d.dy
					if nx < bx || nx >= bx+SectorSize || ny < by || ny >= by+SectorSize {
						continue
					}
					nc := ny*GridSize + nx
					if h.cellRegion[nc] != regionNone {
						continue
					}
					if !h.stepClearLocal(cx, cy, d.dx, d.dy, bx, by) {
						continue
					}
					h.cellRegion[nc] = region
					h.ffQueue[tail] = nc
					tail++
				}
			}
		}
	}
	h.regionCount[s] = next
	if capHit {
		h.regionCount[s] = maxRegions
	}
}

// stepClearLocal is the fine A* step rule restricted to one sector
// (orthogonal corner cells outside the sector still count for the
// corner-cut test — consistency with the fine search).
func (h *HPA) stepClearLocal(x, y, dx, dy, bx, by int32) bool {
	nx, ny := x+dx, y+dy
	if !h.l.CenterClear(nx, ny) || !h.g.AdjacencyLegal(x, y, nx, ny) {
		return false
	}
	if dx != 0 && dy != 0 {
		if !h.l.CenterClear(x, ny) || !h.l.CenterClear(nx, y) {
			return false
		}
	}
	return true
}

// rebuildBorder re-extracts the maximal-run portals on one border of
// sector s. East border: cells (bx+15, by+i) vs (bx+16, by+i). North
// border: cells (bx+i, by+15) vs (bx+i, by+16). One portal per
// maximal run, crossing at the run's midpoint (WC3-ish door cells).
func (h *HPA) rebuildBorder(s int32, border int) {
	for i := range h.portals[s][border] {
		h.portals[s][border][i] = portalSlot{aCell: -1, bCell: -1}
	}
	bx := (s % SectorsPerSide) * SectorSize
	by := (s / SectorsPerSide) * SectorSize
	var n uint8
	runStart := int32(-1)
	flush := func(runEnd int32) {
		if runStart < 0 {
			return
		}
		mid := (runStart + runEnd) / 2
		var a, b int32
		if border == borderEast {
			a = (by+mid)*GridSize + bx + SectorSize - 1
			b = a + 1
		} else {
			a = (by+SectorSize-1)*GridSize + bx + mid
			b = a + GridSize
		}
		h.portals[s][border][n] = portalSlot{aCell: a, bCell: b}
		n++
		runStart = -1
	}
	edge := bx+SectorSize < GridSize
	if border == borderNorth {
		edge = by+SectorSize < GridSize
	}
	if edge {
		for i := int32(0); i < SectorSize; i++ {
			var ax, ay, bxx, byy int32
			if border == borderEast {
				ax, ay = bx+SectorSize-1, by+i
				bxx, byy = bx+SectorSize, by+i
			} else {
				ax, ay = bx+i, by+SectorSize-1
				bxx, byy = bx+i, by+SectorSize
			}
			open := h.l.CenterClear(ax, ay) && h.l.CenterClear(bxx, byy) &&
				h.g.AdjacencyLegal(ax, ay, bxx, byy)
			if open && runStart < 0 {
				runStart = i
			}
			if !open {
				flush(i - 1)
			}
		}
		flush(SectorSize - 1)
	}
	h.portalCount[s][border] = n
}

// node indexing for union-find and labels: (sector, region).
func regionNode(s int32, region uint8) int32 { return s*int32(maxRegions) + int32(region) }

func (h *HPA) ufFind(n int32) int32 {
	for h.ufParent[n] != n {
		h.ufParent[n] = h.ufParent[h.ufParent[n]] // halving
		n = h.ufParent[n]
	}
	return n
}

// sweepLabels recomputes the global component labels: init used
// (sector, region) nodes, union across every portal, then canonical
// labels in fixed node order — first root seen gets 0, next 1, …
func (h *HPA) sweepLabels() {
	for s := int32(0); s < sectorCount; s++ {
		for r := uint8(0); r < h.regionCount[s]; r++ {
			n := regionNode(s, r)
			h.ufParent[n] = n
		}
	}
	for s := int32(0); s < sectorCount; s++ {
		for b := 0; b < 2; b++ {
			cnt := h.portalCount[s][b]
			neighbor := s + 1
			if b == borderNorth {
				neighbor = s + SectorsPerSide
			}
			for i := uint8(0); i < cnt; i++ {
				p := h.portals[s][b][i]
				ra, rb := h.cellRegion[p.aCell], h.cellRegion[p.bCell]
				if ra == regionNone || rb == regionNone {
					continue
				}
				x, y := h.ufFind(regionNode(s, ra)), h.ufFind(regionNode(neighbor, rb))
				if x != y {
					if x < y { // smaller node id wins: deterministic
						h.ufParent[y] = x
					} else {
						h.ufParent[x] = y
					}
				}
			}
		}
	}
	label := int32(0)
	for s := int32(0); s < sectorCount; s++ {
		for r := uint8(0); r < h.regionCount[s]; r++ {
			n := regionNode(s, r)
			root := h.ufFind(n)
			if root == n {
				h.nodeLabel[n] = label
				label++
			}
		}
	}
	for s := int32(0); s < sectorCount; s++ {
		for r := uint8(0); r < h.regionCount[s]; r++ {
			n := regionNode(s, r)
			h.nodeLabel[n] = h.nodeLabel[h.ufFind(n)]
		}
	}
}

// Label returns the global component label of a cell, or -1 when the
// cell is blocked for this layer's class. O(1) — no search.
func (h *HPA) Label(x, y int32) int32 {
	r := h.cellRegion[idx(x, y)]
	if r == regionNone {
		return -1
	}
	return h.nodeLabel[regionNode(sectorOf(x, y), r)]
}

// Reachable answers reachability in O(1) via labels.
func (h *HPA) Reachable(sx, sy, tx, ty int32) bool {
	a, b := h.Label(sx, sy), h.Label(tx, ty)
	return a >= 0 && a == b
}

// NearestReachable ring-scans outward from (tx, ty) for the closest
// cell whose label matches want. Scan order per ring: top row west→
// east, bottom row west→east, west column south→north, east column
// south→north — fixed, deterministic. Radius capped (WC3 gives up
// too).
func (h *HPA) NearestReachable(want int32, tx, ty int32) (int32, int32, bool) {
	if h.Label(tx, ty) == want {
		return tx, ty, true
	}
	for r := int32(1); r <= substitutionRadius; r++ {
		for x := tx - r; x <= tx+r; x++ {
			if InBounds(x, ty+r) && h.Label(x, ty+r) == want {
				return x, ty + r, true
			}
		}
		for x := tx - r; x <= tx+r; x++ {
			if InBounds(x, ty-r) && h.Label(x, ty-r) == want {
				return x, ty - r, true
			}
		}
		for y := ty - r + 1; y <= ty+r-1; y++ {
			if InBounds(tx-r, y) && h.Label(tx-r, y) == want {
				return tx - r, y, true
			}
		}
		for y := ty - r + 1; y <= ty+r-1; y++ {
			if InBounds(tx+r, y) && h.Label(tx+r, y) == want {
				return tx + r, y, true
			}
		}
	}
	return 0, 0, false
}

// coarse node helpers: node = (slotIndex*2 + side); slotIndex =
// (sector*2 + border)*maxPortalsPerBorder + i.
func (h *HPA) nodeCell(n int32) int32 {
	slot := n / 2
	side := n % 2
	i := slot % maxPortalsPerBorder
	rest := slot / maxPortalsPerBorder
	b := rest % 2
	s := rest / 2
	p := h.portals[s][b][i]
	if side == 0 {
		return p.aCell
	}
	return p.bCell
}

func coarseNodeID(s int32, border int, i uint8, side int32) int32 {
	return ((s*2+int32(border))*maxPortalsPerBorder+int32(i))*2 + side
}

// forEachNodeInSector visits every portal-side node whose cell lies
// in sector s, in fixed order: own east A-sides, own north A-sides,
// west neighbor's east B-sides, south neighbor's north B-sides.
func (h *HPA) forEachNodeInSector(s int32, visit func(node, cell int32)) {
	for i := uint8(0); i < h.portalCount[s][borderEast]; i++ {
		visit(coarseNodeID(s, borderEast, i, 0), h.portals[s][borderEast][i].aCell)
	}
	for i := uint8(0); i < h.portalCount[s][borderNorth]; i++ {
		visit(coarseNodeID(s, borderNorth, i, 0), h.portals[s][borderNorth][i].aCell)
	}
	if s%SectorsPerSide > 0 {
		w := s - 1
		for i := uint8(0); i < h.portalCount[w][borderEast]; i++ {
			visit(coarseNodeID(w, borderEast, i, 1), h.portals[w][borderEast][i].bCell)
		}
	}
	if s >= SectorsPerSide {
		d := s - SectorsPerSide
		for i := uint8(0); i < h.portalCount[d][borderNorth]; i++ {
			visit(coarseNodeID(d, borderNorth, i, 1), h.portals[d][borderNorth][i].bCell)
		}
	}
}

func (h *HPA) cTouch(n int32) {
	if h.cStamp[n] != h.cEpoch {
		h.cStamp[n] = h.cEpoch
		h.cG[n] = 1<<31 - 1
		h.cParent[n] = -1
		h.cClosed[n] = false
	}
}

func (h *HPA) cPush(n openNode) bool {
	if len(h.cHeap) == cap(h.cHeap) {
		return false
	}
	h.cHeap = append(h.cHeap, n)
	i := len(h.cHeap) - 1
	for i > 0 {
		p := (i - 1) / 2
		if !keyLess(&h.cHeap[i], &h.cHeap[p]) {
			break
		}
		h.cHeap[i], h.cHeap[p] = h.cHeap[p], h.cHeap[i]
		i = p
	}
	return true
}

func (h *HPA) cPop() openNode {
	top := h.cHeap[0]
	last := len(h.cHeap) - 1
	h.cHeap[0] = h.cHeap[last]
	h.cHeap = h.cHeap[:last]
	i := 0
	for {
		l, r := 2*i+1, 2*i+2
		m := i
		if l < last && keyLess(&h.cHeap[l], &h.cHeap[m]) {
			m = l
		}
		if r < last && keyLess(&h.cHeap[r], &h.cHeap[m]) {
			m = r
		}
		if m == i {
			break
		}
		h.cHeap[i], h.cHeap[m] = h.cHeap[m], h.cHeap[i]
		i = m
	}
	return top
}

// cRelax pushes a neighbor if it improves. Returns false only on
// heap-arena exhaustion (fail closed).
func (h *HPA) cRelax(n int32, ng int32, hx, hy, tx, ty int32, parent int32) bool {
	h.cTouch(n)
	if h.cClosed[n] || ng >= h.cG[n] {
		return true
	}
	h.cG[n] = ng
	h.cParent[n] = parent
	nh := Octile(hx, hy, tx, ty)
	ok := h.cPush(openNode{f: ng + nh, h: nh, seq: h.cSeq, cell: n})
	h.cSeq++
	return ok
}

// coarseSearch runs the abstract A* from (sx,sy) to (tx,ty), both
// known reachable, filling h.corridor with the sectors of every node
// on the abstract path plus the start/goal sectors. Returns false on
// arena exhaustion only.
func (h *HPA) coarseSearch(sx, sy, tx, ty int32) bool {
	h.cEpoch++
	h.cSeq = 0
	h.cHeap = h.cHeap[:0]
	h.coarseExpCount = 0

	startSec, goalSec := sectorOf(sx, sy), sectorOf(tx, ty)
	startRegion := h.cellRegion[idx(sx, sy)]
	goalRegion := h.cellRegion[idx(tx, ty)]

	ok := true
	h.forEachNodeInSector(startSec, func(n, cell int32) {
		if h.cellRegion[cell] != startRegion {
			return
		}
		cx, cy := cell%GridSize, cell/GridSize
		if !h.cRelax(n, Octile(sx, sy, cx, cy), cx, cy, tx, ty, -1) {
			ok = false
		}
	})
	if !ok {
		return false
	}

	for len(h.cHeap) > 0 {
		top := h.cPop()
		n := top.cell
		if h.cClosed[n] {
			continue
		}
		h.cClosed[n] = true
		h.coarseExpCount++
		if n == goalNodeID {
			h.buildCorridor(startSec, goalSec)
			return true
		}
		cell := h.nodeCell(n)
		cx, cy := cell%GridSize, cell/GridSize
		g := h.cG[n]
		// cross edge: the twin side of the same portal slot
		twin := n ^ 1
		tc := h.nodeCell(twin)
		if !h.cRelax(twin, g+CostCardinal, tc%GridSize, tc/GridSize, tx, ty, n) {
			return false
		}
		// intra-sector edges: every node in this cell's sector with
		// the same local region
		sec := sectorOf(cx, cy)
		region := h.cellRegion[cell]
		ok = true
		h.forEachNodeInSector(sec, func(m, mcell int32) {
			if m == n || h.cellRegion[mcell] != region {
				return
			}
			mx, my := mcell%GridSize, mcell/GridSize
			if !h.cRelax(m, g+Octile(cx, cy, mx, my), mx, my, tx, ty, n) {
				ok = false
			}
		})
		if !ok {
			return false
		}
		// goal connection
		if sec == goalSec && region == goalRegion {
			if !h.cRelax(goalNodeID, g+Octile(cx, cy, tx, ty), tx, ty, tx, ty, n) {
				return false
			}
		}
	}
	// labels said reachable, so the abstract graph must reach too;
	// drained heap here means inconsistent state — fail closed.
	return false
}

// buildCorridor marks the sectors of every abstract-path node (both
// portal sides) plus start and goal sectors.
func (h *HPA) buildCorridor(startSec, goalSec int32) {
	h.corridor.Clear()
	h.corridor.Set(startSec)
	h.corridor.Set(goalSec)
	for n := h.cParent[goalNodeID]; n != -1; n = h.cParent[n] {
		cell := h.nodeCell(n)
		h.corridor.Set(sectorOf(cell%GridSize, cell/GridSize))
	}
}

// FindPath is the full HPA* pipeline: O(1) label check (+ nearest-
// reachable substitution), coarse corridor when start and goal sit
// in different sector regions, then corridor-constrained fine A*.
// Appends waypoints to out exactly like Searcher.Search.
func (h *HPA) FindPath(sx, sy, tx, ty int32, out []int32) ([]int32, FindResult, bool) {
	var res FindResult
	res.GoalX, res.GoalY = tx, ty
	if !InBounds(sx, sy) || !InBounds(tx, ty) {
		return out, res, false
	}
	startLabel := h.Label(sx, sy)
	if startLabel < 0 {
		return out, res, false
	}
	if h.Label(tx, ty) != startLabel {
		nx, ny, ok := h.NearestReachable(startLabel, tx, ty)
		if !ok {
			res.Stage = StageUnreachable
			return out, res, false
		}
		tx, ty = nx, ny
		res.GoalX, res.GoalY = nx, ny
		res.Substituted = true
	}
	startSec, goalSec := sectorOf(sx, sy), sectorOf(tx, ty)
	if startSec == goalSec && h.cellRegion[idx(sx, sy)] == h.cellRegion[idx(tx, ty)] {
		res.Stage = StageFineOnly
		h.corridor.Clear()
		h.corridor.Set(startSec)
		res.CorridorSectorCnt = 1
		out, ok := h.fine.SearchCorridor(h.l, &h.corridor, sx, sy, tx, ty, out)
		res.FineExpansions = h.fine.Expansions()
		return out, res, ok
	}
	res.Stage = StageCoarseFine
	if !h.coarseSearch(sx, sy, tx, ty) {
		res.CoarseExpansions = h.coarseExpCount
		return out, res, false
	}
	res.CoarseExpansions = h.coarseExpCount
	res.CorridorSectorCnt = h.corridor.Count()

	// segment-by-segment refinement — the actual HPA* expansion cut.
	// The abstract path alternates portal-side nodes; consecutive
	// crossing cells are cardinally adjacent (appended directly) and
	// every other hop stays inside ONE sector, so each fine search is
	// confined to a ≤2-sector corridor instead of the whole route.
	count := 0
	for n := h.cParent[goalNodeID]; n != -1; n = h.cParent[n] {
		h.segNodes[count] = n
		count++
	}
	px, py := sx, sy
	for i := count - 1; i >= 0; i-- {
		cell := h.nodeCell(h.segNodes[i])
		cx, cy := cell%GridSize, cell/GridSize
		var ok bool
		out, ok = h.refineSegment(px, py, cx, cy, out, &res)
		if !ok {
			return out, res, false
		}
		px, py = cx, cy
	}
	out, ok := h.refineSegment(px, py, tx, ty, out, &res)
	return out, res, ok
}

// refineSegment appends the path from (px,py) to (cx,cy): adjacent
// cells append directly (portal crossings), anything longer runs a
// fine search confined to the two endpoint sectors.
func (h *HPA) refineSegment(px, py, cx, cy int32, out []int32, res *FindResult) ([]int32, bool) {
	if px == cx && py == cy {
		return out, true
	}
	dx, dy := cx-px, cy-py
	if dx >= -1 && dx <= 1 && dy >= -1 && dy <= 1 {
		return append(out, idx(cx, cy)), true
	}
	h.segMask.Clear()
	h.segMask.Set(sectorOf(px, py))
	h.segMask.Set(sectorOf(cx, cy))
	out, ok := h.fine.SearchCorridor(h.l, &h.segMask, px, py, cx, cy, out)
	res.FineExpansions += h.fine.Expansions()
	return out, ok
}

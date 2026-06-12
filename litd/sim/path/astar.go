package path

// Deterministic grid A* (pathfinding.md §4, D-2026-06-11-29). Every
// equal-cost choice breaks identically everywhere:
//
//  1. integer costs only — cardinal 10, diagonal 14, octile heuristic
//     in the same units (admissible); no float sqrt(2) anywhere
//  2. total ordering on the open list — (f, h, insertionSeq) compared
//     lexicographically; the heap compares the full tuple and never
//     relies on heap-internal stability
//  3. fixed neighbor expansion order N, NE, E, SE, S, SW, W, NW; no
//     corner-cutting (both orthogonals must be CenterClear)
//  4. per-search epoch stamps on the flat scratch arrays — "reset" is
//     a counter increment, never a 512×512 clear
//
// The chosen path is therefore a pure function of (grid, layer,
// request): identical expansion counts and waypoints across runs and
// machines.

// Cost units: the classic ×10 octile approximation.
const (
	CostCardinal int32 = 10
	CostDiagonal int32 = 14
)

// neighborOrder is the mandated compass order. North is +Y.
var neighborOrder = [8]struct{ dx, dy int32 }{
	{0, 1},   // N
	{1, 1},   // NE
	{1, 0},   // E
	{1, -1},  // SE
	{0, -1},  // S
	{-1, -1}, // SW
	{-1, 0},  // W
	{-1, 1},  // NW
}

// openNode is one heap entry: the §4 pqKey plus its cell.
type openNode struct {
	f, h int32
	seq  uint32
	cell int32
}

// keyLess is the total order — no two keys ever compare equal because
// seq is a monotone per-search counter.
func keyLess(a, b *openNode) bool {
	if a.f != b.f {
		return a.f < b.f
	}
	if a.h != b.h {
		return a.h < b.h
	}
	return a.seq < b.seq
}

// openArenaCap bounds one search's open list (overflow fails the
// search closed — deterministic, never grows).
const openArenaCap = 1 << 16

// Searcher is one concurrent search slot (§7 preallocates two). All
// scratch is allocated once; nothing is cleared between searches.
type Searcher struct {
	g *Grid

	gCost  []int32  // per-cell best g, valid only when stamp matches
	parent []int32  // per-cell predecessor, valid only when stamp matches
	stamp  []uint32 // per-cell epoch of last touch
	closed []bool   // per-cell closed marker, valid only when stamp matches
	epoch  uint32   // current search epoch

	heap []openNode // fixed-cap binary heap arena
	seq  uint32     // insertion counter, reset per search

	expansions int32 // last search's expansion count (FSV surface)
}

// NewSearcher allocates one search slot's scratch (≈1.2 MB arrays +
// 1 MB open arena), exactly once.
func NewSearcher(g *Grid) *Searcher {
	if g == nil {
		panic("path: NewSearcher needs a grid")
	}
	cells := int32(GridSize * GridSize)
	return &Searcher{
		g:      g,
		gCost:  make([]int32, cells),
		parent: make([]int32, cells),
		stamp:  make([]uint32, cells),
		closed: make([]bool, cells),
		heap:   make([]openNode, 0, openArenaCap),
	}
}

// Expansions reports the node-expansion count of the last Search —
// the determinism fingerprint (identical across runs by contract).
func (s *Searcher) Expansions() int32 { return s.expansions }

// Octile returns the admissible integer heuristic between two cells.
func Octile(ax, ay, bx, by int32) int32 {
	dx, dy := ax-bx, ay-by
	if dx < 0 {
		dx = -dx
	}
	if dy < 0 {
		dy = -dy
	}
	if dx < dy {
		dx, dy = dy, dx
	}
	return CostDiagonal*dy + CostCardinal*(dx-dy)
}

// touch initializes a cell's scratch on first visit this epoch.
func (s *Searcher) touch(c int32) {
	if s.stamp[c] != s.epoch {
		s.stamp[c] = s.epoch
		s.gCost[c] = 1<<31 - 1
		s.parent[c] = -1
		s.closed[c] = false
	}
}

func (s *Searcher) push(n openNode) bool {
	if len(s.heap) == cap(s.heap) {
		return false
	}
	s.heap = append(s.heap, n)
	i := len(s.heap) - 1
	for i > 0 {
		p := (i - 1) / 2
		if !keyLess(&s.heap[i], &s.heap[p]) {
			break
		}
		s.heap[i], s.heap[p] = s.heap[p], s.heap[i]
		i = p
	}
	return true
}

func (s *Searcher) pop() openNode {
	top := s.heap[0]
	last := len(s.heap) - 1
	s.heap[0] = s.heap[last]
	s.heap = s.heap[:last]
	i := 0
	for {
		l, r := 2*i+1, 2*i+2
		min := i
		if l < last && keyLess(&s.heap[l], &s.heap[min]) {
			min = l
		}
		if r < last && keyLess(&s.heap[r], &s.heap[min]) {
			min = r
		}
		if min == i {
			break
		}
		s.heap[i], s.heap[min] = s.heap[min], s.heap[i]
		i = min
	}
	return top
}

// stepClear reports whether a unit of l's class may step from (x, y)
// to the neighbor cell: in bounds, center clear, cliff-adjacent, and
// for diagonals both orthogonal cells clear (no corner-cutting).
func (s *Searcher) stepClear(l *Layer, x, y, dx, dy int32) bool {
	nx, ny := x+dx, y+dy
	if !InBounds(nx, ny) {
		return false
	}
	if !l.CenterClear(nx, ny) || !s.g.AdjacencyLegal(x, y, nx, ny) {
		return false
	}
	if dx != 0 && dy != 0 {
		if !l.CenterClear(x, ny) || !l.CenterClear(nx, y) {
			return false
		}
	}
	return true
}

// Search finds the cheapest path from (sx, sy) to (tx, ty) on layer
// l, appending waypoint cell indices (start exclusive, goal
// inclusive) to out. Returns the extended slice and whether a path
// exists. goal == start returns (out, true) with zero waypoints and
// zero expansions. Failure modes — blocked endpoint, unreachable
// goal, open-arena overflow — are deterministic.
func (s *Searcher) Search(l *Layer, sx, sy, tx, ty int32, out []int32) ([]int32, bool) {
	s.expansions = 0
	if !InBounds(sx, sy) || !InBounds(tx, ty) {
		return out, false
	}
	if !l.CenterClear(sx, sy) || !l.CenterClear(tx, ty) {
		return out, false
	}
	start, goal := idx(sx, sy), idx(tx, ty)
	if start == goal {
		return out, true
	}

	s.epoch++
	s.seq = 0
	s.heap = s.heap[:0]

	s.touch(start)
	s.gCost[start] = 0
	h0 := Octile(sx, sy, tx, ty)
	s.push(openNode{f: h0, h: h0, seq: s.seq, cell: start})
	s.seq++

	for len(s.heap) > 0 {
		n := s.pop()
		c := n.cell
		if s.closed[c] {
			continue // stale entry superseded by a cheaper push
		}
		s.closed[c] = true
		s.expansions++
		if c == goal {
			return s.reconstruct(start, goal, out), true
		}
		x, y := c%GridSize, c/GridSize
		g := s.gCost[c]
		for i := range neighborOrder {
			d := neighborOrder[i]
			if !s.stepClear(l, x, y, d.dx, d.dy) {
				continue
			}
			nc := idx(x+d.dx, y+d.dy)
			s.touch(nc)
			if s.closed[nc] {
				continue
			}
			step := CostCardinal
			if d.dx != 0 && d.dy != 0 {
				step = CostDiagonal
			}
			ng := g + step
			if ng >= s.gCost[nc] {
				continue
			}
			s.gCost[nc] = ng
			s.parent[nc] = c
			nh := Octile(x+d.dx, y+d.dy, tx, ty)
			if !s.push(openNode{f: ng + nh, h: nh, seq: s.seq, cell: nc}) {
				return out, false // arena exhausted: fail closed
			}
			s.seq++
		}
	}
	return out, false // open list drained: unreachable
}

// reconstruct walks parents goal→start, then reverses in place so
// out is start-exclusive, goal-inclusive walking order.
func (s *Searcher) reconstruct(start, goal int32, out []int32) []int32 {
	base := len(out)
	for c := goal; c != start; c = s.parent[c] {
		out = append(out, c)
	}
	for i, j := base, len(out)-1; i < j; i, j = i+1, j-1 {
		out[i], out[j] = out[j], out[i]
	}
	return out
}

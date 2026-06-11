// Spike S3 (decision D-2026-06-11-18 validation): pathfinding cost at
// 1,000 units on a 512×512 WC3-style grid. Answers: does plain A* with
// deterministic tie-breaking fit the tick budget at scale, and how much
// headroom do HPA*/path-sharing/flow-fields need to buy?
package pathfind

import (
	"container/heap"
	"testing"
)

const gridN = 512

type Grid struct {
	blocked [gridN * gridN]bool
}

// deterministic obstacle field (~20% blocked) from a tiny PRNG
func newGrid() *Grid {
	g := &Grid{}
	state := uint64(0x9E3779B97F4A7C15)
	for i := range g.blocked {
		state = state*6364136223846793005 + 1442695040888963407
		g.blocked[i] = (state>>33)%5 == 0
	}
	return g
}

type node struct {
	idx     int32
	f, h    int32
	seq     int32 // insertion order: deterministic tie-break (f, h, seq)
	heapIdx int
}

type pq []*node

func (p pq) Len() int { return len(p) }
func (p pq) Less(i, j int) bool {
	a, b := p[i], p[j]
	if a.f != b.f {
		return a.f < b.f
	}
	if a.h != b.h {
		return a.h < b.h
	}
	return a.seq < b.seq
}
func (p pq) Swap(i, j int) {
	p[i], p[j] = p[j], p[i]
	p[i].heapIdx, p[j].heapIdx = i, j
}
func (p *pq) Push(x any) { n := x.(*node); n.heapIdx = len(*p); *p = append(*p, n) }
func (p *pq) Pop() any   { old := *p; n := old[len(old)-1]; *p = old[:len(old)-1]; return n }

type astar struct {
	g       *Grid
	gScore  []int32
	visited []int32 // generation-stamped to avoid clearing
	gen     int32
	open    pq
	nodes   []node
}

func newAstar(g *Grid) *astar {
	return &astar{
		g:       g,
		gScore:  make([]int32, gridN*gridN),
		visited: make([]int32, gridN*gridN),
		nodes:   make([]node, 0, 1<<16),
	}
}

func abs32(v int32) int32 {
	if v < 0 {
		return -v
	}
	return v
}

func octile(a, b int32) int32 {
	ax, ay := a%gridN, a/gridN
	bx, by := b%gridN, b/gridN
	dx, dy := abs32(ax-bx), abs32(ay-by)
	if dx < dy {
		dx, dy = dy, dx
	}
	return 10*dx + 4*dy // 10/14 integer costs
}

var dirs = [8][3]int32{ // fixed compass order: deterministic expansion
	{0, -1, 10}, {1, 0, 10}, {0, 1, 10}, {-1, 0, 10},
	{1, -1, 14}, {1, 1, 14}, {-1, 1, 14}, {-1, -1, 14},
}

// find runs A* start→goal; returns expansions (path itself not materialized).
func (a *astar) find(start, goal int32) (expansions int, found bool) {
	a.gen++
	a.open = a.open[:0]
	a.nodes = a.nodes[:0]
	var seq int32
	a.nodes = append(a.nodes, node{idx: start, h: octile(start, goal)})
	n0 := &a.nodes[0]
	n0.f = n0.h
	heap.Push(&a.open, n0)
	a.gScore[start] = 0
	a.visited[start] = a.gen

	for a.open.Len() > 0 {
		cur := heap.Pop(&a.open).(*node)
		expansions++
		if cur.idx == goal {
			return expansions, true
		}
		cx, cy := cur.idx%gridN, cur.idx/gridN
		cg := a.gScore[cur.idx]
		for _, d := range dirs {
			nx, ny := cx+d[0], cy+d[1]
			if nx < 0 || ny < 0 || nx >= gridN || ny >= gridN {
				continue
			}
			ni := ny*gridN + nx
			if a.g.blocked[ni] {
				continue
			}
			ng := cg + d[2]
			if a.visited[ni] == a.gen && a.gScore[ni] <= ng {
				continue
			}
			a.visited[ni] = a.gen
			a.gScore[ni] = ng
			seq++
			a.nodes = append(a.nodes, node{idx: ni, h: octile(ni, goal), seq: seq})
			nn := &a.nodes[len(a.nodes)-1]
			nn.f = ng + nn.h
			heap.Push(&a.open, nn)
		}
		if expansions > 60000 { // per-request expansion cap (counted, not wall-clock)
			return expansions, false
		}
	}
	return expansions, false
}

// deterministic request set: 1,000 unit start/goal pairs spread across the map
func requests(n int) [][2]int32 {
	reqs := make([][2]int32, n)
	state := uint64(42)
	next := func() int32 {
		state = state*6364136223846793005 + 1442695040888963407
		return int32((state >> 33) % (gridN * gridN))
	}
	for i := range reqs {
		reqs[i] = [2]int32{next(), next()}
	}
	return reqs
}

// TestDeterministicExpansions: same request set → same expansion counts.
func TestDeterministicExpansions(t *testing.T) {
	g := newGrid()
	a1, a2 := newAstar(g), newAstar(g)
	for _, r := range requests(50) {
		e1, _ := a1.find(r[0], r[1])
		e2, _ := a2.find(r[0], r[1])
		if e1 != e2 {
			t.Fatalf("nondeterministic expansions: %d vs %d", e1, e2)
		}
	}
}

// BenchmarkRepath1000 measures the nightmare scenario: all 1,000 units
// request a full repath in one tick (real game amortizes over many ticks).
func BenchmarkRepath1000(b *testing.B) {
	g := newGrid()
	a := newAstar(g)
	reqs := requests(1000)
	b.ReportAllocs()
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		total := 0
		for _, r := range reqs {
			e, _ := a.find(r[0], r[1])
			total += e
		}
		b.ReportMetric(float64(total)/1000, "expansions/path")
	}
}

// BenchmarkSinglePath measures one long-distance path request.
func BenchmarkSinglePath(b *testing.B) {
	g := newGrid()
	a := newAstar(g)
	start, goal := int32(gridN+1), int32(gridN*gridN-gridN-2)
	b.ReportAllocs()
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		a.find(start, goal)
	}
}

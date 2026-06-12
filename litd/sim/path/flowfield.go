package path

// Flow-field backend (pathfinding.md §3.2 — committed v1 per
// D-2026-06-11-29): serves shared-goal moves of ≥ ~40 units, where
// one field amortizes over the whole blob and per-unit searches
// cannot. One direction byte per cell × 512×512 = 256 KB per field;
// EXACTLY four preallocated slots (§7 reserves 1 MB), recycled
// least-recently-used by a monotone use counter — the counter is sim
// state, so the recycle choice is a deterministic function of sim
// state, never of wall-clock or map order.
//
// Determinism (§4 rule 5): integration is a Dijkstra from the goal
// under the same 10/14 integer costs and a (cost, seq) total-order
// heap; the direction assignment then runs as a FIXED row-major
// sweep over the final integration costs, ties broken by the fixed
// compass neighbor order. Same grid + same goal ⇒ byte-identical
// field, on every machine, every run.

// Direction encoding: 0 = none (unreachable cell, or the goal
// itself — units there stop or fall back to nearest-reachable);
// 1..8 = one step in neighborOrder (N, NE, E, SE, S, SW, W, NW).
const DirNone uint8 = 0

// FlowSlots is the fixed slot count (§7: 4 × 256 KB).
const FlowSlots = 4

// DefaultFlowThreshold: shared-goal groups at or above this size use
// the flow field; below it, A*+sharing. Data, sim semantics (D-29,
// tuned in M3).
const DefaultFlowThreshold = 40

// DefaultIntegrationBudget is the default Pump expansion budget:
// ~8k heap pops ≈ 1.25 ms on the #291 reference hardware, inside the
// pathfinding.md §6 ≤2 ms tick slice. M3 tunes it (#202).
const DefaultIntegrationBudget = 8192

type flowSlot struct {
	dirs    []uint8 // 256 KB direction field
	goal    int32   // goal cell, -1 when empty
	live    bool
	ready   bool   // dirs hold a COMPLETE integration for goal (#291)
	dirty   bool   // needs (re-)integration (#291 budgeted mode)
	lastUse uint64 // monotone use counter value at last Acquire
}

// FlowSet owns the four field slots over one (grid, layer) pair plus
// the shared integration scratch. Everything allocates once.
type FlowSet struct {
	g *Grid
	l *Layer

	slots [FlowSlots]flowSlot

	integCost  []int32
	integStamp []uint32
	epoch      uint32
	heap       []openNode
	seq        uint32

	// budgeted integration (#291): one in-flight Dijkstra writes into
	// staging; the slot's previous field keeps serving until the copy
	// at completion — stale but self-consistent, never half-written
	staging  []uint8
	inflight int8 // slot index, -1 = none

	useCounter uint64
}

// NewFlowSet allocates the four 256 KB fields and the shared
// integration scratch (cost+stamp ≈ 2 MB, heap arena ≈ 1 MB).
func NewFlowSet(g *Grid, l *Layer) *FlowSet {
	if g == nil || l == nil {
		panic("path: NewFlowSet needs grid and layer")
	}
	f := &FlowSet{
		g:          g,
		l:          l,
		integCost:  make([]int32, GridSize*GridSize),
		integStamp: make([]uint32, GridSize*GridSize),
		heap:       make([]openNode, 0, openArenaCap),
		staging:    make([]uint8, GridSize*GridSize),
		inflight:   -1,
	}
	for i := range f.slots {
		f.slots[i].dirs = make([]uint8, GridSize*GridSize)
		f.slots[i].goal = -1
	}
	return f
}

// SlotState reports one slot's (goal, live, lastUse) for FSV logs.
func (f *FlowSet) SlotState(i int) (goal int32, live bool, lastUse uint64) {
	s := &f.slots[i]
	return s.goal, s.live, s.lastUse
}

// Dir returns the direction byte at a cell for a slot.
func (f *FlowSet) Dir(slot int, x, y int32) uint8 { return f.slots[slot].dirs[idx(x, y)] }

// Step translates a direction byte into a (dx, dy) step. DirNone
// returns (0, 0): the unit stops or falls back to nearest-reachable.
func Step(dir uint8) (int32, int32) {
	if dir == DirNone || dir > 8 {
		return 0, 0
	}
	d := neighborOrder[dir-1]
	return d.dx, d.dy
}

// pickSlot resolves a goal to a slot: a live same-goal slot, else an
// empty slot, else the least-recently-used (ties: lowest index —
// deterministic). reused reports the same-goal hit.
func (f *FlowSet) pickSlot(goal int32) (slot int, reused bool) {
	f.useCounter++
	for i := range f.slots {
		if f.slots[i].live && f.slots[i].goal == goal {
			f.slots[i].lastUse = f.useCounter
			return i, true
		}
	}
	pick := -1
	for i := range f.slots {
		if !f.slots[i].live {
			pick = i
			break
		}
	}
	if pick == -1 {
		var oldest uint64
		for i := range f.slots {
			if pick == -1 || f.slots[i].lastUse < oldest {
				pick = i
				oldest = f.slots[i].lastUse
			}
		}
	}
	s := &f.slots[pick]
	if f.inflight == int8(pick) {
		f.abortInflight() // recycled out from under its integration
	}
	s.goal = goal
	s.live = true
	s.ready = false
	s.dirty = false
	s.lastUse = f.useCounter
	return pick, false
}

// Acquire returns the slot index of a field flowing toward (gx, gy),
// integrating SYNCHRONOUSLY (the pre-#291 contract: the field is
// ready on return). Fails closed on a blocked goal.
func (f *FlowSet) Acquire(gx, gy int32) (int, bool) {
	if !InBounds(gx, gy) || !f.l.CenterClear(gx, gy) {
		return -1, false
	}
	pick, reused := f.pickSlot(idx(gx, gy))
	if !reused || !f.slots[pick].ready {
		f.integrate(&f.slots[pick])
	}
	return pick, true
}

// AcquireAsync is the #291 budgeted variant: the slot is returned
// immediately and integration runs under the Pump expansion budget.
// ready reports whether the field already serves this goal; a fresh
// slot reads all-DirNone until its first integration completes.
func (f *FlowSet) AcquireAsync(gx, gy int32) (slot int, ready, ok bool) {
	if !InBounds(gx, gy) || !f.l.CenterClear(gx, gy) {
		return -1, false, false
	}
	pick, reused := f.pickSlot(idx(gx, gy))
	s := &f.slots[pick]
	if !reused {
		for i := range s.dirs {
			s.dirs[i] = DirNone // a recycled field must not serve the OLD goal
		}
		s.dirty = true
	}
	return pick, s.ready, true
}

// Ready reports whether a slot's field is a complete integration of
// its current goal (false mid-flight on a fresh slot; a re-integrating
// live slot stays ready, serving its stale-but-consistent field).
func (f *FlowSet) Ready(slot int) bool { return f.slots[slot].ready }

// Release frees a slot (group order completed).
func (f *FlowSet) Release(slot int) {
	if f.inflight == int8(slot) {
		f.abortInflight()
	}
	f.slots[slot].live = false
	f.slots[slot].goal = -1
	f.slots[slot].ready = false
	f.slots[slot].dirty = false
}

// InvalidateAll re-integrates every live slot in slot order,
// synchronously — the restamp hook of the sync contract (§3.2: a
// stamp intersecting a live field re-integrates it; fields are
// map-global, so all live slots refresh).
func (f *FlowSet) InvalidateAll() {
	f.abortInflight()
	for i := range f.slots {
		if f.slots[i].live {
			f.integrate(&f.slots[i])
		}
	}
}

// InvalidateAllAsync is the budgeted restamp hook: live slots are
// marked dirty and re-integrate under the Pump budget; each keeps
// serving its stale-but-consistent field until its refresh completes.
// An in-flight integration restarts (a mid-flight stamp must not mix
// pre- and post-stamp reachability in one field).
func (f *FlowSet) InvalidateAllAsync() {
	f.abortInflight()
	for i := range f.slots {
		if f.slots[i].live {
			f.slots[i].dirty = true
		}
	}
}

// Pump drives pending integrations under a counted expansion budget
// (§6 counted-work rule): at most maxExpansions heap pops across this
// call, dirty slots served in slot-index order, one in flight at a
// time. Returns the expansions consumed. Call once per tick from the
// pathing slice; DefaultIntegrationBudget ≈ 1.25 ms.
func (f *FlowSet) Pump(maxExpansions int) int {
	consumed := 0
	for consumed < maxExpansions {
		if f.inflight == -1 {
			next := -1
			for i := range f.slots {
				if f.slots[i].live && f.slots[i].dirty {
					next = i
					break
				}
			}
			if next == -1 {
				return consumed
			}
			f.startIntegration(next)
		}
		c, done := f.stepIntegration(maxExpansions - consumed)
		consumed += c
		if !done {
			return consumed
		}
		f.finishIntegration()
	}
	return consumed
}

func (f *FlowSet) touch(c int32) {
	if f.integStamp[c] != f.epoch {
		f.integStamp[c] = f.epoch
		f.integCost[c] = 1<<31 - 1
	}
}

// reverseDir maps a neighborOrder index to the direction byte of
// the opposite step (the compass order is symmetric: i ↔ (i+4)%8).
func reverseDir(i int) uint8 { return uint8((i+4)%8) + 1 }

// integrate fills a slot synchronously: start, run to exhaustion,
// publish. The budgeted path (#291) drives the same three pieces
// through Pump.
func (f *FlowSet) integrate(s *flowSlot) {
	si := -1
	for i := range f.slots {
		if &f.slots[i] == s {
			si = i
		}
	}
	f.startIntegration(si)
	for {
		if _, done := f.stepIntegration(1 << 30); done {
			break
		}
	}
	f.finishIntegration()
}

// startIntegration seeds a Dijkstra from the slot's goal into the
// staging buffer. Integration state (epoch'd cost array, heap, seq)
// lives on the FlowSet and persists across budget slices.
func (f *FlowSet) startIntegration(si int) {
	s := &f.slots[si]
	f.inflight = int8(si)
	f.epoch++
	f.seq = 0
	f.heap = f.heap[:0]

	// staging reset: unreached cells must read DirNone
	for i := range f.staging {
		f.staging[i] = DirNone
	}

	f.touch(s.goal)
	f.integCost[s.goal] = 0
	f.push(openNode{f: 0, h: 0, seq: f.seq, cell: s.goal})
	f.seq++
}

// abortInflight drops the in-flight integration; the slot keeps its
// dirty flag (or its recycler resets it), staging is discarded.
func (f *FlowSet) abortInflight() {
	f.inflight = -1
	f.heap = f.heap[:0]
}

// stepIntegration runs up to budget heap pops of the in-flight
// Dijkstra under the §4 (cost, seq) total-order discipline, writing
// each cell's direction at its final relaxation — the direction
// points along the cell's settled cheapest step toward the goal. The
// (cost, seq) heap makes every relaxation order — and therefore
// every tie — identical across runs and machines (§4 rule 5's
// intent; a fixed row-major sweep was measured 2× slower for the
// same determinism). Slicing the loop by pop count cannot change the
// result: the pop order is a pure function of the heap state.
func (f *FlowSet) stepIntegration(budget int) (consumed int, done bool) {
	for len(f.heap) > 0 {
		if consumed >= budget {
			return consumed, false // parked; resume next Pump
		}
		n := f.pop()
		consumed++
		c := n.cell
		if n.f > f.integCost[c] {
			continue // stale
		}
		x, y := c%GridSize, c/GridSize
		for i := range neighborOrder {
			d := neighborOrder[i]
			nx, ny := x+d.dx, y+d.dy
			if !InBounds(nx, ny) {
				continue
			}
			// reverse reachability: the unit walks n→c, so test the
			// step FROM the neighbor INTO this cell
			if !f.stepClear(nx, ny, -d.dx, -d.dy) {
				continue
			}
			step := CostCardinal
			if d.dx != 0 && d.dy != 0 {
				step = CostDiagonal
			}
			nc := idx(nx, ny)
			f.touch(nc)
			ncost := f.integCost[c] + step
			if ncost >= f.integCost[nc] {
				continue
			}
			f.integCost[nc] = ncost
			f.staging[nc] = reverseDir(i) // step n→c, toward the goal
			if !f.push(openNode{f: ncost, h: 0, seq: f.seq, cell: nc}) {
				// arena exhausted: unreached cells stay DirNone —
				// fail closed, field still byte-deterministic
				f.heap = f.heap[:0]
				break
			}
			f.seq++
		}
	}
	return consumed, true
}

// finishIntegration publishes the completed staging field into its
// slot in one copy — readers never observe a partial integration.
func (f *FlowSet) finishIntegration() {
	s := &f.slots[f.inflight]
	f.staging[s.goal] = DirNone // the goal itself has no onward step
	copy(s.dirs, f.staging)
	s.ready = true
	s.dirty = false
	f.inflight = -1
}

// stepClear mirrors Searcher.stepClear for the flow layer: bounds,
// center clearance, cliff adjacency, no corner-cutting. Flat ground
// (both cliff bytes zero — the overwhelmingly common case on the
// integration hot path) skips the level-span computation.
func (f *FlowSet) stepClear(x, y, dx, dy int32) bool {
	nx, ny := x+dx, y+dy
	if !InBounds(nx, ny) {
		return false
	}
	if !f.l.CenterClear(nx, ny) {
		return false
	}
	if f.g.cliff[idx(x, y)]|f.g.cliff[idx(nx, ny)] != 0 &&
		!f.g.AdjacencyLegal(x, y, nx, ny) {
		return false
	}
	if dx != 0 && dy != 0 {
		if !f.l.CenterClear(x, ny) || !f.l.CenterClear(nx, y) {
			return false
		}
	}
	return true
}

func (f *FlowSet) push(n openNode) bool {
	if len(f.heap) == cap(f.heap) {
		return false
	}
	f.heap = append(f.heap, n)
	i := len(f.heap) - 1
	for i > 0 {
		p := (i - 1) / 2
		if !keyLess(&f.heap[i], &f.heap[p]) {
			break
		}
		f.heap[i], f.heap[p] = f.heap[p], f.heap[i]
		i = p
	}
	return true
}

func (f *FlowSet) pop() openNode {
	top := f.heap[0]
	last := len(f.heap) - 1
	f.heap[0] = f.heap[last]
	f.heap = f.heap[:last]
	i := 0
	for {
		l, r := 2*i+1, 2*i+2
		m := i
		if l < last && keyLess(&f.heap[l], &f.heap[m]) {
			m = l
		}
		if r < last && keyLess(&f.heap[r], &f.heap[m]) {
			m = r
		}
		if m == i {
			break
		}
		f.heap[i], f.heap[m] = f.heap[m], f.heap[i]
		i = m
	}
	return top
}

// Backend identifies which pathing backend serves a request — the
// PathProvider seam (§3.2: either backend serves with no
// gameplay-visible difference beyond paths taken).
type Backend uint8

const (
	BackendAStar Backend = iota // per-unit A* + sharing
	BackendFlow                 // shared-goal flow field
)

func (b Backend) String() string {
	if b == BackendFlow {
		return "flow-field"
	}
	return "astar+sharing"
}

// Provider is the backend selector. Threshold is data (sim
// semantics): shared-goal groups of at least Threshold units route
// to the flow field.
type Provider struct {
	Sharer    *Sharer
	Flow      *FlowSet
	Threshold int
}

// NewProvider wires both backends with the D-29 default threshold.
func NewProvider(s *Sharer, f *FlowSet) *Provider {
	if s == nil || f == nil {
		panic("path: NewProvider needs both backends")
	}
	return &Provider{Sharer: s, Flow: f, Threshold: DefaultFlowThreshold}
}

// SelectBackend picks the backend for a shared-goal group move.
func (p *Provider) SelectBackend(groupSize int) Backend {
	if groupSize >= p.Threshold {
		return BackendFlow
	}
	return BackendAStar
}

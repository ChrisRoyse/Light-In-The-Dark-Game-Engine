package path

// Amortized path-request queue (pathfinding.md §6, §6.1): requests
// are serviced strictly in (requestTick, requestSeq) order under a
// COUNTED expansion budget — never wall-clock, because a time cutoff
// would diverge across machines (Determinism §2.3). The budget value
// is sim semantics: part of the replay/version contract, tuned only
// as a sim-version change.
//
// A search that exhausts the budget parks its state (its own copied
// abstract-node list + waypoint scratch + segment cursor — never the
// shared HPA scratch) and resumes next Service call. Parking happens
// BETWEEN refinement segments, so the budget can overshoot by at
// most one ≤2-sector segment — a counted, deterministic overshoot.
// The final path of a parked-and-resumed search is identical to an
// unbudgeted run by construction: segment refinement is a pure
// function of (grid, layer, request), independent of where the tick
// boundary fell.

// DefaultExpansionBudget is the spike-calibrated per-tick budget
// (~100,000 expansions ≈ 1–2 ms, D-2026-06-11-29).
const DefaultExpansionBudget int32 = 100_000

// RequestCap is the in-flight ceiling (§7 pool table, D-18 caps).
const RequestCap = 512

// nodeCapPerSector: the per-request hard cap is
// 4 × corridorSectors × SectorSize² fine expansions (§6 "4× the
// sector-corridor estimate"); beyond it the search terminates with
// the best partial path.
const nodeCapPerSector = 4 * SectorSize * SectorSize

// runWaypointCap bounds one request's waypoint scratch: the longest
// sane path. Exceeding it terminates as partial (fail closed).
const runWaypointCap = 4096

// Request is one path request, issued by the Orders phase.
type Request struct {
	Owner          uint32 // opaque handle (EntityID upstairs)
	SX, SY, TX, TY int32
	Tick           uint32 // requestTick
	Seq            uint16 // requestSeq within the tick
}

// RequestStatus is the delivery status of a serviced request.
type RequestStatus uint8

const (
	StatusCompleted RequestStatus = iota
	StatusPartial                 // node cap or scratch cap hit: best partial path
	StatusUnreachable
	StatusFailed // start blocked / pool exhausted / arena overflow
)

func (s RequestStatus) String() string {
	switch s {
	case StatusCompleted:
		return "completed"
	case StatusPartial:
		return "partial"
	case StatusUnreachable:
		return "unreachable"
	case StatusFailed:
		return "failed"
	}
	return "?"
}

// ServiceEvent is one line of the per-tick service log (FSV surface).
type ServiceEvent struct {
	Tick       uint32 // request tick
	Seq        uint16 // request seq
	Expansions int32  // expansions this request consumed THIS service
	Parked     bool   // true: budget boundary hit, search suspended
	Resumed    bool   // true: this service continued a parked search
	Done       bool   // true: delivered (see Status)
	Status     RequestStatus
	Path       PathID // valid when Done && Status != Unreachable/Failed
}

// activeRun is the single suspendable search (only the queue head
// can park, so one slot suffices; the §7 second searcher slot stays
// free for same-tick non-queued searches).
type activeRun struct {
	active   bool
	req      Request
	res      FindResult
	nodes    []int32 // OWN copy of the abstract path, forward order
	nodeCnt  int
	segIdx   int // next index into nodes (counts down; -1 = goal leg)
	px, py   int32
	tx, ty   int32   // possibly substituted goal
	wp       []int32 // waypoint scratch, accumulated across parks
	nodeCap  int32
	resumed  bool
	overflow bool // wp scratch full → partial
}

// Queue is the amortized request queue over one HPA hierarchy and
// one PathStore. Everything preallocates in NewQueue (R-GC-2).
type Queue struct {
	h  *HPA
	ps *PathStore

	ring  []Request // FIFO; enqueue order IS (tick, seq) order
	head  int
	count int

	run activeRun

	log []ServiceEvent // reset every Service call

	lastTick uint32 // monotonicity guard
	lastSeq  uint16

	dropped uint64 // enqueue refusals (queue full)

	DebugAssert func(msg string)

	// OnResult, when set, fires at every delivery (Done events).
	OnResult func(ev ServiceEvent)
}

// NewQueue wires the queue to a hierarchy and a waypoint pool.
func NewQueue(h *HPA, ps *PathStore) *Queue {
	if h == nil || ps == nil {
		panic("path: NewQueue needs an HPA and a PathStore")
	}
	return &Queue{
		h:    h,
		ps:   ps,
		ring: make([]Request, RequestCap),
		log:  make([]ServiceEvent, 0, RequestCap+1),
		run: activeRun{
			nodes: make([]int32, coarseNodes),
			wp:    make([]int32, 0, runWaypointCap),
		},
	}
}

// Pending returns queued (not yet started) requests; InFlight adds
// the parked run.
func (q *Queue) Pending() int { return q.count }
func (q *Queue) InFlight() int {
	if q.run.active {
		return q.count + 1
	}
	return q.count
}

// Dropped returns the lifetime enqueue-refusal count.
func (q *Queue) Dropped() uint64 { return q.dropped }

// Log returns the service log of the most recent Service call.
func (q *Queue) Log() []ServiceEvent { return q.log }

// Enqueue pushes a request. The 513th in-flight request fails
// deterministically. Requests must arrive in (tick, seq) order —
// the Orders phase issues them that way; violations assert.
func (q *Queue) Enqueue(r Request) bool {
	if r.Tick < q.lastTick || (r.Tick == q.lastTick && r.Seq < q.lastSeq) {
		if q.DebugAssert != nil {
			q.DebugAssert("request out of (tick, seq) order")
		}
		return false
	}
	if q.InFlight() >= RequestCap {
		q.dropped++
		return false
	}
	q.lastTick, q.lastSeq = r.Tick, r.Seq
	q.ring[(q.head+q.count)%RequestCap] = r
	q.count++
	return true
}

func (q *Queue) pop() Request {
	r := q.ring[q.head]
	q.head = (q.head + 1) % RequestCap
	q.count--
	return r
}

// Service runs the pathing phase for one tick: in-order servicing
// under the counted expansion budget. Returns expansions spent.
func (q *Queue) Service(budget int32) int32 {
	q.log = q.log[:0]
	spent := int32(0)
	for spent < budget {
		if !q.run.active {
			if q.count == 0 {
				break
			}
			req := q.pop()
			used, started := q.startRun(req)
			spent += used
			if !started {
				continue // delivered immediately (unreachable/failed/trivial)
			}
		}
		used := q.advanceRun(budget - spent)
		spent += used
		if q.run.active {
			// budget boundary: parked
			q.logEvent(ServiceEvent{
				Tick: q.run.req.Tick, Seq: q.run.req.Seq,
				Expansions: used,
				Parked:     true, Resumed: q.run.resumed,
			})
			q.run.resumed = true
			break
		}
	}
	return spent
}

// startRun begins servicing one request: label short-circuit,
// substitution, coarse stage. Returns (expansions, stillRunning).
// Immediate outcomes (unreachable, blocked start, same-cell trivial)
// deliver inline and return stillRunning=false.
func (q *Queue) startRun(req Request) (int32, bool) {
	r := &q.run
	r.req = req
	r.res = FindResult{GoalX: req.TX, GoalY: req.TY}
	r.wp = r.wp[:0]
	r.resumed = false
	r.overflow = false

	if !InBounds(req.SX, req.SY) || !InBounds(req.TX, req.TY) {
		q.deliver(StatusFailed, 0)
		return 0, false
	}
	startLabel := q.h.Label(req.SX, req.SY)
	if startLabel < 0 {
		q.deliver(StatusFailed, 0)
		return 0, false
	}
	r.tx, r.ty = req.TX, req.TY
	if q.h.Label(r.tx, r.ty) != startLabel {
		nx, ny, ok := q.h.NearestReachable(startLabel, r.tx, r.ty)
		if !ok {
			r.res.Stage = StageUnreachable
			q.deliver(StatusUnreachable, 0)
			return 0, false
		}
		r.tx, r.ty = nx, ny
		r.res.GoalX, r.res.GoalY = nx, ny
		r.res.Substituted = true
	}
	r.px, r.py = req.SX, req.SY
	if req.SX == r.tx && req.SY == r.ty {
		q.deliver(StatusCompleted, 0)
		return 0, false
	}

	startSec, goalSec := sectorOf(req.SX, req.SY), sectorOf(r.tx, r.ty)
	if startSec == goalSec &&
		q.h.cellRegion[idx(req.SX, req.SY)] == q.h.cellRegion[idx(r.tx, r.ty)] {
		r.res.Stage = StageFineOnly
		r.nodeCnt = 0
		r.segIdx = -1
		r.nodeCap = nodeCapPerSector
		r.active = true
		return 0, true
	}
	r.res.Stage = StageCoarseFine
	if !q.h.coarseSearch(req.SX, req.SY, r.tx, r.ty) {
		q.deliver(StatusFailed, q.h.coarseExpCount)
		return q.h.coarseExpCount, false
	}
	r.res.CoarseExpansions = q.h.coarseExpCount
	r.res.CorridorSectorCnt = q.h.corridor.Count()
	r.nodeCap = int32(r.res.CorridorSectorCnt) * nodeCapPerSector
	// copy the abstract path out of the shared scratch: forward order
	n := 0
	for node := q.h.cParent[goalNodeID]; node != -1; node = q.h.cParent[node] {
		r.nodes[n] = node
		n++
	}
	r.nodeCnt = n
	r.segIdx = n - 1
	r.active = true
	return q.h.coarseExpCount, true
}

// advanceRun refines segments until done, node-capped, or the given
// budget headroom is spent. Returns fine expansions consumed.
func (q *Queue) advanceRun(headroom int32) int32 {
	r := &q.run
	used := int32(0)
	for used < headroom {
		if r.res.FineExpansions > r.nodeCap || r.overflow {
			q.deliver(StatusPartial, used)
			return used
		}
		var cx, cy int32
		if r.segIdx >= 0 {
			cell := q.h.nodeCell(r.nodes[r.segIdx])
			cx, cy = cell%GridSize, cell/GridSize
		} else {
			cx, cy = r.tx, r.ty
		}
		before := len(r.wp)
		wp, ok := q.refineCapped(r.px, r.py, cx, cy)
		if !ok {
			q.deliver(StatusFailed, used)
			return used
		}
		seg := q.h.fine.Expansions()
		if wp == nil { // scratch overflow inside the segment
			r.wp = r.wp[:before]
			r.overflow = true
			used += seg
			r.res.FineExpansions += seg
			continue
		}
		r.wp = wp
		used += seg
		r.res.FineExpansions += seg
		r.px, r.py = cx, cy
		if r.segIdx < 0 {
			q.deliver(StatusCompleted, used)
			return used
		}
		r.segIdx--
	}
	return used // budget boundary: caller parks
}

// refineCapped is refineSegment against the run's own scratch with
// capacity enforcement (append must not grow past runWaypointCap).
// Returns (nil, true) when the segment would overflow the scratch.
func (q *Queue) refineCapped(px, py, cx, cy int32) ([]int32, bool) {
	r := &q.run
	if px == cx && py == cy {
		q.h.fine.expansions = 0
		return r.wp, true
	}
	dx, dy := cx-px, cy-py
	if dx >= -1 && dx <= 1 && dy >= -1 && dy <= 1 {
		q.h.fine.expansions = 0
		if len(r.wp) == cap(r.wp) {
			return nil, true
		}
		return append(r.wp, idx(cx, cy)), true
	}
	q.h.segMask.Clear()
	q.h.segMask.Set(sectorOf(px, py))
	q.h.segMask.Set(sectorOf(cx, cy))
	wp, ok := q.h.fine.SearchCorridor(q.h.l, &q.h.segMask, px, py, cx, cy, r.wp)
	if !ok {
		return nil, false
	}
	if cap(wp) != cap(r.wp) || len(wp) > runWaypointCap {
		// append grew the scratch: segment didn't fit
		return nil, true
	}
	return wp, true
}

// deliver finishes the active run: waypoints copy into a pooled
// PathStore slot (bbox from the walked cells), the event logs, and
// the run slot recycles.
func (q *Queue) deliver(status RequestStatus, usedThisService int32) {
	r := &q.run
	ev := ServiceEvent{
		Tick: r.req.Tick, Seq: r.req.Seq,
		Expansions: usedThisService,
		Resumed:    r.resumed,
		Done:       true,
		Status:     status,
	}
	if (status == StatusCompleted || status == StatusPartial) && len(r.wp) > 0 {
		minX, minY := r.req.SX, r.req.SY
		maxX, maxY := r.req.SX, r.req.SY
		for _, c := range r.wp {
			x, y := c%GridSize, c/GridSize
			if x < minX {
				minX = x
			}
			if x > maxX {
				maxX = x
			}
			if y < minY {
				minY = y
			}
			if y > maxY {
				maxY = y
			}
		}
		bbox := Rect{X: minX, Y: minY, W: maxX - minX + 1, H: maxY - minY + 1}
		id, buf, ok := q.ps.Acquire(bbox)
		if !ok || cap(buf) < len(r.wp) {
			if ok {
				q.ps.Release(id)
			}
			ev.Status = StatusFailed
		} else {
			buf = buf[:0]
			for _, c := range r.wp {
				buf = append(buf, c)
			}
			q.ps.SetWaypoints(id, buf)
			ev.Path = id
		}
	}
	r.active = false
	q.logEvent(ev)
	if q.OnResult != nil {
		q.OnResult(ev)
	}
}

// InvalidateActive re-enqueues a parked search from its CURRENT
// position (§6.1 step 6: a stamp hit the corridor) and recycles the
// run slot. New (tick, seq) come from the caller — the invalidation
// is itself an ordered sim event. No-op when nothing is parked.
func (q *Queue) InvalidateActive(tick uint32, seq uint16) bool {
	r := &q.run
	if !r.active {
		return false
	}
	req := Request{
		Owner: r.req.Owner,
		SX:    r.px, SY: r.py, // resume from where the path got to
		TX: r.req.TX, TY: r.req.TY, // original goal re-resolves
		Tick: tick, Seq: seq,
	}
	r.active = false
	r.wp = r.wp[:0]
	return q.Enqueue(req)
}

func (q *Queue) logEvent(ev ServiceEvent) {
	if len(q.log) < cap(q.log) {
		q.log = append(q.log, ev)
	}
}

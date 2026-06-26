package sim

import (
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim/path"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/statehash"
)

// openGrid returns a grid with every cell walkable.
func openGrid() *path.Grid {
	g := path.NewGrid()
	for y := int32(0); y < path.GridSize; y++ {
		for x := int32(0); x < path.GridSize; x++ {
			g.SetFlags(x, y, path.Walkable)
		}
	}
	return g
}

func cellIdx(x, y int32) int32 { return y*path.GridSize + x }

// addMover spawns a unit at the center of cell (x, y), attaches
// Movement, and claims the cell.
func addMover(tb testing.TB, w *World, x, y int32, speed fixed.F64) EntityID {
	tb.Helper()
	id, ok := w.CreateUnit(CellCenter(cellIdx(x, y)), 0)
	if !ok || !w.Movements.Add(w.Ents, w.Transforms, id, speed, 0x4000) {
		tb.Fatal("mover setup failed")
	}
	if !w.OccupyCell(id) {
		tb.Fatalf("OccupyCell failed at (%d,%d)", x, y)
	}
	return id
}

// assertReservationConsistent is the global FSV invariant: every
// unit's ResCell is owned by exactly that unit, and no reservedBy
// entry points at a unit that does not claim it back.
func assertReservationConsistent(t *testing.T, w *World) {
	t.Helper()
	m := w.Movements
	claimed := 0
	for r := int32(0); r < m.count; r++ {
		if c := m.ResCell[r]; c != -1 {
			claimed++
			if w.Grid.FlagsAt(c%path.GridSize, c/path.GridSize)&path.OccupiedDynamic == 0 {
				t.Fatalf("row %d: ResCell %d claims a cell without OccupiedDynamic", r, c)
			}
			if w.reservedBy[c] != m.Entity[r] {
				t.Fatalf("row %d: ResCell %d owned by %d, not %d",
					r, c, w.reservedBy[c], m.Entity[r])
			}
		}
	}
	flagged := 0
	for y := int32(0); y < path.GridSize; y++ {
		for x := int32(0); x < path.GridSize; x++ {
			if w.Grid.FlagsAt(x, y)&path.OccupiedDynamic != 0 {
				flagged++
			}
		}
	}
	if flagged != claimed {
		t.Fatalf("reservation leak: %d OccupiedDynamic cells vs %d ResCell claims", flagged, claimed)
	}
}

// Edge 1 (the §5 headline case): 10 units funnel through a 1-cell gap
// in a wall. At no tick do two units stand in the gap cell, all 10
// pass it, all 10 reach distinct destinations, and the full position
// trace is bit-identical across two runs.
func TestAvoidanceChokeSingleFile(t *testing.T) {
	const gapX, gapY = int32(20), int32(8)
	gap := cellIdx(gapX, gapY)
	const n = 10
	type result struct {
		hash     uint64
		arrivals []EntityID
		gapOrder []EntityID
		lastTick uint32
	}
	run := func() result {
		w := NewWorld(Caps{})
		g := openGrid()
		for y := int32(0); y < path.GridSize; y++ {
			if y != gapY {
				g.ClearFlags(gapX, y, path.Walkable)
			}
		}
		w.SetGrid(g)

		ids := make([]EntityID, n)
		leg := make([]int, n)
		dest2 := make([]fixed.Vec2, n)
		idxOf := func(id EntityID) int {
			for i := range ids {
				if ids[i] == id {
					return i
				}
			}
			return -1
		}
		for i := int32(0); i < n; i++ {
			ids[i] = addMover(t, w, 10, 4+i, 16*fixed.One)
			leg[i] = 1
			dest2[i] = CellCenter(cellIdx(30, 4+i))
		}
		var arrivals []EntityID
		w.RegisterHandler(1, func(ww *World, e Event) {
			i := idxOf(e.Src)
			if i == -1 {
				return
			}
			if leg[i] == 1 {
				leg[i] = 2
				ww.StartMoveTo(e.Src, dest2[i])
				return
			}
			if leg[i] == 2 {
				leg[i] = 3
				arrivals = append(arrivals, e.Src)
			}
		})
		w.Subscribe(EvMoveDone, 1)
		// the stand-in order layer: a stalled-out unit re-targets its
		// current leg (the real re-path arrives with #144/#146)
		w.RegisterHandler(2, func(ww *World, e Event) {
			i := idxOf(e.Src)
			if i == -1 {
				return
			}
			switch leg[i] {
			case 1:
				ww.StartMoveTo(e.Src, CellCenter(gap))
			case 2:
				ww.StartMoveTo(e.Src, dest2[i])
			}
		})
		w.Subscribe(EvRepathNeeded, 2)
		for i := range ids {
			w.StartMoveTo(ids[i], CellCenter(gap))
		}

		h := statehash.New()
		var gapOrder []EntityID
		seenGap := make([]bool, n)
		var tick uint32
		for tick = 1; tick <= 3000; tick++ {
			w.Step()
			inGap := 0
			for i := range ids {
				tr := w.Transforms.Row(ids[i])
				p := w.Transforms.Pos[tr]
				h.WriteI64(int64(p.X))
				h.WriteI64(int64(p.Y))
				if cellOfPos(p) == gap {
					inGap++
					if !seenGap[i] {
						seenGap[i] = true
						gapOrder = append(gapOrder, ids[i])
					}
				}
			}
			if inGap > 1 {
				t.Fatalf("tick %d: %d units in the 1-cell gap (single-file violated)", tick, inGap)
			}
			if len(arrivals) == n {
				break
			}
		}
		assertReservationConsistent(t, w)
		return result{h.Sum64(), arrivals, gapOrder, tick}
	}

	r1 := run()
	t.Logf("run1: all %d arrived by tick %d; gap pass order=%v; arrival order=%v; trace hash=%016x",
		len(r1.arrivals), r1.lastTick, r1.gapOrder, r1.arrivals, r1.hash)
	if len(r1.arrivals) != n {
		t.Fatalf("only %d/%d units arrived within 3000 ticks", len(r1.arrivals), n)
	}
	if len(r1.gapOrder) != n {
		t.Fatalf("only %d/%d units passed the gap", len(r1.gapOrder), n)
	}
	r2 := run()
	t.Logf("run2: trace hash=%016x (must equal run1)", r2.hash)
	if r1.hash != r2.hash {
		t.Fatalf("position traces diverged: %016x vs %016x", r1.hash, r2.hash)
	}
	for i := range r1.gapOrder {
		if r1.gapOrder[i] != r2.gapOrder[i] || r1.arrivals[i] != r2.arrivals[i] {
			t.Fatalf("ordering diverged at %d: gap %v/%v arrival %v/%v",
				i, r1.gapOrder[i], r2.gapOrder[i], r1.arrivals[i], r2.arrivals[i])
		}
	}
}

// Edge 2: an idle occupant is shoved out of a mover's way, the shove
// decision is logged, and when TWO movers contend the same occupant
// the dense-row-order mover wins the shove (the other sees the
// occupant already moving and must not double-shove).
func TestAvoidanceShoveContention(t *testing.T) {
	w := NewWorld(Caps{})
	w.SetGrid(openGrid())
	ctr := cellIdx(50, 50)
	a := addMover(t, w, 50, 50, 8*fixed.One)  // idle occupant, dense row 0
	m1 := addMover(t, w, 49, 50, 8*fixed.One) // west flanker, dense row 1
	m2 := addMover(t, w, 51, 50, 8*fixed.One) // east flanker, dense row 2

	type shoveRec struct {
		tick         uint32
		mover, moved EntityID
		cell         int32
	}
	var shoves []shoveRec
	w.OnShove = func(tick uint32, mover, shoved EntityID, cell int32) {
		shoves = append(shoves, shoveRec{tick, mover, shoved, cell})
	}
	w.StartMoveTo(m1, CellCenter(ctr))
	w.StartMoveTo(m2, CellCenter(ctr))

	// flankers spawn at adjacent cell centers (16 units from the
	// boundary, speed 8): both hit the boundary and block on the same
	// tick; exactly one shove fires that tick (dense-row winner)
	firstShoveTick := 0
	for tick := 1; tick <= 10 && len(shoves) == 0; tick++ {
		w.Step()
		firstShoveTick = tick
	}
	if len(shoves) != 1 {
		t.Fatalf("want exactly 1 shove on the first blocked tick, got %d (%+v)", len(shoves), shoves)
	}
	// expected escape: compass scan N,NE,... from (50,50) → N=(50,51)
	wantEscape := cellIdx(50, 51)
	t.Logf("tick %d shove: mover=%d shoved=%d cell=%d (want mover=%d cell=%d — dense-row winner, compass-N escape)",
		firstShoveTick, shoves[0].mover, shoves[0].moved, shoves[0].cell, m1, wantEscape)
	if shoves[0].mover != m1 || shoves[0].moved != a || shoves[0].cell != wantEscape {
		t.Fatalf("shove decision wrong: %+v", shoves[0])
	}

	// first mover to own the vacated center must be m1 (row order)
	var firstOwner EntityID
	haveFirst := false
	for tick := 0; tick < 300; tick++ {
		w.Step()
		if !haveFirst && w.Grid.FlagsAt(50, 50)&path.OccupiedDynamic != 0 {
			if o := w.reservedBy[ctr]; o == m1 || o == m2 {
				firstOwner, haveFirst = o, true
			}
		}
	}
	t.Logf("first mover-owner of center cell: %d (want %d)", firstOwner, m1)
	if !haveFirst || firstOwner != m1 {
		t.Fatalf("dense-row arbitration violated: first owner %d (found=%v), want %d", firstOwner, haveFirst, m1)
	}

	// reservation dump (FSV: read the actual table, not return values)
	for _, u := range []EntityID{a, m1, m2} {
		r := w.Movements.Row(u)
		tr := w.Transforms.Row(u)
		c := w.Movements.ResCell[r]
		t.Logf("unit %d: pos=(%d,%d) cell=%d ResCell=%d reservedBy[ResCell]=%d state=%d",
			u, w.Transforms.Pos[tr].X, w.Transforms.Pos[tr].Y,
			cellOfPos(w.Transforms.Pos[tr]), c, w.reservedBy[c], w.Movements.State[r])
	}
	assertReservationConsistent(t, w)
	if w.Grid.FlagsAt(50, 50)&path.OccupiedDynamic == 0 {
		t.Fatalf("center cell ended unowned — someone should hold it")
	}
}

// Edge 3: a mover walled into a corridor behind an immobile blocker
// (speed 0, Following — not shovable) can neither commit nor sidestep:
// the stall counter climbs and at exactly the threshold the unit goes
// MoveBlocked and emits EvRepathNeeded once. Custom threshold honored.
func TestAvoidanceStallRepath(t *testing.T) {
	build := func(threshold uint16) (*World, EntityID, *[]uint32) {
		w := NewWorld(Caps{})
		g := path.NewGrid() // all unwalkable except the corridor
		for x := int32(4); x <= 10; x++ {
			g.SetFlags(x, 8, path.Walkable)
		}
		w.SetGrid(g)
		if threshold != 0 {
			w.SetStallRepathTicks(threshold)
		}
		blocker := addMover(t, w, 7, 8, 0)                 // immobile
		w.StartMoveTo(blocker, CellCenter(cellIdx(10, 8))) // Following: not shovable
		mover := addMover(t, w, 5, 8, 8*fixed.One)
		ticks := &[]uint32{}
		w.RegisterHandler(7, func(ww *World, e Event) {
			if e.Src == mover {
				*ticks = append(*ticks, ww.Tick())
			}
		})
		w.Subscribe(EvRepathNeeded, 7)
		w.StartMoveTo(mover, CellCenter(cellIdx(9, 8)))
		return w, mover, ticks
	}

	w, mover, repaths := build(0) // default threshold 8
	r := w.Movements.Row(mover)
	tr := w.Transforms.Row(mover)
	var firstBlock uint32
	var posAtBlock fixed.Vec2
	for tick := uint32(1); tick <= 40; tick++ {
		w.Step()
		st := w.Movements.Stall[r]
		t.Logf("tick %2d: pos=(%d,%d) stall=%d state=%d", tick,
			w.Transforms.Pos[tr].X, w.Transforms.Pos[tr].Y, st, w.Movements.State[r])
		if firstBlock == 0 && st == 1 {
			firstBlock = tick
			posAtBlock = w.Transforms.Pos[tr]
		}
		if w.Transforms.Pos[tr].Y != CellCenter(cellIdx(5, 8)).Y {
			t.Fatalf("tick %d: mover left the corridor centerline (sidestep must be impossible)", tick)
		}
	}
	if firstBlock == 0 {
		t.Fatal("mover never blocked")
	}
	if len(*repaths) != 1 {
		t.Fatalf("EvRepathNeeded fired %d times, want exactly 1 (%v)", len(*repaths), *repaths)
	}
	wantFire := firstBlock + uint32(DefaultStallRepathTicks) - 1
	t.Logf("first block tick=%d, EvRepathNeeded at tick=%d (want %d), final state=%d (MoveBlocked=%d)",
		firstBlock, (*repaths)[0], wantFire, w.Movements.State[r], MoveBlocked)
	if (*repaths)[0] != wantFire {
		t.Fatalf("re-path fired at tick %d, want %d", (*repaths)[0], wantFire)
	}
	if w.Movements.State[r] != MoveBlocked {
		t.Fatalf("state after stall: %d, want MoveBlocked", w.Movements.State[r])
	}
	if w.Transforms.Pos[tr] != posAtBlock {
		t.Fatalf("position drifted while blocked")
	}

	// custom threshold 3 → fires at firstBlock+2
	w2, m2, rp2 := build(3)
	r2 := w2.Movements.Row(m2)
	var fb2 uint32
	for tick := uint32(1); tick <= 20; tick++ {
		w2.Step()
		if fb2 == 0 && w2.Movements.Stall[r2] == 1 {
			fb2 = tick
		}
	}
	if len(*rp2) != 1 || (*rp2)[0] != fb2+2 {
		t.Fatalf("threshold 3: fired %v, want once at tick %d", *rp2, fb2+2)
	}
	t.Logf("threshold 3: EvRepathNeeded at tick %d (firstBlock %d + 2) — threshold is data, confirmed", (*rp2)[0], fb2)
}

// Edge 4: integration that would leave the grid holds position
// (fail-closed, no wrap, no panic, no event spam).
func TestAvoidanceOffGridHold(t *testing.T) {
	w := NewWorld(Caps{})
	w.SetGrid(openGrid())
	u := addMover(t, w, 0, 0, 16*fixed.One)
	start := w.Transforms.Pos[w.Transforms.Row(u)]
	w.StartMoveTo(u, fixed.Vec2{X: -100 * fixed.One, Y: start.Y}) // off the west edge
	events := 0
	w.RegisterHandler(9, func(ww *World, e Event) { events++ })
	w.Subscribe(EvRepathNeeded, 9)
	for tick := 0; tick < 30; tick++ {
		w.Step()
	}
	tr := w.Transforms.Row(u)
	r := w.Movements.Row(u)
	pos := w.Transforms.Pos[tr]
	t.Logf("after 30 ticks: pos=(%d,%d) (start (%d,%d)), state=%d, EvRepathNeeded count=%d",
		pos.X, pos.Y, start.X, start.Y, w.Movements.State[r], events)
	// the unit walks within cell (0,0) to the boundary, then holds:
	// the step that would land off-grid is refused, position frozen
	if pos.X < 0 || pos.Y != start.Y || cellOfPos(pos) != cellIdx(0, 0) {
		t.Fatalf("off-grid order must hold inside the edge cell: pos=(%d,%d)", pos.X, pos.Y)
	}
	if w.Movements.State[r] != MoveFollowing {
		t.Fatalf("hold must not change state: %d", w.Movements.State[r])
	}
	if events != 0 {
		t.Fatalf("off-grid hold must not spam re-path events: %d", events)
	}
}

// Death releases the reservation — no leaked cells.
func TestAvoidanceDeathReleasesCell(t *testing.T) {
	w := NewWorld(Caps{})
	w.SetGrid(openGrid())
	u := addMover(t, w, 60, 60, 8*fixed.One)
	c := cellIdx(60, 60)
	// NOTE: u is EntityID 0 (first handle = gen 0, index 0) — the very
	// value that made a bare reservedBy==0 check read as "free". The
	// OccupiedDynamic flag is the occupancy truth.
	if w.reservedBy[c] != u || w.Grid.FlagsAt(60, 60)&path.OccupiedDynamic == 0 {
		t.Fatalf("setup: cell %d not owned by %d (flag=%v)", c, u,
			w.Grid.FlagsAt(60, 60)&path.OccupiedDynamic != 0)
	}
	t.Logf("before kill: reservedBy[%d]=%d, OccupiedDynamic=%v", c, w.reservedBy[c],
		w.Grid.FlagsAt(60, 60)&path.OccupiedDynamic != 0)
	w.KillUnit(u)
	w.Step() // phase 7 destroys
	t.Logf("after kill+step: reservedBy[%d]=%d, OccupiedDynamic=%v", c, w.reservedBy[c],
		w.Grid.FlagsAt(60, 60)&path.OccupiedDynamic != 0)
	if w.reservedBy[c] != 0 || w.Grid.FlagsAt(60, 60)&path.OccupiedDynamic != 0 {
		t.Fatalf("death leaked the cell reservation")
	}
	assertReservationConsistent(t, w)
}

// Zero allocations with 256 grid-aware movers converging (shoves,
// sidesteps, and stalls all on the hot path).
func TestAvoidanceZeroAlloc(t *testing.T) {
	w := NewWorld(Caps{})
	w.SetGrid(openGrid())
	for u := 0; u < 256; u++ {
		x := int32(20 + (u%16)*2)
		y := int32(20 + (u/16)*2)
		id := addMover(t, w, x, y, fixed.One/2)
		w.StartMoveTo(id, CellCenter(cellIdx(36, 36)))
	}
	for i := 0; i < allocWarmupTicks; i++ {
		w.Step()
	}
	allocs := testing.AllocsPerRun(100, func() { w.Step() })
	t.Logf("AllocsPerRun(Step with 256 converging grid movers) = %v", allocs)
	if allocs != 0 {
		t.Fatalf("avoidance allocated: %v", allocs)
	}
}

func BenchmarkAvoidanceStep(b *testing.B) {
	w := NewWorld(Caps{})
	w.SetGrid(openGrid())
	for u := 0; u < 1000; u++ {
		x := int32(10 + (u%40)*3)
		y := int32(10 + (u/40)*3)
		id := addMover(b, w, x, y, fixed.One/2)
		w.StartMoveTo(id, CellCenter(cellIdx(450, 450)))
	}
	w.Step()
	b.ReportAllocs()
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		w.movementSystem()
	}
}

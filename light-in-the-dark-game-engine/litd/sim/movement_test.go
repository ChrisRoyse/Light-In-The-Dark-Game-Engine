package sim

import (
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim/path"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/statehash"
)

func moverWorld(t *testing.T, speed fixed.F64, turnRate fixed.Angle, pos fixed.Vec2) (*World, EntityID) {
	t.Helper()
	w := NewWorld(Caps{})
	id, ok := w.CreateUnit(pos, 0)
	if !ok || !w.Movements.Add(w.Ents, w.Transforms, id, speed, turnRate) {
		t.Fatal("mover setup failed")
	}
	return w, id
}

// storePath puts a hand-built waypoint list into the world's pool.
func storePath(t *testing.T, w *World, cells ...int32) path.PathID {
	t.Helper()
	pid, buf, ok := w.Paths.Acquire(path.Rect{X: 0, Y: 0, W: 1, H: 1})
	if !ok {
		t.Fatal("path pool exhausted")
	}
	buf = append(buf, cells...)
	w.Paths.SetWaypoints(pid, buf)
	return pid
}

// Edge 1: unit exactly ON the waypoint at tick start pops it without
// overshoot.
func TestMovementExactlyOnWaypoint(t *testing.T) {
	cell0 := int32(10 + 10*path.GridSize)
	cell1 := int32(20 + 10*path.GridSize)
	w, id := moverWorld(t, 4*fixed.One, 0x4000, CellCenter(cell0)) // spawn ON wp0
	pid := storePath(t, w, cell0, cell1)
	w.StartPath(id, pid)
	r := w.Movements.Row(id)
	tr := w.Transforms.Row(id)
	t.Logf("before: pos=(%d,%d) target=(%d,%d) wpIdx=%d state=%d",
		w.Transforms.Pos[tr].X, w.Transforms.Pos[tr].Y,
		w.Movements.Target[r].X, w.Movements.Target[r].Y,
		w.Movements.WaypointIdx[r], w.Movements.State[r])
	w.Step()
	t.Logf("after:  pos=(%d,%d) target=(%d,%d) wpIdx=%d state=%d (popped wp0, now heading to wp1, no overshoot)",
		w.Transforms.Pos[tr].X, w.Transforms.Pos[tr].Y,
		w.Movements.Target[r].X, w.Movements.Target[r].Y,
		w.Movements.WaypointIdx[r], w.Movements.State[r])
	if w.Movements.WaypointIdx[r] != 1 || w.Movements.Target[r] != CellCenter(cell1) {
		t.Fatalf("waypoint must pop on exact arrival: idx=%d", w.Movements.WaypointIdx[r])
	}
	if w.Transforms.Pos[tr] != CellCenter(cell0) {
		t.Fatalf("pop tick must not move past the waypoint")
	}
}

// Edge 2: displacement larger than remaining distance clamps to the
// waypoint — 3-tick trace shows no oscillation.
func TestMovementClampNoOscillation(t *testing.T) {
	target := fixed.Vec2{X: 100 * fixed.One, Y: 50 * fixed.One}
	start := fixed.Vec2{X: 99 * fixed.One, Y: 50 * fixed.One} // 1 unit short
	w, id := moverWorld(t, 3*fixed.One, 0x4000, start)        // 3 units/tick
	w.StartMoveTo(id, target)
	tr := w.Transforms.Row(id)
	r := w.Movements.Row(id)
	for tick := 1; tick <= 3; tick++ {
		w.Step()
		t.Logf("tick %d: pos=(%d,%d) state=%d", tick,
			w.Transforms.Pos[tr].X, w.Transforms.Pos[tr].Y, w.Movements.State[r])
		if w.Transforms.Pos[tr] != target {
			t.Fatalf("tick %d: must clamp exactly to target, got (%d,%d)",
				tick, w.Transforms.Pos[tr].X, w.Transforms.Pos[tr].Y)
		}
	}
	if w.Movements.State[r] != MoveIdle {
		t.Fatalf("arrival must end the move: state=%d", w.Movements.State[r])
	}
}

// Edge 3: 180° turn at low turn rate converges via the shortest arc
// (counterclockwise on the exact tie), never wrapping the wrong way.
func TestMovementTurn180ShortestArc(t *testing.T) {
	start := fixed.Vec2{X: 1000 * fixed.One, Y: 1000 * fixed.One}
	w, id := moverWorld(t, fixed.One/4, 0x1000, start) // slow walk, 1/16 turn per tick
	// target due WEST: desired facing 0x8000, exactly 180° from 0
	w.StartMoveTo(id, fixed.Vec2{X: 200 * fixed.One, Y: 1000 * fixed.One})
	tr := w.Transforms.Row(id)
	seq := []fixed.Angle{w.Transforms.Facing[tr]}
	for tick := 0; tick < 10; tick++ {
		w.Step()
		seq = append(seq, w.Transforms.Facing[tr])
	}
	t.Logf("facing sequence (BAM): %#04x %#04x %#04x %#04x %#04x %#04x %#04x %#04x %#04x %#04x %#04x",
		uint16(seq[0]), uint16(seq[1]), uint16(seq[2]), uint16(seq[3]), uint16(seq[4]),
		uint16(seq[5]), uint16(seq[6]), uint16(seq[7]), uint16(seq[8]), uint16(seq[9]), uint16(seq[10]))
	for i := 1; i <= 8; i++ {
		want := fixed.Angle(i * 0x1000)
		if i >= 8 {
			want = 0x8000
		}
		if seq[i] != want {
			t.Fatalf("tick %d: facing %#04x want %#04x (must climb +0x1000 ccw, no wrong-way wrap)",
				i, uint16(seq[i]), uint16(want))
		}
	}
	if seq[9] != 0x8000 || seq[10] != 0x8000 {
		t.Fatalf("facing must hold at 0x8000 after converging")
	}
}

// Edge 4: two 1,000-tick marches produce bit-identical position
// traces.
func TestMovementThousandTickBitIdentical(t *testing.T) {
	run := func() uint64 {
		w := NewWorld(Caps{})
		h := statehash.New()
		for u := 0; u < 8; u++ {
			pos := fixed.Vec2{X: fixed.FromInt(int32(100 + u*40)), Y: fixed.FromInt(int32(60 + u*24))}
			id, _ := w.CreateUnit(pos, fixed.Angle(u*0x2000))
			w.Movements.Add(w.Ents, w.Transforms, id, fixed.One+fixed.F64(u)*fixed.One/8, fixed.Angle(0x0400+u*0x80))
			// zig-zag path across the map
			cells := []int32{}
			for k := int32(1); k <= 6; k++ {
				x := (int32(u)*7+k*53)%path.GridSize - 0
				y := (k*37 + int32(u)*11) % path.GridSize
				cells = append(cells, y*path.GridSize+x)
			}
			pid := func() path.PathID {
				pid, buf, _ := w.Paths.Acquire(path.Rect{X: 0, Y: 0, W: 1, H: 1})
				buf = append(buf, cells...)
				w.Paths.SetWaypoints(pid, buf)
				return pid
			}()
			w.StartPath(id, pid)
		}
		for tick := 0; tick < 1000; tick++ {
			w.Step()
			for r := int32(0); r < w.Transforms.Count(); r++ {
				h.WriteI64(int64(w.Transforms.Pos[r].X))
				h.WriteI64(int64(w.Transforms.Pos[r].Y))
				h.WriteU16(uint16(w.Transforms.Facing[r]))
			}
		}
		return h.Sum64()
	}
	h1, h2 := run(), run()
	t.Logf("1,000-tick march of 8 units: trace hash run1=%016x run2=%016x", h1, h2)
	if h1 != h2 {
		t.Fatalf("position traces diverged: %016x vs %016x", h1, h2)
	}
}

// Raw per-tick trace: straight east march, displacement exactly
// Speed per tick (X+X=Y: 2 units/tick × 5 ticks = 10 units).
func TestMovementRawTrace(t *testing.T) {
	start := fixed.Vec2{X: 100 * fixed.One, Y: 200 * fixed.One}
	w, id := moverWorld(t, 2*fixed.One, 0x4000, start)
	w.StartMoveTo(id, fixed.Vec2{X: 300 * fixed.One, Y: 200 * fixed.One})
	tr := w.Transforms.Row(id)
	prev := w.Transforms.Pos[tr]
	for tick := 1; tick <= 5; tick++ {
		w.Step()
		cur := w.Transforms.Pos[tr]
		t.Logf("tick %d: X=%d Y=%d (dX=%d, want %d)", tick, cur.X, cur.Y, cur.X-prev.X, 2*fixed.One)
		if cur.X-prev.X != 2*fixed.One || cur.Y != 200*fixed.One {
			t.Fatalf("tick %d: displacement wrong: dX=%d", tick, cur.X-prev.X)
		}
		prev = cur
	}
}

// Final waypoint emits the order-completion event exactly once.
func TestMovementCompletionEvent(t *testing.T) {
	cell := int32(12 + 12*path.GridSize)
	w, id := moverWorld(t, 50*fixed.One, 0x4000, fixed.Vec2{X: CellCenter(cell).X - 10*fixed.One, Y: CellCenter(cell).Y})
	pid := storePath(t, w, cell)
	w.StartPath(id, pid)
	done := 0
	w.RegisterHandler(1, func(ww *World, e Event) {
		done++
		if e.Src != id {
			t.Fatalf("EvMoveDone for wrong entity")
		}
	})
	w.Subscribe(EvMoveDone, 1)
	for tick := 0; tick < 4; tick++ {
		w.Step()
	}
	t.Logf("EvMoveDone fired %d time(s); path slot live=%v (released)", done, w.Paths.Valid(pid))
	if done != 1 {
		t.Fatalf("completion event must fire exactly once: %d", done)
	}
	if w.Paths.Valid(pid) {
		t.Fatalf("path must be released on completion")
	}
}

// Zero allocations with 256 active movers.
func TestMovementZeroAlloc(t *testing.T) {
	w := NewWorld(Caps{})
	for u := 0; u < 256; u++ {
		pos := fixed.Vec2{X: fixed.FromInt(int32(u * 30)), Y: fixed.One}
		id, _ := w.CreateUnit(pos, 0)
		w.Movements.Add(w.Ents, w.Transforms, id, fixed.One/2, 0x0200)
		w.StartMoveTo(id, fixed.Vec2{X: 8000 * fixed.One, Y: 8000 * fixed.One})
	}
	for i := 0; i < allocWarmupTicks; i++ {
		w.Step()
	}
	allocs := testing.AllocsPerRun(100, func() { w.Step() })
	t.Logf("AllocsPerRun(Step with 256 movers) = %v", allocs)
	if allocs != 0 {
		t.Fatalf("movement allocated: %v", allocs)
	}
}

func BenchmarkMovementStep(b *testing.B) {
	w := NewWorld(Caps{})
	for u := 0; u < 1000; u++ {
		pos := fixed.Vec2{X: fixed.FromInt(int32(u * 16)), Y: fixed.One}
		id, _ := w.CreateUnit(pos, 0)
		w.Movements.Add(w.Ents, w.Transforms, id, fixed.One/2, 0x0200)
		w.StartMoveTo(id, fixed.Vec2{X: 9000 * fixed.One, Y: 9000 * fixed.One})
	}
	w.Step()
	b.ReportAllocs()
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		w.movementSystem()
	}
}

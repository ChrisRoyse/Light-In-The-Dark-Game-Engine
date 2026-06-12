package sim

import (
	"fmt"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
)

func snapEntry(s *Snapshot, id EntityID) (SnapshotEntry, bool) {
	for i := range s.Entries {
		if s.Entries[i].ID == id {
			return s.Entries[i], true
		}
	}
	return SnapshotEntry{}, false
}

// Edge 1: unit moves one cell between ticks — prev/curr snapshots
// differ by exactly the displacement, and the moved tick does NOT
// carry the no-lerp flag (render interpolates it).
func TestSnapshotMoveDelta(t *testing.T) {
	w := NewWorld(Caps{})
	id, _ := w.CreateUnit(fixed.Vec2{X: 5 * fixed.One, Y: 5 * fixed.One}, 0)
	w.Step() // snapshot 1: spawn pose
	r := w.Transforms.Row(id)
	w.Transforms.Pos[r].X += fixed.One // one cell right, "movement system"
	w.Step()                           // snapshot 2: moved pose
	prev, _ := snapEntry(w.Snaps.Prev(), id)
	curr, _ := snapEntry(w.Snaps.Curr(), id)
	t.Logf("prev (tick %d): pos=(%d,%d) flags=%02x", w.Snaps.Prev().Tick, prev.Pos.X, prev.Pos.Y, prev.Flags)
	t.Logf("curr (tick %d): pos=(%d,%d) flags=%02x", w.Snaps.Curr().Tick, curr.Pos.X, curr.Pos.Y, curr.Flags)
	t.Logf("delta = (%d,%d), want exactly (%d,0) = one cell in 32.32", curr.Pos.X-prev.Pos.X, curr.Pos.Y-prev.Pos.Y, int64(fixed.One))
	if curr.Pos.X-prev.Pos.X != fixed.One || curr.Pos.Y != prev.Pos.Y {
		t.Fatalf("displacement wrong")
	}
	if curr.Flags&SnapNoLerp != 0 {
		t.Fatalf("ordinary movement must interpolate: flags=%02x", curr.Flags)
	}
}

// Edge 2: teleport sets the snap flag for exactly one snapshot.
func TestSnapshotTeleportSnaps(t *testing.T) {
	w := NewWorld(Caps{})
	id, _ := w.CreateUnit(fixed.Vec2{X: fixed.One, Y: fixed.One}, 0)
	w.Step() // flush the spawn snap
	w.Step() // a clean ordinary tick
	before, _ := snapEntry(w.Snaps.Curr(), id)
	w.TeleportUnit(id, fixed.Vec2{X: 100 * fixed.One, Y: 200 * fixed.One})
	w.Step()
	after, _ := snapEntry(w.Snaps.Curr(), id)
	w.Step()
	next, _ := snapEntry(w.Snaps.Curr(), id)
	t.Logf("before teleport: pos=(%d,%d) flags=%02x", before.Pos.X, before.Pos.Y, before.Flags)
	t.Logf("teleport tick:   pos=(%d,%d) flags=%02x (SnapNoLerp=%02x set)", after.Pos.X, after.Pos.Y, after.Flags, SnapNoLerp)
	t.Logf("tick after:      pos=(%d,%d) flags=%02x (flag cleared)", next.Pos.X, next.Pos.Y, next.Flags)
	if before.Flags&SnapNoLerp != 0 {
		t.Fatalf("pre-teleport snapshot must lerp")
	}
	if after.Flags&SnapNoLerp == 0 || after.Pos.X != 100*fixed.One {
		t.Fatalf("teleport tick must carry SnapNoLerp at the new position")
	}
	if next.Flags&SnapNoLerp != 0 {
		t.Fatalf("snap flag must clear after one snapshot")
	}
}

// Edge 3: entity dies tick N — present in snapshot N with the death
// cue, absent from snapshot N+1.
func TestSnapshotDeathCue(t *testing.T) {
	w := NewWorld(Caps{})
	id, _ := w.CreateUnit(fixed.Vec2{X: 3 * fixed.One, Y: 4 * fixed.One}, 0)
	w.Step()
	w.KillUnit(id)
	w.Step() // tick N: killed this tick
	dying, present := snapEntry(w.Snaps.Curr(), id)
	tickN := w.Snaps.Curr().Tick
	w.Step() // tick N+1
	_, stillThere := snapEntry(w.Snaps.Curr(), id)
	t.Logf("snapshot N (tick %d): present=%v flags=%02x (SnapDeath=%02x SnapNoLerp=%02x) pos=(%d,%d)",
		tickN, present, dying.Flags, SnapDeath, SnapNoLerp, dying.Pos.X, dying.Pos.Y)
	t.Logf("snapshot N+1 (tick %d): present=%v entries=%d", w.Snaps.Curr().Tick, stillThere, len(w.Snaps.Curr().Entries))
	if !present || dying.Flags&SnapDeath == 0 || dying.Flags&SnapNoLerp == 0 {
		t.Fatalf("death tick must include the entity with death cue: present=%v flags=%02x", present, dying.Flags)
	}
	if stillThere {
		t.Fatalf("entity must be absent the tick after death")
	}
}

// Edge 4: buffers ping-pong with stable backing pointers.
func TestSnapshotBuffersPingPong(t *testing.T) {
	w := NewWorld(Caps{})
	w.CreateUnit(fixed.Vec2{}, 0)
	w.Step()
	a := fmt.Sprintf("%p", w.Snaps.Curr().Entries[:1])
	w.Step()
	b := fmt.Sprintf("%p", w.Snaps.Curr().Entries[:1])
	w.Step()
	c := fmt.Sprintf("%p", w.Snaps.Curr().Entries[:1])
	w.Step()
	d := fmt.Sprintf("%p", w.Snaps.Curr().Entries[:1])
	t.Logf("curr buffer address by tick: %s %s %s %s (published=%d)", a, b, c, d, w.Snaps.Published())
	if a == b || c != a || d != b {
		t.Fatalf("buffers must alternate between two stable arrays: %s %s %s %s", a, b, c, d)
	}
	if w.Snaps.Prev().Tick+1 != w.Snaps.Curr().Tick {
		t.Fatalf("prev/curr must be consecutive ticks: %d, %d", w.Snaps.Prev().Tick, w.Snaps.Curr().Tick)
	}
}

func todForSnapshotTest(q uint16) fixed.F64 {
	return fixed.F64(uint64(q) * clockDayRaw / 65536)
}

func TestSnapshotTimeOfDayBeforeFirstPublishIsMidnight(t *testing.T) {
	w := NewWorld(Caps{})
	prev, curr := w.Snaps.Prev(), w.Snaps.Curr()
	t.Logf("FSV ToD before first publish prev: tick=%d tod=%d entries=%d", prev.Tick, prev.TimeOfDay, len(prev.Entries))
	t.Logf("FSV ToD before first publish curr: tick=%d tod=%d entries=%d", curr.Tick, curr.TimeOfDay, len(curr.Entries))
	if prev.TimeOfDay != 0 || curr.TimeOfDay != 0 || prev.Tick != 0 || curr.Tick != 0 {
		t.Fatalf("zero snapshot must be midnight tick 0: prev=%+v curr=%+v", prev, curr)
	}
}

func TestSnapshotTimeOfDayWrapDelta(t *testing.T) {
	w := NewWorld(Caps{})
	inc, _ := clockAdvance(fixed.One, DefaultDayLengthTicks, 0)
	w.SetTimeOfDay(todForSnapshotTest(65534) - fixed.F64(inc))
	before := clockFSV(w)
	w.Step()
	w.Step()
	prev, curr := w.Snaps.Prev(), w.Snaps.Curr()
	delta := int16(curr.TimeOfDay - prev.TimeOfDay)
	t.Logf("FSV ToD wrap BEFORE: %s", before)
	t.Logf("FSV ToD wrap Prev: tick=%d tod=%d", prev.Tick, prev.TimeOfDay)
	t.Logf("FSV ToD wrap Curr: tick=%d tod=%d delta(int16)=%d", curr.Tick, curr.TimeOfDay, delta)
	if prev.TimeOfDay < 65520 || curr.TimeOfDay > 16 {
		t.Fatalf("expected wrap-adjacent snapshots, got prev=%d curr=%d", prev.TimeOfDay, curr.TimeOfDay)
	}
	if delta <= 0 || delta > 32 {
		t.Fatalf("wrap delta=%d, want small positive shortest-ring delta", delta)
	}
}

func TestSnapshotTimeOfDayFrozenHeader(t *testing.T) {
	w := NewWorld(Caps{})
	w.SetTimeOfDay(9 * fixed.One)
	w.SuspendTimeOfDay(true)
	w.Step()
	w.Step()
	prev, curr := w.Snaps.Prev(), w.Snaps.Curr()
	want := uint16(24576) // 9/24 * 65536
	t.Logf("FSV ToD frozen Prev: tick=%d tod=%d", prev.Tick, prev.TimeOfDay)
	t.Logf("FSV ToD frozen Curr: tick=%d tod=%d", curr.Tick, curr.TimeOfDay)
	if prev.TimeOfDay != want || curr.TimeOfDay != want {
		t.Fatalf("frozen snapshots changed ToD: prev=%d curr=%d want=%d", prev.TimeOfDay, curr.TimeOfDay, want)
	}
}

// Life fraction quantization: known life values produce known u16
// fractions (X+X=Y: 50% of 100 = 32767).
func TestSnapshotLifeFraction(t *testing.T) {
	w := NewWorld(Caps{})
	id, _ := w.CreateUnit(fixed.Vec2{}, 0)
	w.Healths.Add(w.Ents, id, 100*fixed.One, 0, 0, 0)
	hr := w.Healths.Row(id)
	cases := []struct {
		life fixed.F64
		want uint16
	}{
		{100 * fixed.One, 65535}, // full
		{50 * fixed.One, 32767},  // half: 50*65535/100 truncates
		{0, 0},                   // empty
		{-5 * fixed.One, 0},      // below zero clamps
		{200 * fixed.One, 65535}, // above max clamps
	}
	for _, c := range cases {
		w.Healths.Life[hr] = c.life
		w.Step()
		e, _ := snapEntry(w.Snaps.Curr(), id)
		t.Logf("life=%d/%d -> LifeFrac=%d (want %d)", c.life/fixed.One, 100, e.LifeFrac, c.want)
		if e.LifeFrac != c.want {
			t.Fatalf("life fraction wrong: got %d want %d", e.LifeFrac, c.want)
		}
	}
}

// Render events: staged cues land in this tick's snapshot, stamped,
// and do not leak into the next.
func TestSnapshotRenderEvents(t *testing.T) {
	w := NewWorld(Caps{})
	id, _ := w.CreateUnit(fixed.Vec2{}, 0)
	w.EmitRenderEvent(7, id, 42)
	w.EmitRenderEvent(9, id, 43)
	w.Step()
	evs := w.Snaps.Curr().Events
	t.Logf("snapshot tick %d events: %+v", w.Snaps.Curr().Tick, evs)
	if len(evs) != 2 || evs[0].Kind != 7 || evs[0].Tick != w.Snaps.Curr().Tick || evs[1].Data != 43 {
		t.Fatalf("staged events must publish tick-tagged: %+v", evs)
	}
	w.Step()
	if len(w.Snaps.Curr().Events) != 0 {
		t.Fatalf("events must not leak into the next snapshot: %+v", w.Snaps.Curr().Events)
	}
}

// Zero allocations per publish at steady state.
func TestSnapshotPublishZeroAlloc(t *testing.T) {
	w := NewWorld(Caps{})
	for i := 0; i < 256; i++ {
		id, _ := w.CreateUnit(fixed.Vec2{X: fixed.F64(i) * fixed.One, Y: fixed.One}, 0)
		w.Healths.Add(w.Ents, id, 100*fixed.One, 0, 0, 0)
		w.Movements.Add(w.Ents, w.Transforms, id, fixed.One, 100)
	}
	for i := 0; i < allocWarmupTicks; i++ {
		w.Step()
	}
	allocs := testing.AllocsPerRun(100, func() {
		w.EmitRenderEvent(1, 0, 0)
		w.Step()
	})
	t.Logf("AllocsPerRun(Step incl. publish of 256 entities + render event) = %v", allocs)
	if allocs != 0 {
		t.Fatalf("publish allocated: %v", allocs)
	}
}

func BenchmarkSnapshotPublish(b *testing.B) {
	w := NewWorld(Caps{})
	for i := 0; i < 1000; i++ {
		id, _ := w.CreateUnit(fixed.Vec2{X: fixed.F64(i) * fixed.One, Y: fixed.One}, 0)
		w.Healths.Add(w.Ents, id, 100*fixed.One, 0, 0, 0)
	}
	w.Step()
	b.ReportAllocs()
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		w.publishSnapshot()
	}
}

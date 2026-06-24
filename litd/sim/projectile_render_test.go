package sim

// #590 — render-only projectile surface FSV. SoT = the published
// Snapshot.Missiles / Snapshot.Entries slices read directly after a
// publish: a mover-driven projectile body publishes as an arced billboard
// (NOT a unit model), its Pos tracks the live transform, its Arc/Guidance
// come from ProjRender, its LifeFrac reflects flight progress, and on body
// death the record is reclaimed and the billboard disappears.

import (
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
)

func TestProjectileRenderPublishFSV(t *testing.T) {
	w := NewWorld(Caps{Units: 8, Movers: 8, Projectiles: 8})
	const arc = 64 * fixed.One
	const span = int32(200)
	body, _ := w.CreateUnit(fixed.Vec2{X: 1000 * fixed.One, Y: 1000 * fixed.One}, 0)
	mid := w.Movers.Create(MoverSpec{
		Kind: MoverLinear, Target: body,
		Dir: fixed.Vec2{X: fixed.One}, Speed: 50 * fixed.One, RangeLeft: fixed.FromInt(span),
		Flags: MoverConsume,
	})
	if !w.ProjRender.Add(body, mid, arc, MissileGuidanceLinear, span) {
		t.Fatal("ProjRender.Add failed")
	}

	// BEFORE: a real (model) unit alongside, to prove the projectile is excluded
	// from Entries but the unit is not.
	unit, _ := w.CreateUnit(fixed.Vec2{X: 5000 * fixed.One}, 0)

	w.Step() // advances the mover + publishes (phase 7)
	snap := w.Snaps.Curr()
	t.Logf("FSV: entries=%d missiles=%d", len(snap.Entries), len(snap.Missiles))

	// SoT 1: the projectile is published as a billboard, Pos == live transform.
	if len(snap.Missiles) != 1 {
		t.Fatalf("snapshot billboards = %d, want 1 (the mover projectile)", len(snap.Missiles))
	}
	me := snap.Missiles[0]
	if me.ID != body {
		t.Fatalf("billboard ID = %d, want body %d", me.ID, body)
	}
	tr := w.Transforms.Row(body)
	if me.Pos != w.Transforms.Pos[tr] {
		t.Fatalf("billboard pos %v != live transform %v", me.Pos, w.Transforms.Pos[tr])
	}
	if me.Arc != arc || me.GuidanceID != MissileGuidanceLinear {
		t.Fatalf("billboard render statics wrong: arc=%d guid=%d", me.Arc.Floor(), me.GuidanceID)
	}
	// SoT 2: LifeFrac reflects flight progress. After one 50-unit step of a
	// 200-unit flight, traveled=50 → 50*65535/200 = 16383.
	if me.LifeFrac != uint16(50*65535/200) {
		t.Fatalf("LifeFrac = %d, want %d (50/200 traveled)", me.LifeFrac, uint16(50*65535/200))
	}
	t.Logf("FSV billboard: pos=(%d,%d) arc=%d guid=%d lifeFrac=%d",
		me.Pos.X.Floor(), me.Pos.Y.Floor(), me.Arc.Floor(), me.GuidanceID, me.LifeFrac)

	// SoT 3: the projectile body is NOT a unit entry; the real unit IS.
	for _, e := range snap.Entries {
		if e.ID == body {
			t.Fatal("projectile body leaked into Snapshot.Entries — would render as a unit model")
		}
	}
	sawUnit := false
	for _, e := range snap.Entries {
		sawUnit = sawUnit || e.ID == unit
	}
	if !sawUnit {
		t.Fatal("real unit missing from Entries")
	}

	// SoT 4: on body death the render record is reclaimed (phase 7 publishes a
	// killed entity once for its final position, then reaps it — so the record
	// is gone after this step and the billboard disappears the NEXT publish,
	// matching missile semantics).
	w.KillUnit(body)
	w.Step() // publishes body's final frame, then reaps it at cleanup
	if w.ProjRender.Row(body) != -1 {
		t.Fatalf("ProjRender record not reclaimed on body death: row=%d", w.ProjRender.Row(body))
	}
	w.Step() // first publish after the body is gone
	snap2 := w.Snaps.Curr()
	for _, m := range snap2.Missiles {
		if m.ID == body {
			t.Fatal("dead projectile still published as a billboard")
		}
	}
	t.Logf("after body death: billboards=%d, ProjRender.Count=%d", len(snap2.Missiles), w.ProjRender.Count())
}

// TestProjectileRenderStoreUnit exercises the store's Add/Row/Remove dense
// bookkeeping directly (SoT = Count + rowOf), including double-add reject and
// swap-with-last on a middle remove.
func TestProjectileRenderStoreUnit(t *testing.T) {
	s := NewProjectileRender(4, 16)
	a, b, c := makeEntityID(1, 1), makeEntityID(2, 1), makeEntityID(3, 1)
	if !s.Add(a, 0, fixed.One, 0, 10) || !s.Add(b, 0, fixed.One, 0, 20) || !s.Add(c, 0, fixed.One, 0, 30) {
		t.Fatal("adds failed")
	}
	if s.Add(b, 0, fixed.One, 0, 99) {
		t.Fatal("double-add returned true")
	}
	if s.Count() != 3 {
		t.Fatalf("count=%d want 3", s.Count())
	}
	// remove the middle → last (c) swaps into b's row; rowOf stays consistent.
	if !s.Remove(b) || s.Row(b) != -1 {
		t.Fatal("remove(b) bookkeeping wrong")
	}
	if s.Count() != 2 {
		t.Fatalf("count=%d want 2 after remove", s.Count())
	}
	if r := s.Row(c); r == -1 || s.Entity[r] != c {
		t.Fatalf("c not resolvable after swap: row=%d", r)
	}
	if r := s.Row(a); r == -1 || s.Entity[r] != a {
		t.Fatalf("a corrupted: row=%d", r)
	}
}

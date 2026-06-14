package sim

import (
	"bytes"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/statehash"
)

// pauseSubIndex returns the registration index of the "pause" sub-hash so a
// test can read that exact slice of the snapshot (the FSV Source of Truth for
// the persisted pause set).
func pauseSubIndex(t *testing.T) int {
	t.Helper()
	for i, n := range HashSystems {
		if n == "pause" {
			return i
		}
	}
	t.Fatal(`"pause" not registered in HashSystems`)
	return -1
}

// TestPauseFreezesAndResumes is the behavioral FSV: a paused unit holds its
// position (orders/movement frozen); resuming lets it move again. Source of
// truth = the unit's transform position read straight from the store before and
// after each Step batch (X+X=Y: a known move toward a known cell either
// advances or it does not).
func TestPauseFreezesAndResumes(t *testing.T) {
	w := NewWorld(Caps{})
	w.SetGrid(openGrid())
	u := orderUnit(t, w, 10, 10, 16*fixed.One)
	w.OccupyCell(u)
	w.IssueOrder(u, Order{Kind: OrderMove, Point: CellCenter(cellIdx(40, 10))}, false)

	posOf := func() fixed.Vec2 { return w.Transforms.Pos[w.Transforms.Row(u)] }
	start := posOf()

	// 1) move a few ticks unpaused — must travel.
	for i := 0; i < 8; i++ {
		w.Step()
	}
	moving := posOf()
	t.Logf("unpaused: start=(%d,%d) after 8 ticks=(%d,%d)", start.X, start.Y, moving.X, moving.Y)
	if moving == start {
		t.Fatal("degenerate fixture: unit never started moving while unpaused")
	}

	// 2) pause — store SoT: present, count 1.
	if !w.PauseUnit(u, true) {
		t.Fatal("PauseUnit(true) returned false on a live unit")
	}
	if !w.IsUnitPaused(u) || w.Pauses.Count() != 1 {
		t.Fatalf("after pause: IsUnitPaused=%v count=%d, want true/1", w.IsUnitPaused(u), w.Pauses.Count())
	}
	frozen := posOf()
	for i := 0; i < 20; i++ {
		w.Step()
	}
	afterPause := posOf()
	t.Logf("paused 20 ticks: frozen=(%d,%d) after=(%d,%d)", frozen.X, frozen.Y, afterPause.X, afterPause.Y)
	if afterPause != frozen {
		t.Fatalf("paused unit MOVED: (%d,%d) -> (%d,%d)", frozen.X, frozen.Y, afterPause.X, afterPause.Y)
	}

	// 3) resume — store SoT: absent, count 0; unit travels again.
	if !w.PauseUnit(u, false) {
		t.Fatal("PauseUnit(false) returned false")
	}
	if w.IsUnitPaused(u) || w.Pauses.Count() != 0 {
		t.Fatalf("after resume: IsUnitPaused=%v count=%d, want false/0", w.IsUnitPaused(u), w.Pauses.Count())
	}
	for i := 0; i < 10; i++ {
		w.Step()
	}
	afterResume := posOf()
	t.Logf("resumed 10 ticks: after=(%d,%d)", afterResume.X, afterResume.Y)
	if afterResume == afterPause {
		t.Fatal("resumed unit did not move")
	}
}

// TestPauseSurvivesSaveLoad is the persistence FSV: the pause bit round-trips
// through SaveState/LoadState. Source of truth = the restored world's Pauses
// store AND the "pause" sub-hash, which must be byte-identical to the source.
func TestPauseSurvivesSaveLoad(t *testing.T) {
	build := func() (*World, EntityID) {
		w := NewWorld(Caps{})
		w.SetGrid(openGrid())
		u := orderUnit(t, w, 10, 10, 16*fixed.One)
		w.OccupyCell(u)
		return w, u
	}
	src, u := build()
	if !src.PauseUnit(u, true) {
		t.Fatal("pause setup failed")
	}
	if src.Pauses.Count() != 1 {
		t.Fatalf("src pause count=%d want 1", src.Pauses.Count())
	}

	var buf bytes.Buffer
	if err := src.SaveState(&buf, 0); err != nil {
		t.Fatalf("SaveState: %v", err)
	}
	t.Logf("saved %d bytes; src IsUnitPaused=%v", buf.Len(), src.IsUnitPaused(u))

	dst, _ := build()
	if dst.IsUnitPaused(u) {
		t.Fatal("precondition: fresh dst already reports unit paused")
	}
	if err := dst.LoadState(bytes.NewReader(buf.Bytes()), 0); err != nil {
		t.Fatalf("LoadState: %v", err)
	}

	if !dst.IsUnitPaused(u) || dst.Pauses.Count() != 1 {
		t.Fatalf("after load: IsUnitPaused=%v count=%d, want true/1", dst.IsUnitPaused(u), dst.Pauses.Count())
	}

	// Sub-hash equality at the "pause" slot.
	reg := NewHashRegistry()
	var hs, hd statehash.Snapshot
	src.HashState(reg, &hs)
	dst.HashState(reg, &hd)
	pi := pauseSubIndex(t)
	t.Logf("pause sub-hash: src=%016x dst=%016x (top src=%016x dst=%016x)", hs.Subs[pi], hd.Subs[pi], hs.Top, hd.Top)
	if hs.Subs[pi] != hd.Subs[pi] {
		t.Fatalf("pause sub-hash diverged across save/load: %016x != %016x", hs.Subs[pi], hd.Subs[pi])
	}
}

// TestPauseEdges covers the boundary/edge audit: dead handle, double-pause
// idempotency, resume-never-paused (no underflow), and the all-active empty
// set. Each prints the store state before and after.
func TestPauseEdges(t *testing.T) {
	w := NewWorld(Caps{})
	w.SetGrid(openGrid())
	u := orderUnit(t, w, 5, 5, 16*fixed.One)

	// Edge 1: pause a dead/never-alive entity — must reject, count unchanged.
	dead := EntityID(0xDEAD0001)
	before := w.Pauses.Count()
	got := w.PauseUnit(dead, true)
	t.Logf("edge dead: PauseUnit(dead,true)=%v count %d->%d", got, before, w.Pauses.Count())
	if got || w.Pauses.Count() != before {
		t.Fatalf("pausing a dead entity must be a no-op false: got=%v count=%d", got, w.Pauses.Count())
	}

	// Edge 2: double pause — idempotent, count stays 1 (no duplicate row).
	w.PauseUnit(u, true)
	c1 := w.Pauses.Count()
	w.PauseUnit(u, true)
	c2 := w.Pauses.Count()
	t.Logf("edge double-pause: count after 1st=%d after 2nd=%d", c1, c2)
	if c1 != 1 || c2 != 1 {
		t.Fatalf("double pause not idempotent: %d then %d, want 1/1", c1, c2)
	}

	// Edge 3: resume twice — second resume on an already-active unit is a
	// harmless no-op, count floors at 0 (no underflow).
	w.PauseUnit(u, false)
	r1 := w.Pauses.Count()
	ok := w.PauseUnit(u, false)
	r2 := w.Pauses.Count()
	t.Logf("edge double-resume: count after 1st=%d after 2nd=%d (ret=%v)", r1, r2, ok)
	if r1 != 0 || r2 != 0 || !ok {
		t.Fatalf("double resume mishandled: %d then %d ret=%v, want 0/0/true", r1, r2, ok)
	}
}

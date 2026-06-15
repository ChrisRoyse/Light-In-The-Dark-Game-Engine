package sim

// #229 destructable sim FSV. SoT = the destructable store rows + the pathing
// grid cell flags, read back directly, through a save/load round-trip
// (byte-identical state hash), and through two independent runs (determinism).

import (
	"bytes"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim/path"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/statehash"
)

// destGrid bakes Walkable over cells [18,46)² so footprint stamps are visible.
func destGrid() *path.Grid {
	g := path.NewGrid()
	for y := int32(18); y < 46; y++ {
		for x := int32(18); x < 46; x++ {
			g.SetFlags(x, y, path.Walkable)
		}
	}
	return g
}

// cellPos returns the world position at the centre of pathing cell (cx,cy).
func cellPos(cx, cy int32) fixed.Vec2 {
	return fixed.Vec2{X: fixed.FromInt(cx*32 + 16), Y: fixed.FromInt(cy*32 + 16)}
}

// TestDestructableKillFreesPathingFSV — a blocking destructable occupies its
// cell; Kill frees it the SAME tick; Resurrect re-blocks it.
func TestDestructableKillFreesPathingFSV(t *testing.T) {
	w := NewWorld(Caps{Units: 8})
	w.SetGrid(destGrid())
	const cx, cy = 30, 30

	t.Logf("FSV cell(%d,%d) walkable before create: %v (baked walkable, unoccupied)", cx, cy, w.Grid.CellWalkable(cx, cy))
	if !w.Grid.CellWalkable(cx, cy) {
		t.Fatal("baked cell should start walkable")
	}

	id := w.CreateDestructable(7, cellPos(cx, cy), 0, 100, true, 1)
	if id == 0 {
		t.Fatal("CreateDestructable failed")
	}
	t.Logf("FSV after create: walkable=%v life=%d dead=%v (want false/100/false)",
		w.Grid.CellWalkable(cx, cy), w.DestructableLife(id), w.DestructableDead(id))
	if w.Grid.CellWalkable(cx, cy) {
		t.Fatal("blocking destructable should occupy its cell")
	}
	if w.DestructableLife(id) != 100 || w.DestructableDead(id) {
		t.Fatal("fresh destructable should be alive at full life")
	}

	if !w.KillDestructable(id) {
		t.Fatal("Kill should succeed on a live destructable")
	}
	t.Logf("FSV after kill: walkable=%v life=%d dead=%v (want true/0/true)",
		w.Grid.CellWalkable(cx, cy), w.DestructableLife(id), w.DestructableDead(id))
	if !w.Grid.CellWalkable(cx, cy) {
		t.Fatal("killed destructable must free its pathing cell same tick")
	}
	if w.DestructableLife(id) != 0 || !w.DestructableDead(id) {
		t.Fatal("killed destructable should be dead at 0 life")
	}

	// edge: double kill is a no-op false
	if w.KillDestructable(id) {
		t.Fatal("second Kill should be a no-op false")
	}

	if !w.ResurrectDestructable(id) {
		t.Fatal("Resurrect should succeed on a dead destructable")
	}
	t.Logf("FSV after resurrect: walkable=%v life=%d dead=%v (want false/100/false)",
		w.Grid.CellWalkable(cx, cy), w.DestructableLife(id), w.DestructableDead(id))
	if w.Grid.CellWalkable(cx, cy) || w.DestructableLife(id) != 100 || w.DestructableDead(id) {
		t.Fatal("resurrected destructable should re-block at full life")
	}

	// edge: invalid handle verbs are safe no-ops
	if w.KillDestructable(99999) || w.SetDestructableLife(99999, 5) || w.DestructableLife(99999) != 0 {
		t.Fatal("invalid-handle verbs must be safe no-ops")
	}
}

// TestDestructableNonBlockingNoStamp — a non-blocking destructable never
// touches pathing.
func TestDestructableNonBlockingNoStamp(t *testing.T) {
	w := NewWorld(Caps{Units: 8})
	w.SetGrid(destGrid())
	const cx, cy = 25, 25
	w.CreateDestructable(3, cellPos(cx, cy), 0, 50, false, 1)
	t.Logf("FSV non-blocking cell walkable: %v (want true)", w.Grid.CellWalkable(cx, cy))
	if !w.Grid.CellWalkable(cx, cy) {
		t.Fatal("non-blocking destructable must not occupy pathing")
	}
}

// TestDestructableSetLifeClamp — SetLife clamps [0,MaxLife] and never auto-kills.
func TestDestructableSetLifeClamp(t *testing.T) {
	w := NewWorld(Caps{Units: 4})
	id := w.CreateDestructable(1, cellPos(30, 30), 0, 80, false, 0)
	w.SetDestructableLife(id, 1000)
	if w.DestructableLife(id) != 80 {
		t.Fatalf("over-max not clamped: %d", w.DestructableLife(id))
	}
	w.SetDestructableLife(id, -5)
	if w.DestructableLife(id) != 0 {
		t.Fatalf("negative not clamped: %d", w.DestructableLife(id))
	}
	if w.DestructableDead(id) {
		t.Fatal("life 0 must not auto-kill (death is explicit)")
	}
}

// TestDestructableDeterminism — two identical runs produce the identical
// state hash, including the destructable sub-hash.
func TestDestructableDeterminism(t *testing.T) {
	run := func() statehash.Snapshot {
		w := NewWorld(Caps{Units: 8})
		w.SetGrid(destGrid())
		a := w.CreateDestructable(7, cellPos(30, 30), fixed.Angle(100), 100, true, 1)
		w.CreateDestructable(9, cellPos(31, 31), 0, 60, false, 0)
		w.KillDestructable(a)
		b := w.CreateDestructable(7, cellPos(32, 32), 0, 100, true, 1)
		w.ResurrectDestructable(a)
		w.SetDestructableInvulnerable(b, true)
		reg := NewHashRegistry()
		var snap statehash.Snapshot
		w.HashState(reg, &snap)
		return snap
	}
	s1, s2 := run(), run()
	t.Logf("FSV determinism: run1=%016x run2=%016x", s1.Top, s2.Top)
	if s1.Top != s2.Top {
		t.Fatalf("destructable runs diverged: %016x vs %016x", s1.Top, s2.Top)
	}
}

// TestDestructableSaveRoundTrip — destructable state (and its pathing stamp)
// survives save/load byte-identically (state hash).
func TestDestructableSaveRoundTrip(t *testing.T) {
	w := NewWorld(Caps{Units: 8})
	w.SetGrid(destGrid())
	live := w.CreateDestructable(7, cellPos(30, 30), fixed.Angle(250), 100, true, 1)
	dead := w.CreateDestructable(7, cellPos(32, 32), 0, 100, true, 1)
	w.CreateDestructable(3, cellPos(34, 34), 0, 40, false, 0)
	w.KillDestructable(dead)
	w.SetDestructableLife(live, 55)
	w.SetDestructableInvulnerable(live, true)

	reg := NewHashRegistry()
	var before statehash.Snapshot
	w.HashState(reg, &before)

	var buf bytes.Buffer
	const fp = 0x2299
	if err := w.SaveState(&buf, fp); err != nil {
		t.Fatalf("save: %v", err)
	}
	w2 := NewWorld(Caps{Units: 8})
	w2.SetGrid(destGrid())
	if err := w2.LoadState(bytes.NewReader(buf.Bytes()), fp); err != nil {
		t.Fatalf("load: %v", err)
	}
	var after statehash.Snapshot
	w2.HashState(reg, &after)

	t.Logf("FSV reload: live life=%d invuln=%v walkable@30=%v; dead dead=%v walkable@32=%v",
		w2.DestructableLife(live), w2.DestructableInvulnerable(live),
		w2.Grid.CellWalkable(30, 30), w2.DestructableDead(dead), w2.Grid.CellWalkable(32, 32))
	t.Logf("FSV hash: orig=%016x reload=%016x", before.Top, after.Top)

	if w2.DestructableLife(live) != 55 || !w2.DestructableInvulnerable(live) {
		t.Fatal("live destructable state not restored")
	}
	if !w2.DestructableDead(dead) {
		t.Fatal("dead destructable not restored")
	}
	if w2.Grid.CellWalkable(30, 30) {
		t.Fatal("restored live blocker should re-occupy its cell")
	}
	if !w2.Grid.CellWalkable(32, 32) {
		t.Fatal("restored dead destructable should not occupy its cell")
	}
	if before.Top != after.Top {
		t.Fatalf("state hash diverged across save/load: %016x vs %016x", before.Top, after.Top)
	}
}

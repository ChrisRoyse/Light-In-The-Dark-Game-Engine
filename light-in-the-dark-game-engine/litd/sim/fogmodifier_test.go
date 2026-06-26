package sim

// #243 fog-of-war override FSV. SoT = the per-player visibility grid cell
// read back via FogStateAt after recompute/save. Default-inert checks prove
// the golden path is untouched; explicit overrides change exactly the cells
// expected.

import (
	"bytes"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/statehash"
)

// fogCellWorld returns the world-space center of a fog cell.
func fogCellWorld(fx, fy int32) fixed.Vec2 { return fogCellCenter(fogCellIndex(fx, fy)) }

// TestFogModifierVisibleFSV — a FogVisible rect modifier reveals empty cells
// for a player with no units; Stop and Destroy revert / invalidate.
func TestFogModifierVisibleFSV(t *testing.T) {
	w := visWorld(t)
	const fx, fy int32 = 20, 20
	// no units anywhere → the cell is hidden for player 0.
	w.RecomputeVisibility()
	before := w.FogStateAt(0, fx, fy)
	t.Logf("FSV before: FogStateAt(0, %d,%d)=%d (want %d hidden)", fx, fy, before, FogHidden)
	if before != FogHidden {
		t.Fatalf("expected hidden before modifier, got %d", before)
	}

	c := fogCellWorld(fx, fy)
	id, ok := w.CreateFogModifierRect(0, FogVisible, c.X, c.Y, c.X, c.Y, false, true)
	if !ok {
		t.Fatal("CreateFogModifierRect failed")
	}
	w.RecomputeVisibility()
	got := w.FogStateAt(0, fx, fy)
	t.Logf("FSV active: FogStateAt(0, %d,%d)=%d (want %d visible) active=%v", fx, fy, got, FogVisible, w.FogModifierActive(id))
	if got != FogVisible || !w.FogModifierActive(id) {
		t.Fatalf("modifier did not reveal cell: state=%d active=%v", got, w.FogModifierActive(id))
	}

	// Stop → no longer re-applied; the once-revealed cell demotes to explored
	// (it was visible, now has no live source) — proving the override stopped.
	w.StopFogModifier(id)
	w.RecomputeVisibility()
	stopped := w.FogStateAt(0, fx, fy)
	t.Logf("FSV stopped: FogStateAt=%d (want %d explored, was-visible) active=%v", stopped, FogExplored, w.FogModifierActive(id))
	if stopped != FogExplored || w.FogModifierActive(id) {
		t.Fatalf("stop did not revert: state=%d active=%v", stopped, w.FogModifierActive(id))
	}

	// Destroy → handle invalid, restart is a no-op.
	if !w.DestroyFogModifier(id) {
		t.Fatal("destroy failed")
	}
	t.Logf("FSV destroyed: valid=%v StartFogModifier=%v", w.FogModifierValid(id), w.StartFogModifier(id))
	if w.FogModifierValid(id) || w.StartFogModifier(id) {
		t.Fatal("destroyed modifier still valid")
	}
}

// TestFogModifierRecycleFSV — a destroyed slot is recycled with a bumped
// generation; the stale handle never aliases the new modifier.
func TestFogModifierRecycleFSV(t *testing.T) {
	w := visWorld(t)
	c := fogCellWorld(15, 15)
	id1, _ := w.CreateFogModifierRect(0, FogVisible, c.X, c.Y, c.X, c.Y, false, true)
	w.DestroyFogModifier(id1)
	id2, ok := w.CreateFogModifierRect(1, FogVisible, c.X, c.Y, c.X, c.Y, false, true)
	if !ok {
		t.Fatal("recycle create failed")
	}
	t.Logf("FSV recycle: id1=%#x id2=%#x sameSlot=%v id1valid=%v id2valid=%v",
		uint32(id1), uint32(id2), id1.slot() == id2.slot(), w.FogModifierValid(id1), w.FogModifierValid(id2))
	if id1.slot() != id2.slot() {
		t.Fatalf("expected slot reuse: %d vs %d", id1.slot(), id2.slot())
	}
	if w.FogModifierValid(id1) {
		t.Fatal("stale handle aliases recycled slot")
	}
	if !w.FogModifierValid(id2) {
		t.Fatal("fresh handle invalid")
	}
}

// TestFogModifierCircleFSV — a circular modifier reveals only cells within
// the radius. SoT: center cell visible, a far cell still hidden.
func TestFogModifierCircleFSV(t *testing.T) {
	w := visWorld(t)
	const cx, cy int32 = 25, 25
	center := fogCellWorld(cx, cy)
	// radius 1.5 fog cells worth of world units (~ FogCellPathingSize*pathcell).
	radius := fixed.FromInt(FogCellPathingSize * 32) // one fog cell ≈ 4 path cells * 32 wu
	w.CreateFogModifierRadius(0, FogVisible, center.X, center.Y, radius, false, true)
	w.RecomputeVisibility()
	mid := w.FogStateAt(0, cx, cy)
	far := w.FogStateAt(0, cx+8, cy+8) // well outside the radius
	t.Logf("FSV circle: center(%d,%d)=%d (want visible) far=%d (want hidden) radius=%d", cx, cy, mid, far, radius.Floor())
	if mid != FogVisible {
		t.Fatalf("circle center not revealed: %d", mid)
	}
	if far != FogHidden {
		t.Fatalf("circle leaked outside radius: %d", far)
	}
}

// TestFogModifierSharedFSV — a shared modifier reveals for allied players
// that have the shared-vision flag, and only those.
func TestFogModifierSharedFSV(t *testing.T) {
	w := visWorld(t)
	const fx, fy int32 = 30, 30
	w.SetAllianceFlag(0, 1, AllianceSharedVision, true) // 0 shares vision with 1
	// player 2 gets nothing.
	c := fogCellWorld(fx, fy)
	w.CreateFogModifierRect(0, FogVisible, c.X, c.Y, c.X, c.Y, true, true)
	w.RecomputeVisibility()
	p0 := w.FogStateAt(0, fx, fy)
	p1 := w.FogStateAt(1, fx, fy)
	p2 := w.FogStateAt(2, fx, fy)
	t.Logf("FSV shared: p0=%d p1=%d p2=%d (want visible/visible/hidden)", p0, p1, p2)
	if p0 != FogVisible || p1 != FogVisible {
		t.Fatalf("shared vision not granted: p0=%d p1=%d", p0, p1)
	}
	if p2 != FogHidden {
		t.Fatalf("shared vision leaked to non-ally: p2=%d", p2)
	}
}

// TestSetFogStateInstantFSV — an instant stamp shows immediately but is
// overwritten by the next vision finalize (no modifier lifetime).
func TestSetFogStateInstantFSV(t *testing.T) {
	w := visWorld(t)
	const fx, fy int32 = 18, 18
	c := fogCellWorld(fx, fy)
	w.SetFogStateRect(0, FogVisible, c.X, c.Y, c.X, c.Y, false)
	immediate := w.FogStateAt(0, fx, fy)
	w.RecomputeVisibility() // recompute from sources (none) → reverts
	after := w.FogStateAt(0, fx, fy)
	t.Logf("FSV instant: immediate=%d (want visible) afterRecompute=%d (want explored, was-visible)", immediate, after)
	if immediate != FogVisible {
		t.Fatalf("instant stamp not applied: %d", immediate)
	}
	// it was visible, so finalize demotes to explored (not hidden) — proving
	// the stamp took effect and is no longer being re-applied.
	if after != FogExplored {
		t.Fatalf("instant stamp persisted or vanished wrongly: %d", after)
	}
}

// TestFogTogglesFSV — global fog/mask toggles change query results without
// touching the grid; defaults are inert.
func TestFogTogglesFSV(t *testing.T) {
	w := visWorld(t)
	const fx, fy int32 = 22, 22
	w.RecomputeVisibility()
	t.Logf("FSV default: enabled=%v maskEnabled=%v raw=%d", w.FogEnabled(), w.FogMaskEnabled(), w.FogStateAt(0, fx, fy))
	if !w.FogEnabled() || !w.FogMaskEnabled() {
		t.Fatal("defaults should be fog on / mask on")
	}
	if w.FogStateAt(0, fx, fy) != FogHidden {
		t.Fatalf("default hidden expected, got %d", w.FogStateAt(0, fx, fy))
	}
	// mask off → hidden reads as explored.
	w.SetFogMaskEnabled(false)
	t.Logf("FSV mask-off: FogStateAt=%d (want %d explored)", w.FogStateAt(0, fx, fy), FogExplored)
	if w.FogStateAt(0, fx, fy) != FogExplored {
		t.Fatalf("mask off should report explored, got %d", w.FogStateAt(0, fx, fy))
	}
	// fog off → visible everywhere.
	w.SetFogEnabled(false)
	t.Logf("FSV fog-off: FogStateAt=%d (want %d visible)", w.FogStateAt(0, fx, fy), FogVisible)
	if w.FogStateAt(0, fx, fy) != FogVisible {
		t.Fatalf("fog off should report visible, got %d", w.FogStateAt(0, fx, fy))
	}
}

// TestShareVisionFSV — a unit's sight stamps an allied player's grid only
// while sharing is on.
func TestShareVisionFSV(t *testing.T) {
	w := visWorld(t)
	const cx, cy int32 = 50, 50
	scout := spawnVisUnit(t, w, 0, 0, 0, cx, cy, PathGround) // player 0 scout
	fxc, fyc := fogOf(cx, cy)

	w.RecomputeVisibility()
	t.Logf("FSV before share: p0=%d p1=%d (want visible/hidden)", w.FogStateAt(0, fxc, fyc), w.FogStateAt(1, fxc, fyc))
	if w.FogStateAt(0, fxc, fyc) != FogVisible {
		t.Fatalf("owner cannot see own scout cell: %d", w.FogStateAt(0, fxc, fyc))
	}
	if w.FogStateAt(1, fxc, fyc) != FogHidden {
		t.Fatalf("player 1 already sees scout cell: %d", w.FogStateAt(1, fxc, fyc))
	}

	if !w.SetShareVision(scout, 1, true) {
		t.Fatal("SetShareVision failed")
	}
	w.RecomputeVisibility()
	t.Logf("FSV shared: p1=%d sharesWith1=%v (want visible/true)", w.FogStateAt(1, fxc, fyc), w.SharesVisionWith(scout, 1))
	if w.FogStateAt(1, fxc, fyc) != FogVisible || !w.SharesVisionWith(scout, 1) {
		t.Fatalf("share vision not applied: state=%d shares=%v", w.FogStateAt(1, fxc, fyc), w.SharesVisionWith(scout, 1))
	}

	// revoke → cell was visible, demotes to explored.
	w.SetShareVision(scout, 1, false)
	w.RecomputeVisibility()
	t.Logf("FSV revoked: p1=%d (want %d explored) shares=%v", w.FogStateAt(1, fxc, fyc), FogExplored, w.SharesVisionWith(scout, 1))
	if w.FogStateAt(1, fxc, fyc) != FogExplored || w.SharesVisionWith(scout, 1) {
		t.Fatalf("revoke failed: state=%d shares=%v", w.FogStateAt(1, fxc, fyc), w.SharesVisionWith(scout, 1))
	}
}

// TestFogSaveRoundTripFSV — modifiers, toggles, and shared vision survive
// save(v26)→load and the full-World hash matches.
func TestFogSaveRoundTripFSV(t *testing.T) {
	w := visWorld(t)
	scout := spawnVisUnit(t, w, 0, 0, 0, 60, 60, PathGround)
	w.SetShareVision(scout, 2, true)
	c := fogCellWorld(40, 40)
	idKeep, _ := w.CreateFogModifierRect(0, FogVisible, c.X, c.Y, c.X, c.Y, false, true)
	idGone, _ := w.CreateFogModifierRadius(1, FogExplored, c.X, c.Y, fixed.FromInt(64), true, false)
	w.DestroyFogModifier(idGone) // exercise the free list
	w.SetFogMaskEnabled(false)
	w.RecomputeVisibility()

	reg := NewHashRegistry()
	var before statehash.Snapshot
	w.HashState(reg, &before)

	var buf bytes.Buffer
	const fp = 0x243243
	if err := w.SaveState(&buf, fp); err != nil {
		t.Fatalf("save: %v", err)
	}
	w2 := visWorld(t)
	// recreate the same scout so the reloaded share-vision row references a
	// live entity index (load restores rows by index).
	spawnVisUnit(t, w2, 0, 0, 0, 60, 60, PathGround)
	if err := w2.LoadState(bytes.NewReader(buf.Bytes()), fp); err != nil {
		t.Fatalf("load: %v", err)
	}
	var after statehash.Snapshot
	w2.HashState(reg, &after)

	t.Logf("FSV reload: keepValid=%v goneValid=%v maskEnabled=%v shares2=%v",
		w2.FogModifierValid(idKeep), w2.FogModifierValid(idGone), w2.FogMaskEnabled(), w2.SharesVisionWith(scout, 2))
	t.Logf("FSV hash: orig=%016x reload=%016x", before.Top, after.Top)
	if !w2.FogModifierValid(idKeep) || w2.FogModifierValid(idGone) {
		t.Fatalf("modifier validity not restored: keep=%v gone=%v", w2.FogModifierValid(idKeep), w2.FogModifierValid(idGone))
	}
	if w2.FogMaskEnabled() {
		t.Fatal("mask toggle not restored")
	}
	if !w2.SharesVisionWith(scout, 2) {
		t.Fatal("shared vision not restored")
	}
	if before.Top != after.Top {
		t.Fatalf("post-load hash mismatch: %016x vs %016x", before.Top, after.Top)
	}
}

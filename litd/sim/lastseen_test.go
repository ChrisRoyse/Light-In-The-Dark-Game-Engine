package sim

import "testing"

// Real-building end-to-end FSV for the last-seen accessors (#163) and the
// ghost lifecycle the render layer consumes. Building creation is sim-internal
// (Build.add), so the authoritative real-data test for the store + the new
// LastSeenCount/LastSeenAt/FogCellOf accessors lives here; the render consumer
// (render.GhostSet) is verified separately against the identical contract.
//
// "wouldGhost" mirrors exactly what render.GhostSet.Rebuild decides — a record
// is drawn when its cell is not currently visible — so the assertions prove the
// contract the render layer relies on holds against the live sim store.

func wouldGhost(w *World, player uint8) int {
	n := w.LastSeenCount(player)
	ghosts := 0
	for i := 0; i < n; i++ {
		_, _, pos, ok := w.LastSeenAt(player, i)
		if !ok {
			continue
		}
		fx, fy, ok := w.FogCellOf(pos)
		if !ok {
			continue
		}
		if w.FogStateAt(player, fx, fy) != FogVisible {
			ghosts++
		}
	}
	return ghosts
}

func TestLastSeenGhostLifecycleFSV(t *testing.T) {
	w := visWorld(t)
	const viewer uint8 = 0 // player who scouts
	const enemy uint8 = 1  // building owner

	// Enemy tower at (50,50); scout (player 0) nearby at (50,52).
	tower := spawnVisBuilding(t, w, enemy, enemy, 2, 50, 50)
	scout := spawnVisUnit(t, w, viewer, viewer, 0, 50, 52, PathGround)
	towerFx, towerFy := fogOf(50, 50)

	// PHASE 1 — scout sees the tower. Record created; cell VISIBLE → no ghost.
	w.RecomputeVisibility()
	count1 := w.LastSeenCount(viewer)
	tID, owner, pos, ok := w.LastSeenAt(viewer, 0)
	fog1 := w.FogStateAt(viewer, towerFx, towerFy)
	g1 := wouldGhost(w, viewer)
	t.Logf("FSV phase1 SEEN: count=%d rec={type=%d owner=%d pos=(%d,%d) ok=%v} fog=%d wouldGhost=%d",
		count1, tID, owner, pos.X.Floor(), pos.Y.Floor(), ok, fog1, g1)
	if count1 != 1 || !ok || tID != 2 || owner != enemy {
		t.Fatalf("phase1: expected 1 record type=2 owner=%d, got count=%d type=%d owner=%d ok=%v", enemy, count1, tID, owner, ok)
	}
	if pos.X.Floor() != w.Transforms.Pos[w.Transforms.Row(tower)].X.Floor() {
		t.Fatalf("phase1: last-seen pos X=%d != tower X=%d", pos.X.Floor(), w.Transforms.Pos[w.Transforms.Row(tower)].X.Floor())
	}
	if fog1 != FogVisible {
		t.Fatalf("phase1: tower cell should be visible, fog=%d", fog1)
	}
	if g1 != 0 {
		t.Fatalf("phase1: visible cell must NOT ghost, wouldGhost=%d", g1)
	}

	// PHASE 2 — scout retreats. Cell EXPLORED; record persists → GHOST.
	visMove(t, w, scout, 200, 200)
	w.RecomputeVisibility()
	count2 := w.LastSeenCount(viewer)
	fog2 := w.FogStateAt(viewer, towerFx, towerFy)
	g2 := wouldGhost(w, viewer)
	t.Logf("FSV phase2 RETREAT: count=%d fog=%d wouldGhost=%d", count2, fog2, g2)
	if count2 != 1 || fog2 != FogExplored || g2 != 1 {
		t.Fatalf("phase2: expected explored ghost (count=1 fog=%d ghost=1), got count=%d fog=%d ghost=%d", FogExplored, count2, fog2, g2)
	}

	// PHASE 3 — tower destroyed while out of sight. Cell still EXPLORED, so the
	// ghost PERSISTS (WC3 parity: you don't learn it died until you re-scout).
	if !w.Build.Remove(tower) {
		t.Fatal("phase3: Build.Remove failed")
	}
	if !w.DestroyUnit(tower) {
		t.Fatal("phase3: DestroyUnit failed")
	}
	w.RecomputeVisibility()
	count3 := w.LastSeenCount(viewer)
	fog3 := w.FogStateAt(viewer, towerFx, towerFy)
	g3 := wouldGhost(w, viewer)
	t.Logf("FSV phase3 DESTROYED-UNDER-FOG: count=%d fog=%d wouldGhost=%d (tower alive=%v)", count3, fog3, g3, w.Ents.Alive(tower))
	if count3 != 1 || fog3 != FogExplored || g3 != 1 {
		t.Fatalf("phase3: ghost must persist after death under fog, got count=%d fog=%d ghost=%d", count3, fog3, g3)
	}

	// PHASE 4 — scout returns. Cell VISIBLE, building gone → record cleared,
	// no ghost.
	visMove(t, w, scout, 50, 52)
	w.RecomputeVisibility()
	count4 := w.LastSeenCount(viewer)
	fog4 := w.FogStateAt(viewer, towerFx, towerFy)
	g4 := wouldGhost(w, viewer)
	t.Logf("FSV phase4 RE-SCOUT: count=%d fog=%d wouldGhost=%d", count4, fog4, g4)
	if count4 != 0 || fog4 != FogVisible || g4 != 0 {
		t.Fatalf("phase4: re-scout must clear ghost, got count=%d fog=%d ghost=%d", count4, fog4, g4)
	}
}

// TestLastSeenAccessorBoundsFSV — accessor edge cases: out-of-range index,
// out-of-range player, and FogCellOf agreement with the internal mapping.
func TestLastSeenAccessorBoundsFSV(t *testing.T) {
	w := visWorld(t)
	spawnVisBuilding(t, w, 1, 1, 2, 60, 60)
	spawnVisUnit(t, w, 0, 0, 0, 60, 62, PathGround)
	w.RecomputeVisibility()

	if c := w.LastSeenCount(0); c != 1 {
		t.Fatalf("expected 1 record, got %d", c)
	}
	// Out-of-range index.
	if _, _, _, ok := w.LastSeenAt(0, 1); ok {
		t.Fatal("index past count must return ok=false")
	}
	if _, _, _, ok := w.LastSeenAt(0, -1); ok {
		t.Fatal("negative index must return ok=false")
	}
	// Out-of-range player.
	if c := w.LastSeenCount(MaxPlayers); c != 0 {
		t.Fatalf("out-of-range player count=%d want 0", c)
	}
	if _, _, _, ok := w.LastSeenAt(MaxPlayers, 0); ok {
		t.Fatal("out-of-range player must return ok=false")
	}
	// FogCellOf agrees with the internal worldToFogCell for the record pos.
	_, _, pos, _ := w.LastSeenAt(0, 0)
	ax, ay, aok := w.FogCellOf(pos)
	bx, by, bok := worldToFogCell(pos)
	t.Logf("FSV FogCellOf=(%d,%d,%v) worldToFogCell=(%d,%d,%v)", ax, ay, aok, bx, by, bok)
	if ax != bx || ay != by || aok != bok {
		t.Fatalf("FogCellOf disagrees with worldToFogCell")
	}
}

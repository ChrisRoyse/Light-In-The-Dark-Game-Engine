package sim

// Movement authority + terrain collision (#588). A mover with the
// MoverAuthority flag OWNS its Target unit's transform: normal pathing /
// movement integration is suspended for that unit so the mover alone
// drives it (knockback, charge, forced march). The hold set is rebuilt
// each tick from the live movers (derived state — not hashed/saved) in a
// pre-pass at the top of phaseMovement, before movementSystem runs.
//
// Terrain: a NON-flying mover may not push its Target into impassable
// terrain. After a step, if the new cell is unwalkable the move is blocked
// (clamped to the pre-step position). A MoverFlying mover ignores ground
// terrain entirely. Both are no-ops on a world with no baked terrain grid.

import "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"

// refreshMoverAuthority rebuilds the per-entity authority-hold marks from
// the live MoverAuthority movers. O(movers). Called before movementSystem.
func (w *World) refreshMoverAuthority() {
	// clear last tick's marks (only the few that were set)
	for _, id := range w.moverAuthList {
		if idx := int(id.Index()); idx < len(w.moverAuthHold) {
			w.moverAuthHold[idx] = false
		}
	}
	w.moverAuthList = w.moverAuthList[:0]

	ms := w.Movers
	if ms == nil {
		return
	}
	for r := int32(1); r < int32(len(ms.live)); r++ {
		if !ms.live[r] || ms.Flags[r]&MoverAuthority == 0 {
			continue
		}
		id := ms.Target[r]
		idx := int(id.Index())
		if idx <= 0 || idx >= len(w.moverAuthHold) || w.moverAuthHold[idx] {
			continue
		}
		w.moverAuthHold[idx] = true
		w.moverAuthList = append(w.moverAuthList, id)
	}
}

// moverAuthHeld reports whether a unit's transform is currently owned by a
// MoverAuthority mover (so its own movement integration must stand down).
func (w *World) moverAuthHeld(id EntityID) bool {
	idx := int(id.Index())
	return idx > 0 && idx < len(w.moverAuthHold) && w.moverAuthHold[idx]
}

// posWalkable reports whether pos sits on walkable ground. Always true when
// no terrain grid is baked (the mover world is open).
func (w *World) posWalkable(pos fixed.Vec2) bool {
	if !w.Grid.Ready() {
		return true
	}
	c := cellOfPos(pos)
	return c >= 0 && w.cellStaticWalkable(c)
}

// moverTerrainBlocks reports whether a non-flying mover's move from->to is
// blocked by terrain (to is unwalkable). Flying movers never block.
func (w *World) moverTerrainBlocks(r int32, to fixed.Vec2) bool {
	if w.Movers.Flags[r]&MoverFlying != 0 {
		return false
	}
	return !w.posWalkable(to)
}

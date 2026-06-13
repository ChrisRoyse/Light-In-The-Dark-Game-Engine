package sim

// changeowner.go — the ownership-migration primitive (#362). Reassigning a
// unit's owner is NOT a raw Owners store write: ownership is the key for derived
// per-player state that must move with the unit, or the sim silently desyncs:
//
//   - the food/supply ledger (foodUsed/foodCap): the unit's consumed and
//     provided food must leave the old player and join the new one, exactly as
//     DestroyUnit settles it on death (world.go). A store poke would leave the
//     old owner permanently charged and the new owner uncharged.
//   - visibility: the per-player fog contribution is owner-scoped
//     (visibility.go), so the owner change must refresh the vision cycle.
//
// Team is taken explicitly so the alliance model (#218) can drive it; today the
// API passes the new player's own slot (FFA default, #361).

// ChangeOwner reassigns unit id to newPlayer on newTeam, migrating the derived
// per-player state (food ledger, visibility) that ownership keys. When
// changeColor is true the unit's team color follows the new player's slot;
// otherwise the existing color is kept (WC3 SetUnitOwner changeColor semantics).
// Returns false (no change) on a dead/invalid unit or one with no owner row.
func (w *World) ChangeOwner(id EntityID, newPlayer, newTeam uint8, changeColor bool) bool {
	if !w.Ents.Alive(id) {
		return false
	}
	or := w.Owners.Row(id)
	if or == -1 {
		return false
	}
	oldPlayer := w.Owners.Player[or]

	// Migrate the food ledger: the old owner sheds this unit's consumed and
	// provided food; the new owner takes it on. Mirrors the settle in
	// DestroyUnit so the two paths can never drift.
	if er := w.Econs.Row(id); er != -1 {
		cost := int32(w.Econs.FoodCost[er])
		provided := int32(w.Econs.FoodProvided[er])
		if oldPlayer < MaxPlayers {
			w.foodUsed[oldPlayer] -= cost
			w.foodCap[oldPlayer] -= provided
		}
		if newPlayer < MaxPlayers {
			w.foodUsed[newPlayer] += cost
			w.foodCap[newPlayer] += provided
		}
	}

	w.Owners.Player[or] = newPlayer
	w.Owners.Team[or] = newTeam
	if changeColor {
		w.Owners.Color[or] = newPlayer
	}

	// Vision is owner-scoped: the unit now contributes to the new owner's fog
	// and stops contributing to the old owner's. Refresh the vision cycle.
	w.RecomputeVisibility()
	return true
}

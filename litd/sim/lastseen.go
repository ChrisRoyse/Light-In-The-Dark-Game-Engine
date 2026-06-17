package sim

import "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"

// Read-only accessors over the per-player last-seen building store for the
// render layer (#163). The store itself lives in visibility.go — it derives
// purely from the visibility grid, is preallocated and fixed-size, and is part
// of the state hash (VisibilityGrid.HashInto), so it stays lockstep-safe. This
// file exposes just enough to enumerate ghost records and map a world position
// to its fog cell, without leaking the internal layout or letting render
// mutate sim state.

// LastSeenCount returns how many last-seen building records player currently
// holds (the live prefix). Zero if visibility is disabled or player is out of
// range.
func (w *World) LastSeenCount(player uint8) int {
	if w.Visibility == nil || player >= MaxPlayers {
		return 0
	}
	return int(w.Visibility.lastSeenCount[player])
}

// LastSeenAt returns the i-th last-seen building record for player: the type,
// the owner at last sighting, and the position where it was last seen. ok is
// false for an out-of-range index or disabled visibility. Records in the live
// prefix are always in use.
func (w *World) LastSeenAt(player uint8, i int) (typeID uint16, owner uint8, pos fixed.Vec2, ok bool) {
	if w.Visibility == nil || player >= MaxPlayers {
		return 0, 0, fixed.Vec2{}, false
	}
	v := w.Visibility
	if i < 0 || i >= int(v.lastSeenCount[player]) {
		return 0, 0, fixed.Vec2{}, false
	}
	rec := &v.lastSeen[int(player)*v.LastSeenCap()+i]
	if !rec.Used {
		return 0, 0, fixed.Vec2{}, false
	}
	return rec.TypeID, rec.Owner, rec.Pos, true
}

// FogCellOf maps a world position to its fog-grid cell, the same mapping the
// visibility system uses internally. ok is false when the position is outside
// the playable bounds. Render uses this to look up the current fog state of a
// ghost record's position.
func (w *World) FogCellOf(pos fixed.Vec2) (x, y int32, ok bool) {
	return worldToFogCell(pos)
}

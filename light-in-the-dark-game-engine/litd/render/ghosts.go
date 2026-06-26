package render

import "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"

// Ghost-building rendering (#163; fog-of-war-minimap-selection.md §2.3, §9.2).
//
// WC3-parity: once a player has seen an enemy building, a "ghost" of it stays
// painted under explored fog at its last-seen position even after the real
// building is destroyed — the ghost only updates or vanishes when the cell is
// re-scouted. The sim owns the truth (a per-player last-seen store derived from
// the visibility grid, part of the state hash); this file is a pure read-only
// consumer that turns those records into a draw list.
//
// The ghost-vs-live decision is render's: a record is drawn as a ghost only
// while its cell is NOT currently visible to the viewer. When the cell is
// visible the live entity is drawn instead (and the sim refreshes or clears the
// record), so the ghost and the real building are never drawn at once.

// GhostSource is the read-only sim contract: enumerate a player's last-seen
// building records and map a world position to its fog cell. *sim.World
// satisfies it; tests pass a synthetic source. The verdict is always the
// produced ghost list checked against the sim store.
type GhostSource interface {
	LastSeenCount(player uint8) int
	LastSeenAt(player uint8, i int) (typeID uint16, owner uint8, pos fixed.Vec2, ok bool)
	FogCellOf(pos fixed.Vec2) (x, y int32, ok bool)
}

// Ghost is one building snapshot to draw under explored fog: the type and
// owner at last sighting and the position it was last seen at. It carries no
// live entity reference — by construction the live entity is not being drawn.
type Ghost struct {
	TypeID uint16
	Owner  uint8
	Pos    fixed.Vec2
}

// GhostSet builds and holds the current ghost draw list. The backing slice is
// reused across rebuilds, so steady-state Rebuild allocates nothing (R-GC).
type GhostSet struct {
	ghosts []Ghost
}

// Rebuild refreshes the ghost list for player: every last-seen building whose
// cell is currently explored-but-not-visible (to player, per fog) becomes a
// ghost. A record whose cell is currently visible is skipped — the live entity
// is drawn there instead. A record whose position falls outside the playable
// bounds is skipped. Returns the list (valid until the next Rebuild).
func (gs *GhostSet) Rebuild(src GhostSource, fog FogGridSource, player uint8) []Ghost {
	gs.ghosts = gs.ghosts[:0]
	n := src.LastSeenCount(player)
	for i := 0; i < n; i++ {
		typeID, owner, pos, ok := src.LastSeenAt(player, i)
		if !ok {
			continue
		}
		fx, fy, ok := src.FogCellOf(pos)
		if !ok {
			continue
		}
		if fog.FogStateAt(player, fx, fy) == fogStateVisible {
			continue // re-scouted: the live building is drawn, not its ghost
		}
		gs.ghosts = append(gs.ghosts, Ghost{TypeID: typeID, Owner: owner, Pos: pos})
	}
	return gs.ghosts
}

// Ghosts returns the current ghost list from the last Rebuild.
func (gs *GhostSet) Ghosts() []Ghost { return gs.ghosts }

// Reserve presizes the backing slice so the first Rebuild of up to n ghosts
// does not allocate.
func (gs *GhostSet) Reserve(n int) {
	if cap(gs.ghosts) < n {
		gs.ghosts = make([]Ghost, 0, n)
	}
}

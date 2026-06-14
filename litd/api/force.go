package litd

// Forces & player enumeration (#218; players-and-forces.md). In JASS a
// `force` is a set of players walked by a `ForForce` callback over
// `GetEnumPlayer`. Canonical Go replaces that callback enumeration with
// plain `[]Player` slices (R-EXEC-4): `Game.Players(filter)` returns the
// matching players, and the caller ranges over them — no enum-state
// globals, no per-element closure dispatch.
//
// The `Force` noun survives as a thin, mutable player-set convenience
// (public-api-design.md §2 row 3) for scripts that want a named, reusable
// group. It is script-transient state (like triggers/timers), not part
// of the hashed/serialized sim — alliances, the real per-player relation,
// live on Player (player.go).

import "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"

// PlayerFilter selects players for enumeration. A nil filter matches all.
type PlayerFilter func(Player) bool

// Players returns the player slots matching filter, in ascending slot
// order. A nil filter returns every slot. Replaces the JASS
// force-enumeration zoo (ForForce/CountPlayersInForceBJ/…). Returns nil
// on a nil game.
func (g *Game) Players(filter PlayerFilter) []Player {
	if g == nil || g.w == nil {
		return nil
	}
	var out []Player
	for s := 0; s < sim.MaxPlayers; s++ {
		p := Player{idx: int32(s), g: g}
		if filter == nil || filter(p) {
			out = append(out, p)
		}
	}
	return out
}

// AllPlayers returns every player slot. JASS: GetPlayersAll convenience.
func (g *Game) AllPlayers() []Player { return g.Players(nil) }

// Allies returns the players p is passive toward (its allies). D4 helper
// (GetPlayersAllies). Excludes p itself.
func (g *Game) Allies(p Player) []Player {
	return g.Players(func(o Player) bool { return o.idx != p.idx && p.IsAlly(o) })
}

// Enemies returns the players p is at war with. D4 helper
// (GetPlayersEnemies). Excludes p itself.
func (g *Game) Enemies(p Player) []Player {
	return g.Players(func(o Player) bool { return o.idx != p.idx && p.IsEnemy(o) })
}

// ---- the Force noun (mutable player set) ----

// CreateForce makes a new, empty force. JASS: CreateForce / CreateForceBJ.
// Zero-value Force on a nil game.
func (g *Game) CreateForce() Force {
	if g == nil || g.w == nil {
		return Force{}
	}
	g.forces = append(g.forces, 0)
	return Force{id: uint32(len(g.forces)), g: g}
}

// mask returns a pointer to the force's player bitset, or nil if invalid.
func (f Force) mask() *uint32 {
	if !f.Valid() {
		return nil
	}
	return &f.g.forces[f.id-1]
}

// AddPlayer adds p to the force. No-op on an invalid force or a foreign/
// invalid player. JASS: ForceAddPlayer.
func (f Force) AddPlayer(p Player) {
	m := f.mask()
	if m == nil {
		f.g.reportInvalid("Force.AddPlayer")
		return
	}
	if p.g != f.g || !p.Valid() {
		return
	}
	*m |= 1 << uint(p.idx)
}

// RemovePlayer removes p from the force. No-op on an invalid force or
// player. JASS: ForceRemovePlayer.
func (f Force) RemovePlayer(p Player) {
	m := f.mask()
	if m == nil {
		f.g.reportInvalid("Force.RemovePlayer")
		return
	}
	if p.g != f.g || !p.Valid() {
		return
	}
	*m &^= 1 << uint(p.idx)
}

// Clear empties the force. JASS: ForceClear.
func (f Force) Clear() {
	if m := f.mask(); m != nil {
		*m = 0
	}
}

// AddAllPlayers adds every player slot to the force. JASS: ForceEnumPlayers
// with a null filter.
func (f Force) AddAllPlayers() {
	if m := f.mask(); m != nil {
		*m = (1 << uint(sim.MaxPlayers)) - 1
	}
}

// Contains reports whether p is in the force. JASS: IsPlayerInForce.
func (f Force) Contains(p Player) bool {
	m := f.mask()
	if m == nil || p.g != f.g || !p.Valid() {
		return false
	}
	return *m&(1<<uint(p.idx)) != 0
}

// Count returns the number of players in the force. JASS:
// CountPlayersInForceBJ.
func (f Force) Count() int {
	m := f.mask()
	if m == nil {
		return 0
	}
	n := 0
	for v := *m; v != 0; v &= v - 1 {
		n++
	}
	return n
}

// Players returns the force's members in ascending slot order. JASS:
// ForForce enumeration collapsed to a slice (R-EXEC-4).
func (f Force) Players() []Player {
	m := f.mask()
	if m == nil {
		return nil
	}
	var out []Player
	for s := 0; s < sim.MaxPlayers; s++ {
		if *m&(1<<uint(s)) != 0 {
			out = append(out, Player{idx: int32(s), g: f.g})
		}
	}
	return out
}

package helpers

import (
	litd "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
)

// CreateUnits spawns n units of type typ for owner at pos, each facing
// the given angle, and returns their handles in creation order. It is the
// D4 keep for the CreateNUnitsAtLoc family (deduplication-policy.md §5 row
// 3): the JASS BJ looped CreateUnit and accumulated a group; here the loop
// is the helper and the return is a plain slice (R-EXEC-4 — no hidden
// group, no GetEnumUnit side channel).
//
// The returned slice is always non-nil. n <= 0 yields an empty (length 0)
// slice and spawns nothing. Individual spawns that fail (unit cap reached,
// null/unbound type, foreign owner) contribute the zero Unit at their
// position, exactly as a direct CreateUnit call would — the helper adds no
// error masking over the public verb (a caller can Valid()-check each).
func CreateUnits(g *litd.Game, n int, owner litd.Player, typ litd.UnitType, pos litd.Vec2, facing litd.Angle) []litd.Unit {
	if n < 0 {
		n = 0
	}
	out := make([]litd.Unit, 0, n)
	for i := 0; i < n; i++ {
		out = append(out, g.CreateUnit(owner, typ, pos, facing))
	}
	return out
}

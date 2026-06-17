package helpers

import (
	litd "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
)

// WeightedChoice picks an index in [0, len(weights)) with probability
// proportional to weights[i], drawing from the sim's seeded PRNG (R-SIM-2)
// via Game.RandomInt — never math/rand, whose global state would desync a
// match. It is the D4 keep for the RandomDistReset/AddItem/Choose machinery
// (deduplication-policy.md §5 row 6): the JASS distribution accumulator
// becomes one pure call over a weight slice.
//
// Defined behavior at the degenerate boundaries (deterministic, PRNG
// untouched, so two runs from the same seed always agree):
//   - empty slice           → -1 (no item to choose, matches JASS empty dist)
//   - no positive weight     → -1 (all weights <= 0: nothing is selectable)
// Negative or zero weights are treated as zero (never selected). A draw is
// taken only when the positive total is > 0.
func WeightedChoice(g *litd.Game, weights []int) int {
	total := 0
	for _, w := range weights {
		if w > 0 {
			total += w
		}
	}
	if total == 0 {
		return -1 // empty, or no positive weight — no selectable item
	}
	// RandomInt is inclusive on both bounds; draw r in [0, total-1].
	r := g.RandomInt(0, total-1)
	acc := 0
	for i, w := range weights {
		if w <= 0 {
			continue
		}
		acc += w
		if r < acc {
			return i
		}
	}
	// Unreachable: r < total == acc by construction. Return the last
	// positive-weight index defensively rather than a wrong 0.
	for i := len(weights) - 1; i >= 0; i-- {
		if weights[i] > 0 {
			return i
		}
	}
	return -1
}

// RandomItemType picks one code from codes uniformly at random (sim PRNG)
// and resolves it to its bound ItemType. An empty list, or a nil game,
// yields the null ItemType (IsZero). If the drawn code is unbound, the null
// ItemType is returned for it — the resolution failure is the public
// Game.ItemType behavior, not masked here. D4 keep for the
// ChooseRandomItem* / random-reward helpers.
func RandomItemType(g *litd.Game, codes []string) litd.ItemType {
	if g == nil || len(codes) == 0 {
		return litd.ItemType{}
	}
	i := g.RandomInt(0, len(codes)-1)
	return g.ItemType(codes[i])
}

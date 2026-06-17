package litd

import (
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/statehash"
)

// StateHash returns a deterministic 64-bit digest of the full authoritative sim
// state: two games hash equal iff their authoritative state is identical, and
// any state mutation changes the digest. It is a verification seam — the
// Go-vs-Lua binding conformance suite (#267) drives a verb through the Go api on
// one game and through the Lua bindings on a twin, then asserts equal StateHash;
// the determinism harness (#271) uses the same digest across runs.
//
// It builds a fresh hash registry per call (allocating), so it is NOT for the
// per-tick hot path — the sim retains its own registry there (R-GC-1). A nil or
// uninitialized game hashes to 0.
func (g *Game) StateHash() uint64 {
	if g == nil || g.w == nil {
		return 0
	}
	reg := sim.NewHashRegistry()
	return g.w.HashState(reg, &statehash.Snapshot{}).Top
}

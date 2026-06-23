package litd

import (
	"io"

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

// HashSnapshot returns the full state digest broken out by system: top is the
// same value as StateHash, and subs is the per-system sub-hash vector indexed by
// the canonical system order (HashSystemNames). It is the seam the desync FSV
// harness (#82) and the netplay desync detector (#77) use to bisect a divergence
// to a named system. Like StateHash it allocates a fresh registry — not for the
// hot path. A nil/uninitialized game returns (0, nil).
func (g *Game) HashSnapshot() (top uint64, subs []uint64) {
	if g == nil || g.w == nil {
		return 0, nil
	}
	reg := sim.NewHashRegistry()
	var snap statehash.Snapshot
	g.w.HashState(reg, &snap)
	return snap.Top, append([]uint64(nil), snap.Subs...)
}

// HashSystemNames returns the canonical state-hash system vocabulary in sub-hash
// index order: subs[i] from HashSnapshot is the digest of system HashSystemNames()[i].
func HashSystemNames() []string {
	return append([]string(nil), sim.HashSystems...)
}

// DumpState writes the full authoritative sim state as JSON (R-FSV-2) to wr —
// the public-boundary accessor for sim.World.DumpState. It is the Source-of-Truth
// read an FSV agent inspects after a trigger: the dump path allocates freely (it
// is off the steady-state R-GC-1 gate) but NEVER mutates the world, so hashing
// before and after a dump is bit-identical. A nil/uninitialized game writes the
// empty-world dump. This is the structured-text SoT that lets FSV avoid the far
// costlier screenshot read whenever the question is non-pixel (#516).
func (g *Game) DumpState(wr io.Writer) error {
	if g == nil || g.w == nil {
		return nil
	}
	return g.w.DumpState(wr)
}

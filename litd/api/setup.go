package litd

import (
	"fmt"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
)

// Public game setup path (#386). NewGame is the only exported way to obtain a
// *Game from outside this package — the Lua binding layer (#267), the world
// loader (#268), and cmd entry points all build a game through it. The
// unexported newGame(w) remains the internal/test seam over an existing world.
//
// NewGame builds the deterministic sim core only: a headless game with no
// presentation sinks (every render/audio verb is a deterministic no-op until a
// driver attaches). Map/world population is a separate setup verb (LoadWorld,
// #268) — NewGame never reads the filesystem, so it cannot fail on missing
// assets and stays a pure, deterministic constructor. No sim type appears in
// the signature (R-API-6): capacities cross as plain int knobs.

// GameOptions configures NewGame. The zero value is valid: it builds a headless
// game with engine-default capacities and seed 0.
type GameOptions struct {
	// MaxUnits, MaxProjectiles, MaxEffects, MaxDestructables cap the entity
	// pools. Zero means "engine default" (the sim resolves unset caps). These
	// are plain ints so no sim.Caps type leaks across the public boundary.
	MaxUnits         int
	MaxProjectiles   int
	MaxEffects       int
	MaxDestructables int

	// Seed is the deterministic PRNG seed (R-SIM-2). The same seed and command
	// stream reproduce a run bit-for-bit; a different seed diverges.
	Seed int64
}

// NewGame builds a headless, seeded game from opts. It fails closed on an
// invalid capacity (negative) rather than silently clamping. The returned game
// has no map loaded — call LoadWorld (#268) to populate it.
func NewGame(opts GameOptions) (*Game, error) {
	for _, c := range []struct {
		name string
		val  int
	}{
		{"MaxUnits", opts.MaxUnits},
		{"MaxProjectiles", opts.MaxProjectiles},
		{"MaxEffects", opts.MaxEffects},
		{"MaxDestructables", opts.MaxDestructables},
	} {
		if c.val < 0 {
			return nil, fmt.Errorf("api: NewGame: %s = %d, must be >= 0", c.name, c.val)
		}
	}
	w := sim.NewWorld(sim.Caps{
		Units:         opts.MaxUnits,
		Projectiles:   opts.MaxProjectiles,
		Effects:       opts.MaxEffects,
		Destructables: opts.MaxDestructables,
	})
	w.SetSeed(uint64(opts.Seed))
	return newGame(w), nil
}

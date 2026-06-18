package litd

import (
	"fmt"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
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
	// MaxUnits caps the unit entity pool. Zero means the engine default (unset
	// caps are resolved internally). A plain int so no internal cap type leaks
	// across the public boundary.
	MaxUnits int
	// MaxProjectiles caps the in-flight missile/projectile pool; zero = default.
	MaxProjectiles int
	// MaxEffects caps the transient visual-effect pool; zero = default.
	MaxEffects int
	// MaxDestructables caps the destructable-object pool; zero = default.
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

// DefineUnits installs the unit-type definitions this game can spawn — the
// public path to seed unit data from outside the api package (#387). A setup
// verb (R-API-5): it returns an error and fails closed on an empty or oversized
// table, or a conflicting rebind, rather than silently ignoring the data. The
// world loader (#268) calls this with a world's parsed unit table; it can also
// seed a programmatic game directly. UnitType(code) resolves against these defs.
func (g *Game) DefineUnits(defs []data.Unit) error {
	if g == nil || g.w == nil {
		return fmt.Errorf("api: DefineUnits: nil game")
	}
	if !g.w.BindUnitDefs(defs) {
		return fmt.Errorf("api: DefineUnits: rejected %d definitions (empty, exceeds the 65536 type-id space, or conflicts with an existing binding)", len(defs))
	}
	return nil
}

// DefineEconomy initialises the economy with resourceTypes per-player resource
// counters (#396) — e.g. 2 for the gold/lumber pair. A setup verb (R-API-5): it
// returns an error and fails closed on a non-positive or over-ceiling count, or
// a rebind to a different count. Until it is called the per-player resource
// ledger is unallocated, so Player.SetGold/SetLumber/SetResource are no-ops and
// Gold/Lumber read zero; after it they read and write.
func (g *Game) DefineEconomy(resourceTypes int) error {
	if g == nil || g.w == nil {
		return fmt.Errorf("api: DefineEconomy: nil game")
	}
	if !g.w.BindEconomy(resourceTypes) {
		return fmt.Errorf("api: DefineEconomy: rejected %d resource types (must be 1..%d and not conflict with an existing binding)", resourceTypes, data.MaxResourceTypes)
	}
	return nil
}

// DefineHeroes installs the hero rule set — XP curve, per-unit bounty table,
// hero definitions with skill trees, attribute coefficients, and revive costs
// (#396). A setup verb (R-API-5): it returns an error and fails closed when
// units are not yet defined, the XP curve is shorter than two levels, the bounty
// table length does not match the unit table, a hero references an unknown unit
// or skill ability, revive costs do not match the resource count, or heroes are
// already bound. Call DefineUnits (and DefineAbilities for any hero skills, and
// DefineEconomy for any revive costs) first.
func (g *Game) DefineHeroes(h *data.HeroTables) error {
	if g == nil || g.w == nil {
		return fmt.Errorf("api: DefineHeroes: nil game")
	}
	if !g.w.BindHeroes(h) {
		return fmt.Errorf("api: DefineHeroes: rejected hero rule set (nil, units not defined, XP curve shorter than two levels, bounty length mismatch, unknown hero unit or skill ability, revive-cost/resource-count mismatch, or already bound)")
	}
	return nil
}

// DefineResourceNodes installs the resource-node type table this game can spawn
// (#401) — the install seam that lets a world ship gold mines / harvestable
// nodes. A setup verb (R-API-5): it fails closed on an empty or oversized table
// or a length-mismatch rebind. ResourceNodeType(code) resolves against these
// defs and Game.CreateResourceNode spawns from them. Spawning also needs
// DefineEconomy bound (a node's Resource index is checked against the resource
// count), so call DefineEconomy first for any world that ships nodes.
func (g *Game) DefineResourceNodes(nodes []data.ResourceNodeType) error {
	if g == nil || g.w == nil {
		return fmt.Errorf("api: DefineResourceNodes: nil game")
	}
	if !g.w.BindResourceNodeDefs(nodes) {
		return fmt.Errorf("api: DefineResourceNodes: rejected %d node type(s) (empty, exceeds the 65536 type-id space, or conflicts with an existing binding)", len(nodes))
	}
	return nil
}

// DefineEffects installs the compiled effect-composition arena that abilities
// and items reference by index (#394). A setup verb (R-API-5): it fails closed
// — propagating the validation error — when an entry names an unknown effect
// primitive or one with no registered executor, rather than binding a malformed
// arena that would fault mid-match. Call this before DefineAbilities/DefineItems
// for any world whose abilities or items carry an effect composition, since
// those tables validate their effect ranges against this arena.
func (g *Game) DefineEffects(arena []data.CompiledEffect) error {
	if g == nil || g.w == nil {
		return fmt.Errorf("api: DefineEffects: nil game")
	}
	if err := g.w.BindEffects(arena); err != nil {
		return fmt.Errorf("api: DefineEffects: %w", err)
	}
	return nil
}

// DefineAbilities installs the ability definitions this game can grant and cast
// (#394) — the bulk companion to RegisterAbility, seeding a whole table at once.
// A setup verb (R-API-5): it returns an error and fails closed when the table is
// already bound or exceeds the ability-definition ceiling, rather than silently
// dropping the data. AbilityRef lookups resolve against these defs.
func (g *Game) DefineAbilities(defs []data.Ability) error {
	if g == nil || g.w == nil {
		return fmt.Errorf("api: DefineAbilities: nil game")
	}
	if !g.w.BindAbilityDefs(defs) {
		return fmt.Errorf("api: DefineAbilities: rejected %d definitions (already bound, or exceeds the ability-definition ceiling)", len(defs))
	}
	return nil
}

// DefineItems installs the item-type definitions this game can spawn and carry
// (#394). A setup verb (R-API-5): it returns an error and fails closed on an
// empty or oversized table, a conflicting rebind, or an item whose effect
// composition points outside the bound effect arena — call DefineEffects first
// when items carry effects.
func (g *Game) DefineItems(defs []data.Item) error {
	if g == nil || g.w == nil {
		return fmt.Errorf("api: DefineItems: nil game")
	}
	if !g.w.BindItemDefs(defs) {
		return fmt.Errorf("api: DefineItems: rejected %d definitions (empty, exceeds the 65536 type-id space, conflicts with an existing binding, or references the effect arena out of range)", len(defs))
	}
	return nil
}

// DefineBuffTypes installs the buff/debuff type definitions this game can apply
// (#394). A setup verb (R-API-5): it returns an error and fails closed when the
// table exceeds the 65536 type-id space. Buff codes resolve against these types.
func (g *Game) DefineBuffTypes(types []data.BuffType) error {
	if g == nil || g.w == nil {
		return fmt.Errorf("api: DefineBuffTypes: nil game")
	}
	if !g.w.BindBuffTypes(types) {
		return fmt.Errorf("api: DefineBuffTypes: rejected %d definitions (exceeds the 65536 type-id space)", len(types))
	}
	return nil
}

// DefineUpgrades installs the tech-upgrade definitions and their admission
// requirements (#394). A setup verb (R-API-5): it returns an error and fails
// closed on an empty or oversized table, a conflicting rebind, or a requirement
// or AppliesTo index that points outside the known unit/upgrade tables. Call
// DefineUnits first — upgrade definitions validate their target units against
// the bound unit table.
func (g *Game) DefineUpgrades(upgrades []data.Upgrade, requires []data.Require) error {
	if g == nil || g.w == nil {
		return fmt.Errorf("api: DefineUpgrades: nil game")
	}
	if !g.w.BindTech(upgrades, requires) {
		return fmt.Errorf("api: DefineUpgrades: rejected %d upgrades / %d requirements (empty, oversized, units not yet defined, or an out-of-range AppliesTo/requirement target)", len(upgrades), len(requires))
	}
	return nil
}

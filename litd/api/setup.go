package litd

import (
	"fmt"
	"log"

	mapdata "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/asset/mapdata"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
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

	// Map is the loaded skirmish map (#410), or nil for a mapless programmatic
	// game. The CALLER loads + validates it (mapdata.Load) and passes it in, so
	// NewGame still reads no filesystem. When set, NewGame seeds each player's
	// start location from the map; MapStarts/MapBeacons expose the placements,
	// and the script-binding layer surfaces them to Lua worlds.
	Map *mapdata.Map
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
	g := newGame(w)
	if opts.Map != nil {
		g.mapData = opts.Map
		// Seed start locations: the map's per-player start cell becomes both the
		// indexed start-location table (StartLocation(i)) and the player's sim
		// start point (Player.StartLocation). Cells convert to world centers
		// (sim.CellCenter convention).
		for _, s := range opts.Map.Starts() {
			if int(s.Player) >= sim.MaxPlayers {
				continue
			}
			g.match.startLocations[s.Player] = mapCellCenter(s.X, s.Y)
			g.w.SetPlayerStart(s.Player, fixed.FromInt(int32(s.X*32+16)), fixed.FromInt(int32(s.Y*32+16)))
		}
	}
	return g, nil
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

// logSetup emits a one-line observability trace for an install-seam verb when
// debug mode is on (Game.SetDebug). Setup verbs are infrequent, so this is a
// plain structured log — it gives an operator a record of exactly which tables
// and dims a world bound, which is the first thing to check when combat
// mis-resolves. Routed through the OnInvalidHandle sink if one is installed so
// a host can capture setup traces alongside its other diagnostics.
func (g *Game) logSetup(format string, args ...any) {
	if g == nil || !g.debug {
		return
	}
	msg := "litd/setup: " + fmt.Sprintf(format, args...)
	if g.onInvalid != nil {
		g.onInvalid(msg)
		return
	}
	log.Println(msg)
}

// DefineCombat installs the damage-coefficient matrix — coeff[attackType]
// [armorType] in thousandths (1000 = 100%) — that combat resolution multiplies
// each hit by (#406). The install-seam companion to DefineUnits for combat
// constants; a world that ships custom attack/armor types seeds them here, and
// until it is bound every queued hit is dropped (counted, never guessed —
// damage.go), so no unit can take damage. A setup verb (R-API-5): it fails
// closed on an empty or ragged matrix. The api takes [][]int so no sim integer
// width leaks across the boundary.
func (g *Game) DefineCombat(matrix [][]int) error {
	if g == nil || g.w == nil {
		return fmt.Errorf("api: DefineCombat: nil game")
	}
	coeff := make([][]int32, len(matrix))
	for i, row := range matrix {
		coeff[i] = make([]int32, len(row))
		for j, v := range row {
			coeff[i][j] = int32(v)
		}
	}
	if err := g.w.BindDamageMatrix(coeff); err != nil {
		return fmt.Errorf("api: DefineCombat: %w", err)
	}
	cols := 0
	if len(coeff) > 0 {
		cols = len(coeff[0])
	}
	g.logSetup("DefineCombat: bound %dx%d coefficient matrix", len(coeff), cols)
	return nil
}

// DefineDamageTypes declares the named attack- and armor-type tables that index
// the coefficient matrix (#472). Names are ordered: a name's position is its
// matrix row (attack) or column (armor). Call before DefineCombat — once the
// types are declared, DefineCombat validates the matrix dims equal
// len(attack)×len(armor), so a world that adds a new attack/armor type but
// forgets a matrix row/column fails loudly at setup instead of silently
// dropping packets. A setup verb (R-API-5): fails closed on an empty or
// duplicate-named table. The api takes []string so no sim width leaks across
// the boundary. Modders extend combat by adding a name + a matrix row — no
// engine edit (ADR #453).
func (g *Game) DefineDamageTypes(attack, armor []string) error {
	if g == nil || g.w == nil {
		return fmt.Errorf("api: DefineDamageTypes: nil game")
	}
	if err := g.w.BindDamageTypes(attack, armor); err != nil {
		return fmt.Errorf("api: DefineDamageTypes: %w", err)
	}
	g.logSetup("DefineDamageTypes: %d attack types %v, %d armor types %v",
		len(attack), attack, len(armor), armor)
	return nil
}

// AttackTypeID / ArmorTypeID resolve a declared type name to its matrix index
// (ok=false if unknown or the tables were never declared). The name→index seam
// combat conditions (C2/C4) build on so scripts reference types by name.
func (g *Game) AttackTypeID(name string) (int, bool) {
	if g == nil || g.w == nil {
		return 0, false
	}
	i, ok := g.w.AttackTypeIndex(name)
	return int(i), ok
}

// ArmorTypeID resolves a declared armor-type name to its matrix column index,
// reporting ok=false if the name is unknown or no tables were declared.
func (g *Game) ArmorTypeID(name string) (int, bool) {
	if g == nil || g.w == nil {
		return 0, false
	}
	i, ok := g.w.ArmorTypeIndex(name)
	return int(i), ok
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

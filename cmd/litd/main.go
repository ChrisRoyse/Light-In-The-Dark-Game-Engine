// Command litd loads a world directory — validated data tables plus sandboxed
// Lua scripts — and runs it headless, with no engine recompile (#268). A world
// is <dir>/data/** (unit/combat/... tables, validated by litd/data) plus
// <dir>/main.lua (the entry chunk, executed inside the R-SEC-1 sandbox under an
// instruction budget). Editing the world's .lua or data and re-running needs no
// rebuild of this binary.
package main

import (
	"encoding/json"
	"flag"
	"fmt"
	"os"
	"path/filepath"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/luabind"
)

func main() {
	world := flag.String("world", "", "world directory to load (contains data/ and main.lua)")
	autotest := flag.Bool("autotest", false, "advance -ticks then print the sim state as JSON")
	ticks := flag.Int("ticks", 40, "ticks to advance under -autotest")
	seed := flag.Int64("seed", 1, "deterministic PRNG seed (R-SIM-2)")
	budget := flag.Int64("budget", 50_000_000, "per-eval Lua instruction budget (R-SEC-1 quota)")
	flag.Parse()

	if err := run(*world, *autotest, *ticks, *seed, *budget); err != nil {
		fmt.Fprintln(os.Stderr, "litd:", err)
		os.Exit(1)
	}
}

func run(world string, autotest bool, ticks int, seed, budget int64) error {
	g, cleanup, err := loadWorld(world, seed, budget)
	if err != nil {
		return err
	}
	defer cleanup()

	// Headless run + Source-of-Truth state dump. The interpreter is kept alive
	// (by cleanup deferral) across Advance so any handlers the world registered
	// can fire on later ticks.
	if autotest {
		g.Advance(ticks)
		printState(g, ticks)
	}
	return nil
}

// loadWorld loads + validates a world's data tables, installs them, and runs
// the world's sandboxed scripts through the public g.LoadWorld verb. It returns
// the live game and a cleanup func the caller must defer (it closes the
// interpreter and chunk registry, which must outlive any sim Advance because
// the world's script callbacks run on them). Every failure is loud and at load
// time — never mid-match (#268 constraint).
func loadWorld(world string, seed, budget int64) (*api.Game, func(), error) {
	if world == "" {
		return nil, nil, fmt.Errorf("missing -world <dir>")
	}

	// 1. Load + validate the world's data tables.
	tables, err := data.Load(os.DirFS(filepath.Join(world, "data")))
	if err != nil {
		return nil, nil, fmt.Errorf("load data tables: %w", err)
	}

	// 2. Fail-closed scaffold: every content table now has an api install seam
	//    (units/effects/abilities/items/buffs/upgrades #394, heroes #396,
	//    resource nodes #401). uninstallableTables stays as the registration
	//    point — if a future table type ships without a seam it is refused here
	//    rather than silently dropped.
	if missing := uninstallableTables(tables); missing != "" {
		return nil, nil, fmt.Errorf("world ships %s, but that table has no api install seam yet; "+
			"refusing to load a partial world", missing)
	}

	// 3. New game, then install the data tables in dependency order: effects
	//    (referenced by items/abilities) → units (referenced by upgrades) →
	//    abilities/items/buffs → upgrades. Empty optional tables are skipped —
	//    the item/upgrade seams reject an empty table by design.
	g, err := api.NewGame(api.GameOptions{Seed: seed, MaxUnits: 256})
	if err != nil {
		return nil, nil, fmt.Errorf("new game: %w", err)
	}
	// Combat coefficient matrix first (#406): the world's required damage table
	// (data/combat) parses to tables.Coeff; until it is installed combat
	// resolution drops every hit, so a loaded world's units could not take
	// damage. The api takes [][]int, so widen the sim's [][]int32.
	if len(tables.Coeff) > 0 {
		// Declare the named attack/armor type tables first (#472) so DefineCombat
		// validates the matrix dims against them — a world that adds a type but
		// forgets a matrix row/column fails loudly here, not silently in combat.
		if len(tables.AttackTypes) > 0 && len(tables.ArmorTypes) > 0 {
			if err := g.DefineDamageTypes(tables.AttackTypes, tables.ArmorTypes); err != nil {
				return nil, nil, fmt.Errorf("define damage types: %w", err)
			}
		}
		coeff := make([][]int, len(tables.Coeff))
		for i, row := range tables.Coeff {
			coeff[i] = make([]int, len(row))
			for j, v := range row {
				coeff[i][j] = int(v)
			}
		}
		if err := g.DefineCombat(coeff); err != nil {
			return nil, nil, fmt.Errorf("define combat: %w", err)
		}
	}
	if len(tables.ResourceTypes) > 0 {
		if err := g.DefineEconomy(len(tables.ResourceTypes)); err != nil {
			return nil, nil, fmt.Errorf("define economy: %w", err)
		}
	}
	// Resource-node types after the economy (a node's Resource index is checked
	// against the resource count at spawn) (#401).
	if len(tables.Nodes) > 0 {
		if err := g.DefineResourceNodes(tables.Nodes); err != nil {
			return nil, nil, fmt.Errorf("define resource nodes: %w", err)
		}
	}
	if len(tables.Effects) > 0 {
		if err := g.DefineEffects(tables.Effects); err != nil {
			return nil, nil, fmt.Errorf("define effects: %w", err)
		}
	}
	if err := g.DefineUnits(tables.Units); err != nil {
		return nil, nil, fmt.Errorf("define units: %w", err)
	}
	if len(tables.Abilities) > 0 {
		if err := g.DefineAbilities(tables.Abilities); err != nil {
			return nil, nil, fmt.Errorf("define abilities: %w", err)
		}
	}
	if len(tables.Items) > 0 {
		if err := g.DefineItems(tables.Items); err != nil {
			return nil, nil, fmt.Errorf("define items: %w", err)
		}
	}
	if len(tables.BuffTypes) > 0 {
		if err := g.DefineBuffTypes(tables.BuffTypes); err != nil {
			return nil, nil, fmt.Errorf("define buff types: %w", err)
		}
	}
	if len(tables.Upgrades) > 0 {
		if err := g.DefineUpgrades(tables.Upgrades, tables.Requires); err != nil {
			return nil, nil, fmt.Errorf("define upgrades: %w", err)
		}
	}
	// Heroes last: BindHeroes consults the unit defs (always), the ability defs
	// (if a hero has skills), and the resource count (if revive costs are set) —
	// all bound above by this point (#396).
	if tables.Hero != nil {
		if err := g.DefineHeroes(tables.Hero); err != nil {
			return nil, nil, fmt.Errorf("define heroes: %w", err)
		}
	}

	// 3b. Declarative placement (#403): spawn the world's placement rows after the
	//     type tables are installed and before main.lua runs. Rows are in a
	//     canonical order (data layer), so entity-id assignment is deterministic;
	//     an unknown code or a failed spawn fails the load loudly.
	if tables.Placement != nil {
		for _, pu := range tables.Placement.Units {
			typ := g.UnitType(pu.Type)
			if typ.IsZero() {
				return nil, nil, fmt.Errorf("placement: unknown unit type %q", pu.Type)
			}
			if !g.CreateUnit(g.Player(pu.Owner), typ, api.Vec2{X: pu.X, Y: pu.Y}, api.Deg(pu.Facing)).Valid() {
				return nil, nil, fmt.Errorf("placement: failed to spawn unit %q at (%g,%g)", pu.Type, pu.X, pu.Y)
			}
		}
		for _, pn := range tables.Placement.Nodes {
			typ := g.ResourceNodeType(pn.Type)
			if typ.IsZero() {
				return nil, nil, fmt.Errorf("placement: unknown node type %q", pn.Type)
			}
			if !g.CreateResourceNode(typ, api.Vec2{X: pn.X, Y: pn.Y}).Valid() {
				return nil, nil, fmt.Errorf("placement: failed to spawn node %q at (%g,%g)", pn.Type, pn.X, pn.Y)
			}
		}
	}

	// 4. Sandbox + bindings + world-loader seam, then run the world's scripts
	//    through the public g.LoadWorld verb.
	// RandomSource wires Lua math.random to the sim PRNG (#400/#263): a loaded
	// world's math.random draws deterministically from sim state (R-SIM-2), not
	// a raise. The game exists here (built above), so its draw is bindable.
	interp := luabind.NewSandbox(luabind.SandboxOptions{InstructionBudget: budget, RandomSource: g.RandomFloat})
	reg := luabind.NewChunkRegistry()
	cleanup := func() { reg.Close(); interp.Close() }
	if err := luabind.Register(interp.L, g); err != nil {
		cleanup()
		return nil, nil, fmt.Errorf("register bindings: %w", err)
	}
	luabind.InstallWorldLoader(g, interp.L, reg)
	if err := g.LoadWorld(world); err != nil {
		cleanup()
		return nil, nil, fmt.Errorf("load world: %w", err)
	}
	return g, cleanup, nil
}

// uninstallableTables names any content table present that still has no api
// install seam (so the caller can fail closed). After #394 + #396 + #401 every
// content table installs — units, effects, abilities, items, buff types,
// upgrades, heroes, and resource-node types — so this returns "" today. It is
// kept as the fail-closed registration point: a new table type added to
// data.Tables without an install seam should be named here until it has one.
// The combat vocabulary (AttackTypes/ArmorTypes/Coeff) is always present because
// data.Load requires a damage table; it is not itself installed, but a unit-only
// world that does no combat never exercises it, so it is not uninstallable.
func uninstallableTables(t *data.Tables) string {
	_ = t
	return ""
}

type unitState struct {
	X      float64 `json:"x"`
	Y      float64 `json:"y"`
	Facing float64 `json:"facing"`
	Life   float64 `json:"life"`
}

// printState writes the sim state as the JSON line an FSV reader inspects (the
// "state:" convention firstlight uses). Units are enumerated by a whole-map
// range query (no all-units iterator on the public surface yet).
func printState(g *api.Game, ticks int) {
	var us []unitState
	for _, u := range g.UnitsInRange(api.Vec2{}, 1e9, nil) {
		p := u.Position()
		us = append(us, unitState{X: p.X, Y: p.Y, Facing: u.Facing().Degrees(), Life: u.Life()})
	}
	s := struct {
		TimeOfDay float64     `json:"tod"`
		Ticks     int         `json:"ticks"`
		Units     []unitState `json:"units"`
	}{g.TimeOfDay(), ticks, us}
	out, _ := json.Marshal(s)
	fmt.Printf("state: %s\n", out)
}

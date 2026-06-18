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

	// 2. Fail-closed: units + the non-unit content tables now have api install
	//    seams (#394). Resource-node and hero tables still lack a seam, so a
	//    world shipping those must fail loudly rather than load a partial world
	//    that silently drops them.
	if missing := uninstallableTables(tables); missing != "" {
		return nil, nil, fmt.Errorf("world ships %s, but that table has no api install seam yet "+
			"(see #394); refusing to load a partial world", missing)
	}

	// 3. New game, then install the data tables in dependency order: effects
	//    (referenced by items/abilities) → units (referenced by upgrades) →
	//    abilities/items/buffs → upgrades. Empty optional tables are skipped —
	//    the item/upgrade seams reject an empty table by design.
	g, err := api.NewGame(api.GameOptions{Seed: seed, MaxUnits: 256})
	if err != nil {
		return nil, nil, fmt.Errorf("new game: %w", err)
	}
	if len(tables.ResourceTypes) > 0 {
		if err := g.DefineEconomy(len(tables.ResourceTypes)); err != nil {
			return nil, nil, fmt.Errorf("define economy: %w", err)
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

	// 4. Sandbox + bindings + world-loader seam, then run the world's scripts
	//    through the public g.LoadWorld verb.
	interp := luabind.NewSandbox(luabind.SandboxOptions{InstructionBudget: budget})
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
// install seam (so the caller can fail closed). After #394, units, effects,
// abilities, items, buff types, and upgrades all install; only resource-node
// and hero tables remain seamless. The combat vocabulary
// (AttackTypes/ArmorTypes/Coeff) is always present because data.Load requires a
// damage table; it is not itself installed, but a unit-only world that does no
// combat never exercises it, so it is not treated as uninstallable content.
func uninstallableTables(t *data.Tables) string {
	switch {
	case len(t.Nodes) > 0:
		return fmt.Sprintf("%d resource-node type(s)", len(t.Nodes))
	case t.Hero != nil:
		return "hero tables"
	}
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

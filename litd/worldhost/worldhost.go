// Package worldhost loads a world directory — validated data tables plus
// sandboxed Lua scripts — into a live, ready-to-advance game, with no engine
// recompile (#268). It is the reusable core extracted from cmd/litd (#490): any
// host — the headless runner, the savegame round-trip, or a render harness —
// loads a world through Load and gets back the game plus the Lua state and chunk
// registry the savegame container needs.
//
// A world is <dir>/data/** (unit/combat/... tables, validated by litd/data) plus
// <dir>/main.lua (the entry chunk, executed inside the R-SEC-1 sandbox under an
// instruction budget). Loading fails closed and loud: a bad data table, an
// uninstallable table type, a failed placement, or a script fault is returned as
// an error here — a broken world never starts a match.
package worldhost

import (
	"errors"
	"fmt"
	"io/fs"
	"os"
	"path"
	"path/filepath"
	"sort"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	mapdata "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/asset/mapdata"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/asset/worldarchive"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/luabind"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
	lua "github.com/yuin/gopher-lua"
)

// Host is a loaded world: the live game plus the Lua state and chunk registry
// that own its suspended scheduler (needed by litd/savegame.Write/Load). Call
// Close exactly once when done; it releases the registry and interpreter.
type Host struct {
	Game *api.Game
	L    *lua.LState
	Reg  *luabind.ChunkRegistry

	closeFn func()
}

// Close releases the interpreter and chunk registry. Safe to call once.
func (h *Host) Close() {
	if h != nil && h.closeFn != nil {
		h.closeFn()
		h.closeFn = nil
	}
}

// Load loads, validates, and runs a world directory, returning a ready-to-advance
// Host. seed is the deterministic PRNG seed (R-SIM-2); budget is the per-eval Lua
// instruction quota (R-SEC-1).
func Load(world string, seed, budget int64) (*Host, error) {
	if world == "" {
		return nil, fmt.Errorf("missing world dir")
	}
	dataFS := os.DirFS(filepath.Join(world, "data"))
	scriptFS := os.DirFS(world)
	return loadFS(dataFS, scriptFS, world, seed, budget, nil)
}

// LoadArchive opens and verifies a .litdworld with the production archive
// loader, then loads its data/ and Lua entry directly from the verified archive
// filesystem. engineVersion, when non-empty, must satisfy the archive manifest.
func LoadArchive(archivePath, engineVersion string, seed, budget int64) (*Host, error) {
	if archivePath == "" {
		return nil, fmt.Errorf("missing world archive")
	}
	arc, err := worldarchive.Open(archivePath, engineVersion)
	if err != nil {
		return nil, err
	}
	dataFS, err := fs.Sub(arc.FS(), "data")
	if err != nil {
		arc.Close()
		return nil, fmt.Errorf("archive data mount: %w", err)
	}
	scriptFS, scriptLabel, err := archiveScriptFS(arc.FS(), archivePath)
	if err != nil {
		arc.Close()
		return nil, err
	}
	return loadFS(dataFS, scriptFS, scriptLabel, seed, budget, func() { _ = arc.Close() })
}

func archiveScriptFS(archiveFS fs.FS, archivePath string) (fs.FS, string, error) {
	if _, err := fs.Stat(archiveFS, luabind.WorldEntry); err == nil {
		return archiveFS, archivePath, nil
	} else if !os.IsNotExist(err) {
		return nil, "", fmt.Errorf("archive script entry check: %w", err)
	}
	if _, err := fs.Stat(archiveFS, "scripts/"+luabind.WorldEntry); err == nil {
		sub, subErr := fs.Sub(archiveFS, "scripts")
		if subErr != nil {
			return nil, "", fmt.Errorf("archive scripts mount: %w", subErr)
		}
		return sub, archivePath + "@scripts", nil
	} else if !os.IsNotExist(err) {
		return nil, "", fmt.Errorf("archive scripts entry check: %w", err)
	}
	return nil, "", fmt.Errorf("archive %q has no %s or scripts/%s", archivePath, luabind.WorldEntry, luabind.WorldEntry)
}

func loadRuntimeMap(dataFS fs.FS) (*mapdata.Map, error) {
	entries, err := fs.ReadDir(dataFS, "maps")
	if errors.Is(err, fs.ErrNotExist) {
		return nil, nil
	}
	if err != nil {
		return nil, fmt.Errorf("runtime maps directory: %w", err)
	}
	var dirs []string
	for _, ent := range entries {
		if !ent.IsDir() {
			continue
		}
		dir := path.Join("maps", ent.Name())
		if _, err := fs.Stat(dataFS, path.Join(dir, "terrain.toml")); err == nil {
			dirs = append(dirs, dir)
		} else if !errors.Is(err, fs.ErrNotExist) {
			return nil, fmt.Errorf("runtime map %s terrain check: %w", dir, err)
		}
	}
	sort.Strings(dirs)
	if len(dirs) == 0 {
		return nil, nil
	}
	if len(dirs) > 1 {
		return nil, fmt.Errorf("world ships %d runtime maps (%v); load path requires exactly one", len(dirs), dirs)
	}
	m, err := mapdata.Load(dataFS, dirs[0])
	if err != nil {
		return nil, fmt.Errorf("load runtime map %s: %w", dirs[0], err)
	}
	return m, nil
}

func loadFS(dataFS, scriptFS fs.FS, scriptLabel string, seed, budget int64, extraCleanup func()) (host *Host, err error) {
	extraClosed := false
	closeExtra := func() {
		if extraCleanup != nil && !extraClosed {
			extraClosed = true
			extraCleanup()
		}
	}
	defer func() {
		if err != nil {
			closeExtra()
		}
	}()

	// 1. Load + validate the world's data tables.
	tables, err := data.Load(dataFS)
	if err != nil {
		return nil, fmt.Errorf("load data tables: %w", err)
	}

	// 2. Fail-closed scaffold: every content table now has an api install seam
	//    (units/effects/abilities/items/buffs/upgrades #394, heroes #396,
	//    resource nodes #401). uninstallableTables stays as the registration
	//    point — if a future table type ships without a seam it is refused here
	//    rather than silently dropped.
	if missing := uninstallableTables(tables); missing != "" {
		return nil, fmt.Errorf("world ships %s, but that table has no api install seam yet; "+
			"refusing to load a partial world", missing)
	}

	runtimeMap, err := loadRuntimeMap(dataFS)
	if err != nil {
		return nil, err
	}

	// 3. New game, then install the data tables in dependency order: effects
	//    (referenced by items/abilities) → units (referenced by upgrades) →
	//    abilities/items/buffs → upgrades. Empty optional tables are skipped —
	//    the item/upgrade seams reject an empty table by design.
	g, err := api.NewGame(api.GameOptions{Seed: seed, MaxUnits: 256, Map: runtimeMap})
	if err != nil {
		return nil, fmt.Errorf("new game: %w", err)
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
				return nil, fmt.Errorf("define damage types: %w", err)
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
			return nil, fmt.Errorf("define combat: %w", err)
		}
	}
	if len(tables.ResourceTypes) > 0 {
		if err := g.DefineEconomy(len(tables.ResourceTypes)); err != nil {
			return nil, fmt.Errorf("define economy: %w", err)
		}
	}
	// Resource-node types after the economy (a node's Resource index is checked
	// against the resource count at spawn) (#401).
	if len(tables.Nodes) > 0 {
		if err := g.DefineResourceNodes(tables.Nodes); err != nil {
			return nil, fmt.Errorf("define resource nodes: %w", err)
		}
	}
	if len(tables.Effects) > 0 {
		// Register the core effect-primitive backends once per process before
		// binding any arena (#479) — without this a world that ships an
		// effect-using ability or buff fails closed at DefineEffects.
		sim.EnsureCoreEffectExecs()
		if err := g.DefineEffects(tables.Effects); err != nil {
			return nil, fmt.Errorf("define effects: %w", err)
		}
	}
	if err := g.DefineUnits(tables.Units); err != nil {
		return nil, fmt.Errorf("define units: %w", err)
	}
	if len(tables.Abilities) > 0 {
		if err := g.DefineAbilities(tables.Abilities); err != nil {
			return nil, fmt.Errorf("define abilities: %w", err)
		}
	}
	if len(tables.Items) > 0 {
		if err := g.DefineItems(tables.Items); err != nil {
			return nil, fmt.Errorf("define items: %w", err)
		}
	}
	if len(tables.BuffTypes) > 0 {
		if err := g.DefineBuffTypes(tables.BuffTypes); err != nil {
			return nil, fmt.Errorf("define buff types: %w", err)
		}
	}
	if len(tables.Upgrades) > 0 {
		if err := g.DefineUpgrades(tables.Upgrades, tables.Requires); err != nil {
			return nil, fmt.Errorf("define upgrades: %w", err)
		}
	}
	// Heroes last: BindHeroes consults the unit defs (always), the ability defs
	// (if a hero has skills), and the resource count (if revive costs are set) —
	// all bound above by this point (#396).
	if tables.Hero != nil {
		if err := g.DefineHeroes(tables.Hero); err != nil {
			return nil, fmt.Errorf("define heroes: %w", err)
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
				return nil, fmt.Errorf("placement: unknown unit type %q", pu.Type)
			}
			if !g.CreateUnit(g.Player(pu.Owner), typ, api.Vec2{X: pu.X, Y: pu.Y}, api.Deg(pu.Facing)).Valid() {
				return nil, fmt.Errorf("placement: failed to spawn unit %q at (%g,%g)", pu.Type, pu.X, pu.Y)
			}
		}
		for _, pn := range tables.Placement.Nodes {
			typ := g.ResourceNodeType(pn.Type)
			if typ.IsZero() {
				return nil, fmt.Errorf("placement: unknown node type %q", pn.Type)
			}
			if !g.CreateResourceNode(typ, api.Vec2{X: pn.X, Y: pn.Y}).Valid() {
				return nil, fmt.Errorf("placement: failed to spawn node %q at (%g,%g)", pn.Type, pn.X, pn.Y)
			}
		}
	}

	// 3c. Special-effect model registry (#530): bind each declared model key to a
	//     deterministic sim ModelID (1..N over the loader's Key-sorted rows) so the
	//     world's main.lua can Game_AddSpecialEffect(key, pos) and get a live
	//     handle. Without this the Lua-bound AddSpecialEffect fails closed on every
	//     authored world (unknown model). Registered before the scripts run.
	for i := range tables.EffectModels {
		g.RegisterEffectModel(tables.EffectModels[i].Key, uint16(i+1))
	}

	// 4. Sandbox + bindings + world-loader seam, then run the world's scripts
	//    through the public g.LoadWorld verb.
	// RandomSource wires Lua math.random to the sim PRNG (#400/#263): a loaded
	// world's math.random draws deterministically from sim state (R-SIM-2), not
	// a raise. The game exists here (built above), so its draw is bindable.
	interp := luabind.NewSandbox(luabind.SandboxOptions{InstructionBudget: budget, RandomSource: g.RandomFloat})
	reg := luabind.NewChunkRegistry()
	cleanup := func() {
		reg.Close()
		interp.Close()
		closeExtra()
	}
	if err := luabind.Register(interp.L, g); err != nil {
		cleanup()
		return nil, fmt.Errorf("register bindings: %w", err)
	}
	luabind.InstallWorldLoader(g, interp.L, reg)
	if _, err := luabind.LoadWorldFS(interp.L, reg, scriptFS, scriptLabel); err != nil {
		cleanup()
		return nil, fmt.Errorf("load world: %w", err)
	}
	return &Host{Game: g, L: interp.L, Reg: reg, closeFn: cleanup}, nil
}

// uninstallableTables is the registration point for content-table types that have
// no api install seam yet (#394). Every shipped table type installs today, so it
// returns "" — a future table type without a seam is named here to fail the load
// loudly rather than be silently dropped.
func uninstallableTables(t *data.Tables) string {
	_ = t
	return ""
}

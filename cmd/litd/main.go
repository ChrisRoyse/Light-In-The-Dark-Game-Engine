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

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/luabind"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/worldhost"
	lua "github.com/yuin/gopher-lua"
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

// loadWorld loads a world for headless run, returning just the game handle.
// Thin wrapper over loadWorldFull for callers that don't need the Lua state /
// chunk registry (everything but the savegame round-trip).
func loadWorld(world string, seed, budget int64) (*api.Game, func(), error) {
	g, _, _, cleanup, err := loadWorldFull(world, seed, budget)
	return g, cleanup, err
}

// loadWorldFull loads a world and also returns its Lua state + chunk registry,
// so a caller can drive the production save/load container (litd/savegame.Write
// /Load, #204/#481) which bundles the sim state with the suspended Lua
// scheduler. The two handles are owned by the returned cleanup. The loader lives
// in litd/worldhost (#490) so a render harness can load worlds identically; this
// stays as the cmd/litd adapter to the 5-value shape its callers use.
func loadWorldFull(world string, seed, budget int64) (*api.Game, *lua.LState, *luabind.ChunkRegistry, func(), error) {
	h, err := worldhost.Load(world, seed, budget)
	if err != nil {
		return nil, nil, nil, nil, err
	}
	return h.Game, h.L, h.Reg, h.Close, nil
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

// Command fsv is a headless Full State Verification scenario harness (#516).
//
// It exists to make manual FSV (prompts/fsv.md) FAST without weakening it. The
// doctrine fixes who renders the verdict — the agent, reading the Source of
// Truth, never an exit code. This tool does NOT decide pass/fail; it only does
// the slow parts (drive a synthetic scenario through the deterministic core,
// capture the SoT before and after, diff it) and emits a structured, cheap-to-
// read JSON delta the agent inspects. No GLFW/GL window is opened: the sim never
// imports render (arch rule), so a scenario runs far faster headless than
// firstlight -autotest, and reading the JSON delta costs a fraction of a
// screenshot vision-read.
//
// SoT captured each scenario: the full R-FSV-2 state dump (api.Game.DumpState),
// the 64-bit top state hash (StateHash), and the per-system sub-hash vector
// (HashSnapshot) named by HashSystemNames — so a divergence is localized to a
// named system, not just "something changed".
//
// Determinism is verified intrinsically: the scenario runs TWICE and the harness
// reports whether the two after-hashes agree. A scenario whose double run
// diverges is non-deterministic and the report says so loudly.
//
// Usage:
//
//	go run ./tools/fsv -scenario path/to/scenario.toml          # report to stdout
//	go run ./tools/fsv -scenario s.toml -out report.json        # report to file
//	go run ./tools/fsv -scenario s.toml -dump                   # include full before/after state dumps
package main

import (
	"bytes"
	"encoding/json"
	"flag"
	"fmt"
	"os"

	"github.com/BurntSushi/toml"
	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/luabind"
	lua "github.com/yuin/gopher-lua"
)

// Scenario is the synthetic FSV input. It is deliberately self-contained and
// deterministic: a known seed, known unit defs, known spawns, an optional Lua
// setup script (triggers/orders), and a tick count. Known input -> known output
// is the X+X=Y discipline (fsv.md §4.4) made executable.
type Scenario struct {
	Name   string       `toml:"name"`
	Seed   uint64       `toml:"seed"`
	Ticks  int          `toml:"ticks"`
	Units  []unitSpec   `toml:"units"`
	Lua    string       `toml:"lua"`     // inline setup script (triggers, regions, conditions)
	LuaFmt string       `toml:"luaFile"` // OR a path to a setup script
	Spawns []spawnSpec  `toml:"spawns"`
	Expect *expectBlock `toml:"expect"` // optional human-authored expectations, echoed into the report
}

// unitSpec is the synthetic unit type. Speeds/ranges are given as whole world
// units and converted to 32.32 fixed via fixed.FromInt — no floats touch the
// fixed-point path (floats are banned there, #335).
type unitSpec struct {
	ID            string `toml:"id"`
	Life          int32  `toml:"life"`
	MoveSpeed     int32  `toml:"moveSpeed"`     // world units / tick
	TurnRate      uint32 `toml:"turnRate"`      // BAM / tick (65535 = instant)
	CollisionSize int32  `toml:"collisionSize"` // world-unit radius
}

type spawnSpec struct {
	Unit      string  `toml:"unit"`
	Player    int     `toml:"player"`
	X         float64 `toml:"x"`
	Y         float64 `toml:"y"`
	FacingDeg float64 `toml:"facingDeg"`
}

// expectBlock is purely documentary: whatever the author believes the after
// state should be. The harness echoes it next to the observed delta so the agent
// can compare expected-vs-observed in one glance, but it never gates the run.
type expectBlock struct {
	Notes string `toml:"notes"`
}

// Report is the structured SoT delta the agent reads. Everything an agent needs
// to render an FSV verdict is here in text: the top hash before/after, the named
// systems that changed, and (with -dump) the full before/after state dumps.
type Report struct {
	Scenario      string             `json:"scenario"`
	Seed          uint64             `json:"seed"`
	Ticks         int                `json:"ticks"`
	Units         int                `json:"units"`
	Spawns        int                `json:"spawns"`
	HashBefore    string             `json:"hashBefore"`
	HashAfter     string             `json:"hashAfter"`
	Changed       bool               `json:"changed"`
	ChangedSys    []string           `json:"changedSystems"`
	SubHashes     map[string]subPair `json:"subHashes"`
	Deterministic bool               `json:"deterministic"`
	HashAfterRun2 string             `json:"hashAfterRun2"`
	Expect        string             `json:"expect,omitempty"`
	StateBefore   json.RawMessage    `json:"stateBefore,omitempty"`
	StateAfter    json.RawMessage    `json:"stateAfter,omitempty"`
}

type subPair struct {
	Before string `json:"before"`
	After  string `json:"after"`
}

func main() {
	scenarioPath := flag.String("scenario", "", "path to a scenario TOML (required)")
	outPath := flag.String("out", "", "write JSON report here (default: stdout)")
	withDump := flag.Bool("dump", false, "include full before/after R-FSV-2 state dumps in the report")
	flag.Parse()

	if *scenarioPath == "" {
		fmt.Fprintln(os.Stderr, "fsv: -scenario is required")
		os.Exit(2)
	}
	sc, err := loadScenario(*scenarioPath)
	if err != nil {
		fmt.Fprintf(os.Stderr, "fsv: load scenario: %v\n", err)
		os.Exit(2)
	}
	rep, err := runScenario(sc, *withDump)
	if err != nil {
		fmt.Fprintf(os.Stderr, "fsv: run scenario: %v\n", err)
		os.Exit(1)
	}
	out, err := json.MarshalIndent(rep, "", "  ")
	if err != nil {
		fmt.Fprintf(os.Stderr, "fsv: encode report: %v\n", err)
		os.Exit(1)
	}
	out = append(out, '\n')
	if *outPath == "" {
		os.Stdout.Write(out)
	} else if err := os.WriteFile(*outPath, out, 0o644); err != nil {
		fmt.Fprintf(os.Stderr, "fsv: write report: %v\n", err)
		os.Exit(1)
	}
}

func loadScenario(path string) (Scenario, error) {
	var sc Scenario
	body, err := os.ReadFile(path)
	if err != nil {
		return sc, err
	}
	md, err := toml.Decode(string(body), &sc)
	if err != nil {
		return sc, fmt.Errorf("decode %s: %w", path, err)
	}
	// Fail-closed on undecoded keys. A key the struct does not claim is almost
	// always a structural TOML mistake — most commonly a top-level scalar (lua,
	// expect, ticks) written AFTER a [[units]] array-of-tables, which TOML then
	// nests under the array element instead of the document root. Silently
	// dropping it produced an empty scenario that "passed" while verifying
	// nothing (fsv.md: no silent gaps). Reject it loudly with the exact keys.
	if undec := md.Undecoded(); len(undec) > 0 {
		keys := make([]string, len(undec))
		for i, k := range undec {
			keys[i] = k.String()
		}
		return sc, fmt.Errorf("decode %s: unknown/misplaced keys %v — top-level keys (lua, ticks, expect) must precede any [[units]]/[[spawns]] array", path, keys)
	}
	if sc.LuaFmt != "" {
		luaBody, err := os.ReadFile(sc.LuaFmt)
		if err != nil {
			return sc, fmt.Errorf("read luaFile %s: %w", sc.LuaFmt, err)
		}
		if sc.Lua != "" {
			return sc, fmt.Errorf("scenario sets both lua and luaFile; pick one")
		}
		sc.Lua = string(luaBody)
	}
	if sc.Ticks < 0 {
		return sc, fmt.Errorf("ticks must be >= 0, got %d", sc.Ticks)
	}
	if len(sc.Units) == 0 {
		return sc, fmt.Errorf("scenario needs at least one unit definition")
	}
	return sc, nil
}

// runScenario executes the scenario twice. The first run captures the BEFORE and
// AFTER SoT; the second run re-derives the AFTER hash to prove determinism. Both
// runs are independent fresh games — no shared mutable state — so a hash match
// means the deterministic core is reproducible for this scenario.
func runScenario(sc Scenario, withDump bool) (Report, error) {
	beforeTop, beforeSubs, beforeDump, afterTop, afterSubs, afterDump, err := runOnce(sc, withDump)
	if err != nil {
		return Report{}, err
	}
	// Second run: only the after hash is needed for the determinism check.
	_, _, _, afterTop2, _, _, err := runOnce(sc, false)
	if err != nil {
		return Report{}, fmt.Errorf("determinism re-run: %w", err)
	}

	names := api.HashSystemNames()
	subPairs := make(map[string]subPair, len(names))
	var changedSys []string
	for i, name := range names {
		var b, a uint64
		if i < len(beforeSubs) {
			b = beforeSubs[i]
		}
		if i < len(afterSubs) {
			a = afterSubs[i]
		}
		subPairs[name] = subPair{Before: hex64(b), After: hex64(a)}
		if b != a {
			changedSys = append(changedSys, name)
		}
	}

	rep := Report{
		Scenario:      orDefault(sc.Name, "unnamed"),
		Seed:          sc.Seed,
		Ticks:         sc.Ticks,
		Units:         len(sc.Units),
		Spawns:        len(sc.Spawns),
		HashBefore:    hex64(beforeTop),
		HashAfter:     hex64(afterTop),
		Changed:       beforeTop != afterTop,
		ChangedSys:    changedSys,
		SubHashes:     subPairs,
		Deterministic: afterTop == afterTop2,
		HashAfterRun2: hex64(afterTop2),
	}
	if sc.Expect != nil {
		rep.Expect = sc.Expect.Notes
	}
	if withDump {
		rep.StateBefore = json.RawMessage(beforeDump)
		rep.StateAfter = json.RawMessage(afterDump)
	}
	return rep, nil
}

func runOnce(sc Scenario, withDump bool) (beforeTop uint64, beforeSubs []uint64, beforeDump []byte, afterTop uint64, afterSubs []uint64, afterDump []byte, err error) {
	maxUnits := len(sc.Spawns) + 8
	g, err := api.NewGame(api.GameOptions{MaxUnits: maxUnits, Seed: int64(sc.Seed)})
	if err != nil {
		return 0, nil, nil, 0, nil, nil, fmt.Errorf("NewGame: %w", err)
	}
	defs := make([]data.Unit, 0, len(sc.Units))
	for _, u := range sc.Units {
		if u.ID == "" {
			return 0, nil, nil, 0, nil, nil, fmt.Errorf("unit def with empty id")
		}
		defs = append(defs, data.Unit{
			ID:               u.ID,
			Life:             u.Life,
			MoveSpeedPerTick: fixed.FromInt(u.MoveSpeed),
			TurnRatePerTick:  fixed.Angle(u.TurnRate),
			CollisionSize:    u.CollisionSize,
		})
	}
	if err := g.DefineUnits(defs); err != nil {
		return 0, nil, nil, 0, nil, nil, fmt.Errorf("DefineUnits: %w", err)
	}

	// Setup script (triggers/regions/conditions) runs BEFORE spawns, mirroring
	// the WC3 InitTrigger -> map-start ordering: a unit must not enter a region
	// before the enter-trigger that watches it is registered.
	if sc.Lua != "" {
		L := lua.NewState()
		defer L.Close()
		if err := luabind.Register(L, g); err != nil {
			return 0, nil, nil, 0, nil, nil, fmt.Errorf("luabind.Register: %w", err)
		}
		if err := L.DoString(sc.Lua); err != nil {
			return 0, nil, nil, 0, nil, nil, fmt.Errorf("scenario lua: %w", err)
		}
	}
	for i, s := range sc.Spawns {
		typ := g.UnitType(s.Unit)
		g.CreateUnit(g.Player(s.Player), typ, api.Vec2{X: s.X, Y: s.Y}, api.Deg(s.FacingDeg))
		_ = i
	}

	beforeTop, beforeSubs = g.HashSnapshot()
	if withDump {
		var buf bytes.Buffer
		g.DumpState(&buf)
		beforeDump = append([]byte(nil), buf.Bytes()...)
	}

	g.Advance(sc.Ticks)

	afterTop, afterSubs = g.HashSnapshot()
	if withDump {
		var buf bytes.Buffer
		g.DumpState(&buf)
		afterDump = append([]byte(nil), buf.Bytes()...)
	}
	return beforeTop, beforeSubs, beforeDump, afterTop, afterSubs, afterDump, nil
}

func hex64(v uint64) string { return fmt.Sprintf("0x%016x", v) }

func orDefault(s, d string) string {
	if s == "" {
		return d
	}
	return s
}

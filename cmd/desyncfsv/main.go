// Command desyncfsv is the M7 desync-detection FSV harness (#82). It runs a
// scripted multi-client lockstep match over REAL sim instances, optionally
// injects a state divergence into ONE client at a chosen tick, and drives the
// netplay desync detector (#77) — proving a real divergence is caught within K
// turns, bisected to the correct named system, and dumped. It is headless and
// CI-runnable; acceptance is by the printed timeline + the dump files, not the
// exit code alone (firstlight -autotest convention: 0 pass / 2 detection
// timeout / 3 wrong bisection).
//
// Fault injection lives ENTIRELY here (a harness concern), never in litd/sim or
// litd/net production paths. Because order-component application is still pending
// (#144/#146), gameplay orders cannot yet perturb movement/combat state; the
// injectable divergences that are real and cleanly bisectable today are
// state-level: "prng" (an extra PRNG draw → isolates the prng sub-hash) and
// "entities" (an extra unit → isolates the entities sub-hash). Both produce a
// genuine, persistent state divergence on one client.
package main

import (
	"flag"
	"fmt"
	"os"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/net"
)

type config struct {
	clients      int
	injectSystem string // "" = control (no injection)
	injectTick   uint32
	K            int // detection bound, in turns
	turnLen      int
	totalTicks   uint32
	seed         uint32
	dumpDir      string
}

type eventRec struct {
	turn   uint64
	system string
	dumps  map[uint8]string
}

type result struct {
	injectTurn  uint64
	cadence     int // hash cadence, in turns
	events      []eventRec
	comparisons int
	hashRounds  int
}

// supportedInjections are the systems the harness can perturb today. See the
// package doc for why movement/combat are not yet injectable.
var supportedInjections = map[string]bool{"prng": true, "entities": true}

func newClient(cfg config) (*api.Game, error) {
	g, err := api.NewGame(api.GameOptions{MaxUnits: 64, Seed: int64(cfg.seed)})
	if err != nil {
		return nil, err
	}
	if err := g.DefineUnits([]data.Unit{
		{ID: "hfoo", Life: 100, MoveSpeedPerTick: 4 * fixed.One, TurnRatePerTick: 65535, CollisionSize: 16},
	}); err != nil {
		return nil, err
	}
	// Identical baseline so every client's state is byte-identical pre-injection.
	if u := g.CreateUnit(g.Player(0), g.UnitType("hfoo"), api.Vec2{X: 100, Y: 100}, api.Deg(0)); u.ID() == 0 {
		return nil, fmt.Errorf("baseline CreateUnit failed")
	}
	return g, nil
}

// perturb applies the system-targeted divergence to a single client.
func perturb(g *api.Game, system string) error {
	switch system {
	case "prng":
		_ = g.RandomInt(0, 1<<30) // advances only this client's PRNG cursor
	case "entities":
		g.CreateUnit(g.Player(0), g.UnitType("hfoo"), api.Vec2{X: 200, Y: 200}, api.Deg(0))
	default:
		return fmt.Errorf("unsupported -inject-system %q (supported: prng, entities; movement/combat await order-application #144/#146)", system)
	}
	return nil
}

func runHarness(cfg config) (result, error) {
	var res result
	if cfg.clients < 2 {
		return res, fmt.Errorf("need ≥2 clients, got %d", cfg.clients)
	}
	if cfg.injectSystem != "" && !supportedInjections[cfg.injectSystem] {
		return res, fmt.Errorf("unsupported -inject-system %q (supported: prng, entities)", cfg.injectSystem)
	}
	cadence := net.HashCadenceTurns(cfg.turnLen) // ≈ once/sec at 20 Hz
	if cfg.K < cadence {
		return res, fmt.Errorf("K=%d turns is below the hash cadence %d turns; detection cannot be proven within K", cfg.K, cadence)
	}
	res.cadence = cadence
	res.injectTurn = uint64(cfg.injectTick) / uint64(cfg.turnLen)
	cadenceTicks := uint32(cadence * cfg.turnLen)

	games := make([]*api.Game, cfg.clients)
	ids := make([]uint8, cfg.clients)
	for i := range games {
		g, err := newClient(cfg)
		if err != nil {
			return res, fmt.Errorf("client %d setup: %w", i, err)
		}
		games[i] = g
		ids[i] = uint8(i)
	}

	det, err := net.NewDesyncDetector(api.HashSystemNames(), ids, cfg.dumpDir)
	if err != nil {
		return res, err
	}

	for tick := uint32(1); tick <= cfg.totalTicks; tick++ {
		for _, g := range games {
			g.Advance(1)
		}
		// Inject into client 1 (NOT the lowest-id reference) right after the
		// injection tick is simulated, so the divergence is live for every
		// subsequent hash.
		if cfg.injectSystem != "" && tick == cfg.injectTick {
			if err := perturb(games[1], cfg.injectSystem); err != nil {
				return res, err
			}
		}
		if tick%cadenceTicks == 0 {
			turn := uint64(tick) / uint64(cfg.turnLen)
			res.hashRounds++
			for ci, g := range games {
				top, subs := g.HashSnapshot()
				ev, err := det.Report(uint8(ci), net.HashReport{Turn: turn, Top: top, Subs: subs})
				if err != nil {
					return res, fmt.Errorf("detector.Report client %d turn %d: %w", ci, turn, err)
				}
				if ev != nil {
					res.events = append(res.events, eventRec{turn: ev.Turn, system: ev.DivergingSystem, dumps: ev.DumpPaths})
				}
			}
		}
	}
	res.comparisons = det.Comparisons()
	return res, nil
}

func main() {
	cfg := config{}
	flag.IntVar(&cfg.clients, "clients", 2, "number of lockstep clients")
	flag.StringVar(&cfg.injectSystem, "inject-system", "", "system to perturb on one client (prng|entities); empty = control run")
	var injTick int
	flag.IntVar(&injTick, "inject-tick", 400, "tick at which to inject the divergence")
	flag.IntVar(&cfg.K, "K", 10, "detection bound in turns")
	flag.IntVar(&cfg.turnLen, "turn-len", 2, "ticks per turn")
	var total int
	flag.IntVar(&total, "ticks", 600, "total ticks to simulate")
	var seed int
	flag.IntVar(&seed, "seed", 7, "sim seed (shared by all clients)")
	flag.StringVar(&cfg.dumpDir, "dump-dir", "artifacts/desyncfsv", "directory for divergence dumps")
	flag.Parse()
	cfg.injectTick = uint32(injTick)
	cfg.totalTicks = uint32(total)
	cfg.seed = uint32(seed)

	if err := os.MkdirAll(cfg.dumpDir, 0o755); err != nil {
		fmt.Fprintf(os.Stderr, "desyncfsv: dump dir: %v\n", err)
		os.Exit(3)
	}

	res, err := runHarness(cfg)
	if err != nil {
		fmt.Fprintf(os.Stderr, "desyncfsv: %v\n", err)
		os.Exit(3)
	}

	fmt.Printf("desyncfsv: clients=%d turnLen=%d ticks=%d hashCadence=%d turns K=%d\n",
		cfg.clients, cfg.turnLen, cfg.totalTicks, res.cadence, cfg.K)
	fmt.Printf("desyncfsv: hash rounds=%d, agreeing comparisons=%d\n", res.hashRounds, res.comparisons)

	if cfg.injectSystem == "" {
		// Control: zero desync events expected over the whole match.
		fmt.Printf("desyncfsv: CONTROL run (no injection) — desync events=%d\n", len(res.events))
		if len(res.events) != 0 {
			fmt.Fprintf(os.Stderr, "desyncfsv: FALSE POSITIVE — %d events on an undisturbed match\n", len(res.events))
			os.Exit(3)
		}
		fmt.Printf("desyncfsv: PASS — no desync over %d comparisons\n", res.comparisons)
		os.Exit(0)
	}

	fmt.Printf("desyncfsv: INJECT system=%q at tick=%d (turn %d)\n", cfg.injectSystem, cfg.injectTick, res.injectTurn)
	if len(res.events) == 0 {
		fmt.Fprintf(os.Stderr, "desyncfsv: TIMEOUT — divergence never detected within %d ticks\n", cfg.totalTicks)
		os.Exit(2)
	}
	first := res.events[0]
	delta := first.turn - res.injectTurn
	fmt.Printf("desyncfsv: DETECTED at turn %d (Δ=%d turns), named system=%q\n", first.turn, delta, first.system)
	for c, p := range first.dumps {
		fmt.Printf("desyncfsv:   dump client %d → %s\n", c, p)
	}
	if delta > uint64(cfg.K) {
		fmt.Fprintf(os.Stderr, "desyncfsv: detection lag %d turns exceeds K=%d\n", delta, cfg.K)
		os.Exit(2)
	}
	if first.system != cfg.injectSystem {
		fmt.Fprintf(os.Stderr, "desyncfsv: WRONG BISECTION — injected %q, detector named %q\n", cfg.injectSystem, first.system)
		os.Exit(3)
	}
	fmt.Printf("desyncfsv: PASS — %q divergence detected in %d turns (≤K=%d), correctly bisected\n", cfg.injectSystem, delta, cfg.K)
	os.Exit(0)
}

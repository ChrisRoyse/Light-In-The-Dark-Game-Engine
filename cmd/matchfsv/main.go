// Command matchfsv runs a full AI-vs-AI match from a world mod to a real
// terminal result, headless and GL-free (ultimate-test-plan Phase 2, #642/#643/
// #644). It loads a match world (worlds/firstclash) through litd/worldhost — so
// the world's own main.lua reads match.toml, sets both players up, and attaches
// both CPU AIs through the public Lua surface — then steps the deterministic sim
// until a player result latches (or a safety cap), and emits FSV artifacts: the
// final StateHash, an optional state-dump JSON, and an optional .litdreplay.
//
// It deliberately does NOT touch cmd/headless's synthetic-grid determinism gate
// (D3): this is a separate driver around the SAME sim.Step, proving the public
// API + Lua + melee AI close the loop. Builds with CGO_ENABLED=0 (no GL import).
package main

import (
	"bufio"
	"flag"
	"fmt"
	"os"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/worldhost"
)

func fatalf(format string, a ...any) {
	fmt.Fprintf(os.Stderr, "matchfsv: "+format+"\n", a...)
	os.Exit(1)
}

func main() {
	world := flag.String("world", "worlds/firstclash", "match world directory (loaded via worldhost; must ship match.toml)")
	seed := flag.Int64("seed", 1337, "deterministic world seed (R-SIM-2)")
	maxTicks := flag.Int("max-ticks", 30000, "safety tick cap (> 24000 timeout backstop); hitting it before a terminal result is a stalemate bug")
	budget := flag.Int64("budget", 50_000_000, "per-eval Lua instruction budget")
	dumpPath := flag.String("dump", "", "write the final state-dump JSON here (R-FSV-2)")
	hashAt := flag.Int("hash-at", 0, "if >0, print the StateHash at exactly this tick then continue (determinism probe)")
	replayPath := flag.String("replay", "", "record the AI match to this .litdreplay (requires recording before AI attach)")
	flag.Parse()

	if *maxTicks <= 24000 {
		fatalf("-max-ticks=%d must exceed the 24,000-tick score-decide backstop", *maxTicks)
	}

	// Record-before-attach: the world's main.lua attaches the AIs during load, so
	// replay recording (which taps each controller's order stream) must be armed
	// BEFORE the scripts run. LoadRecording does that; plain Load otherwise.
	var (
		h   *worldhost.Host
		err error
	)
	if *replayPath != "" {
		h, err = worldhost.LoadRecording(*world, *seed, *budget)
	} else {
		h, err = worldhost.Load(*world, *seed, *budget)
	}
	if err != nil {
		fatalf("load %q: %v", *world, err)
	}
	defer h.Close()
	g := h.Game

	// Determine the participating slots: any slot the world set up (has units).
	slots := participatingSlots(g)
	if len(slots) < 2 {
		fatalf("world %q stood up %d players; an AI-vs-AI match needs >= 2", *world, len(slots))
	}
	fmt.Printf("== matchfsv: world=%s seed=%d players=%d ==\n", *world, *seed, len(slots))
	for _, s := range slots {
		printRoster(g, s)
	}

	// Step until a terminal result latches, or the safety cap.
	resolvedAt := -1
	for g.Tick() < uint32(*maxTicks) {
		if *hashAt > 0 && int(g.Tick()) == *hashAt {
			fmt.Printf("hash@%d = %#016x\n", *hashAt, g.StateHash())
		}
		g.Advance(1)
		if anyTerminal(g, slots) {
			resolvedAt = int(g.Tick())
			break
		}
	}

	if resolvedAt < 0 {
		fmt.Printf("results @cap %d: %s\n", g.Tick(), resultLine(g, slots))
		fatalf("no terminal result within %d ticks — stalemate (the score-decide backstop should force a winner)", *maxTicks)
	}

	// Report the outcome: winner slot, all results, duration.
	winner := -1
	for _, s := range slots {
		if g.Player(s).Result() == api.ResultWon {
			winner = s
		}
	}
	fmt.Printf("== TERMINAL @ tick %d ==\n", resolvedAt)
	fmt.Printf("winner=slot %d  duration=%d ticks  results=[%s]\n", winner, resolvedAt, resultLine(g, slots))
	fmt.Printf("final StateHash = %#016x\n", g.StateHash())

	// FSV artifacts.
	if *dumpPath != "" {
		// Read-only dump must not perturb the hash: prove it.
		pre := g.StateHash()
		f, err := os.Create(*dumpPath)
		if err != nil {
			fatalf("create dump %q: %v", *dumpPath, err)
		}
		g.DumpState(f)
		if err := f.Close(); err != nil {
			fatalf("write dump %q: %v", *dumpPath, err)
		}
		if post := g.StateHash(); post != pre {
			fatalf("DumpState perturbed sim state: hash %#x -> %#x", pre, post)
		}
		fmt.Printf("dump written: %s (hash unchanged %#016x)\n", *dumpPath, pre)
	}
	if *replayPath != "" {
		rp := g.BuildReplay()
		if err := writeReplay(*replayPath, rp); err != nil {
			fatalf("write replay %q: %v", *replayPath, err)
		}
		fmt.Printf("replay written: %s (version=%d ticks=%d commands=%d)\n", *replayPath, rp.Version, rp.Ticks, len(rp.Commands))
	}

	if winner < 0 {
		fatalf("terminal reached but no slot Won (draw?) — results=[%s]", resultLine(g, slots))
	}
}

// participatingSlots returns the sorted slots that own at least one unit.
func participatingSlots(g *api.Game) []int {
	var out []int
	for s := 0; s < sim.MaxPlayers; s++ {
		if len(g.AllUnits(ownerFilter(s))) > 0 {
			out = append(out, s)
		}
	}
	return out
}

func ownerFilter(slot int) api.UnitFilter {
	return func(v api.UnitView) bool { return v.OwnerPlayer() == slot }
}

func printRoster(g *api.Game, slot int) {
	p := g.Player(slot)
	units := len(g.AllUnits(ownerFilter(slot)))
	heroLvl := 0
	for _, u := range g.AllUnits(ownerFilter(slot)) {
		if u.IsHero() {
			heroLvl = u.HeroLevel()
			break
		}
	}
	fmt.Printf("  slot %d: units=%d gold=%d AI=%v heroLvl=%d\n", slot, units, p.Gold(), g.IsAIPlayer(p), heroLvl)
}

func anyTerminal(g *api.Game, slots []int) bool {
	for _, s := range slots {
		if g.Player(s).Result() != api.ResultPlaying {
			return true
		}
	}
	return false
}

func writeReplay(path string, rp *sim.Replay) error {
	f, err := os.Create(path)
	if err != nil {
		return err
	}
	bw := bufio.NewWriter(f)
	if err := rp.Encode(bw); err != nil {
		f.Close()
		return err
	}
	if err := bw.Flush(); err != nil {
		f.Close()
		return err
	}
	return f.Close()
}

func resultLine(g *api.Game, slots []int) string {
	s := ""
	for i, slot := range slots {
		if i > 0 {
			s += " "
		}
		s += fmt.Sprintf("p%d=%d", slot, int(g.Player(slot).Result()))
	}
	return s
}

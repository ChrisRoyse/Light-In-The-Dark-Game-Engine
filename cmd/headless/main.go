// Command headless runs the production sim.Step() loop as fast as
// possible with no GPU, no window, and no G3N import (R-SIM-4,
// tick-and-scheduler.md §5). It is the SAME World and the SAME Step
// as the windowed path — there is no headless variant of any gameplay
// code, only a different driver around it.
//
// Builds with CGO_ENABLED=0; that build succeeding is the proof that
// litd/sim has no transitive cgo/GL dependency.
package main

import (
	"bufio"
	"flag"
	"fmt"
	"os"
	"time"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/prng"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/statehash"
)

func main() {
	mapPath := flag.String("map", "", "map file (map loading lands with M5; only the empty built-in world exists today)")
	seed := flag.Uint64("seed", 1, "world seed: places the built-in unit layout")
	ticks := flag.Int("ticks", 10000, "number of sim ticks to run")
	cmdsPath := flag.String("cmds", "", "command stream file: lines of 'tick kind unitIdx x y'")
	units := flag.Int("units", 256, "units to spawn in the built-in layout")
	dumpPath := flag.String("dump", "", "write the full state-dump JSON here at run end (R-FSV-2)")
	eventLogPath := flag.String("eventlog", "", "stream the structured event log here as JSONL (R-FSV-3)")
	flag.Parse()

	// Fail closed: no map format exists yet, so any -map value would
	// silently mean "ignored". Refuse instead.
	if *mapPath != "" {
		fmt.Fprintf(os.Stderr, "error: -map is not implemented yet (M5); refusing to silently ignore %q\n", *mapPath)
		os.Exit(1)
	}
	if *ticks < 0 {
		fmt.Fprintln(os.Stderr, "error: -ticks must be >= 0")
		os.Exit(1)
	}

	w := sim.NewWorld(sim.Caps{})

	var evw *bufio.Writer
	if *eventLogPath != "" {
		f, err := os.Create(*eventLogPath)
		if err != nil {
			fmt.Fprintf(os.Stderr, "error: %v\n", err)
			os.Exit(1)
		}
		defer f.Close()
		evw = bufio.NewWriter(f)
		w.AttachEventLog(evw)
	}

	// Built-in layout: -seed places -units units deterministically on a
	// 512x512 board through a seeded PRNG (R-SIM-2). The seed fully
	// defines the spawn, so different seeds hash differently. Units are
	// full combatants — alternating teams, health, movement, a melee
	// weapon, acquisition — so the run produces real orders, damage,
	// and death events for the R-FSV-3 log.
	w.SetSeed(*seed)
	if err := w.BindDamageMatrix([][]int32{{1000}}); err != nil {
		fmt.Fprintf(os.Stderr, "error: %v\n", err)
		os.Exit(1)
	}
	weapon := data.Attack{
		AttackType:       0,
		Range:            fixed.FromInt(8),
		DamageBase:       5,
		Dice:             1,
		Sides:            4,
		CooldownTicks:    27,
		DamagePointTicks: 10,
		BackswingTicks:   10,
	}
	rng := prng.New(*seed, 0)
	ids := make([]sim.EntityID, 0, *units)
	for i := 0; i < *units; i++ {
		pos := fixed.Vec2{
			X: fixed.FromInt(int32(rng.Uint32() % 512)),
			Y: fixed.FromInt(int32(rng.Uint32() % 512)),
		}
		facing := fixed.Angle(rng.Uint32() % 65536)
		id, ok := w.CreateUnit(pos, facing)
		if !ok {
			fmt.Fprintf(os.Stderr, "error: unit cap reached at %d/%d\n", i, *units)
			os.Exit(1)
		}
		team := uint8(i % 2)
		if !w.Owners.Add(w.Ents, id, team, team, team) ||
			!w.Healths.Add(w.Ents, id, 100*fixed.One, 0, 0, 0) ||
			!w.Combats.Add(w.Ents, id) ||
			!w.Orders.Add(w.Ents, id) ||
			!w.Movements.Add(w.Ents, w.Transforms, id, fixed.One*7/2, 2048) {
			fmt.Fprintf(os.Stderr, "error: component add failed for unit %d\n", i)
			os.Exit(1)
		}
		if !w.SetWeapon(id, 0, &weapon, 0, data.EffectList{}) {
			fmt.Fprintf(os.Stderr, "error: weapon set failed for unit %d\n", i)
			os.Exit(1)
		}
		w.Combats.AcquisitionRange[w.Combats.Row(id)] = fixed.FromInt(24)
		ids = append(ids, id)
	}

	cmds, err := loadCommands(*cmdsPath, ids)
	if err != nil {
		fmt.Fprintf(os.Stderr, "error: %v\n", err)
		os.Exit(1)
	}

	next := 0
	start := time.Now()
	for t := 1; t <= *ticks; t++ {
		// Commands for tick t issue as orders at the tick boundary —
		// the same deterministic driver position as a script. kind 0 =
		// move, anything else fails closed.
		for next < len(cmds) && cmds[next].tick == uint32(t) {
			c := &cmds[next]
			if c.cmd.Kind != 0 {
				fmt.Fprintf(os.Stderr, "error: unknown command kind %d at tick %d\n", c.cmd.Kind, t)
				os.Exit(1)
			}
			if w.Ents.Alive(c.cmd.Unit) {
				w.IssueOrder(c.cmd.Unit, sim.Order{Kind: sim.OrderMove, Point: c.cmd.Point}, false)
			}
			next++
		}
		w.Step()
	}
	elapsed := time.Since(start)

	reg := sim.NewHashRegistry()
	var snap statehash.Snapshot
	w.HashState(reg, &snap)
	fmt.Printf("hash: %016x\n", snap.Top)
	for i, name := range sim.HashSystems {
		fmt.Printf("sub: %-10s %016x\n", name, snap.Subs[i])
	}
	if evw != nil {
		if err := evw.Flush(); err != nil {
			fmt.Fprintf(os.Stderr, "error: event log flush: %v\n", err)
			os.Exit(1)
		}
		if err := w.EventLogErr(); err != nil {
			fmt.Fprintf(os.Stderr, "error: event log write: %v\n", err)
			os.Exit(1)
		}
	}
	if *dumpPath != "" {
		f, err := os.Create(*dumpPath)
		if err != nil {
			fmt.Fprintf(os.Stderr, "error: %v\n", err)
			os.Exit(1)
		}
		if err := w.DumpState(f); err != nil {
			fmt.Fprintf(os.Stderr, "error: state dump: %v\n", err)
			os.Exit(1)
		}
		if err := f.Close(); err != nil {
			fmt.Fprintf(os.Stderr, "error: %v\n", err)
			os.Exit(1)
		}
		// read-only proof, printed every run: the dump cannot have
		// mutated state if the hash recomputes identically
		var after statehash.Snapshot
		w.HashState(reg, &after)
		fmt.Printf("hash-after-dump: %016x (read-only: %v)\n", after.Top, after.Top == snap.Top)
	}
	fmt.Printf("ticks: %d\n", w.Tick())
	fmt.Printf("units: %d\n", w.UnitCount())
	if *ticks > 0 {
		fmt.Printf("elapsed: %s\n", elapsed.Round(time.Microsecond))
		fmt.Printf("ticks/sec: %.0f\n", float64(*ticks)/elapsed.Seconds())
	}
}

type timedCommand struct {
	tick uint32
	cmd  sim.WorldCommand
}

// loadCommands parses the -cmds stream: whitespace-separated lines of
// `tick kind unitIdx x y` (unitIdx indexes the built-in spawn order).
// Malformed lines are errors, not skips (fail closed).
func loadCommands(path string, ids []sim.EntityID) ([]timedCommand, error) {
	if path == "" {
		return nil, nil
	}
	f, err := os.Open(path)
	if err != nil {
		return nil, err
	}
	defer f.Close()

	var out []timedCommand
	sc := bufio.NewScanner(f)
	line := 0
	lastTick := uint32(0)
	for sc.Scan() {
		line++
		text := sc.Text()
		if len(text) == 0 || text[0] == '#' {
			continue
		}
		var tick uint32
		var kind uint8
		var unitIdx int
		var x, y int32
		if _, err := fmt.Sscan(text, &tick, &kind, &unitIdx, &x, &y); err != nil {
			return nil, fmt.Errorf("%s:%d: bad command line %q: %v", path, line, text, err)
		}
		if tick < lastTick {
			return nil, fmt.Errorf("%s:%d: ticks must be non-decreasing (%d after %d)", path, line, tick, lastTick)
		}
		lastTick = tick
		if unitIdx < 0 || unitIdx >= len(ids) {
			return nil, fmt.Errorf("%s:%d: unit index %d out of range [0,%d)", path, line, unitIdx, len(ids))
		}
		out = append(out, timedCommand{
			tick: tick,
			cmd: sim.WorldCommand{
				Kind:  kind,
				Unit:  ids[unitIdx],
				Point: fixed.Vec2{X: fixed.FromInt(x), Y: fixed.FromInt(y)},
			},
		})
	}
	if err := sc.Err(); err != nil {
		return nil, err
	}
	return out, nil
}

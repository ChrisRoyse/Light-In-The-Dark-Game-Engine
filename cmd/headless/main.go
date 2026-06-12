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

	// Built-in layout: -seed places -units units deterministically on a
	// 512x512 board through the sim's seeded PRNG (R-SIM-2). The seed
	// fully defines the spawn, so different seeds hash differently.
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
		// Commands for tick t enter staging now and apply in t's
		// phase 1 — the same enqueue-then-Step contract as any driver.
		for next < len(cmds) && cmds[next].tick == uint32(t) {
			if !w.EnqueueCommand(cmds[next].cmd) {
				fmt.Fprintf(os.Stderr, "error: command staging full at tick %d\n", t)
				os.Exit(1)
			}
			next++
		}
		w.Step()
	}
	elapsed := time.Since(start)

	snap := hashWorld(w)
	fmt.Printf("hash: %016x\n", snap.Top)
	for i, name := range []string{"entities", "transforms", "sched"} {
		fmt.Printf("sub: %-10s %016x\n", name, snap.Subs[i])
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

// hashWorld hashes the full observable sim state: the entity table,
// every transform row in store order (deterministic — SoA row order is
// part of sim state), and the scheduler's canonical save blob.
func hashWorld(w *sim.World) *statehash.Snapshot {
	reg := statehash.NewRegistry()
	hEnts := reg.Register("entities")
	hTrans := reg.Register("transforms")
	hSched := reg.Register("sched")

	hEnts.WriteU32(w.Tick())
	hEnts.WriteU32(uint32(w.UnitCount()))

	n := w.Transforms.Count()
	hTrans.WriteU32(uint32(n))
	for i := int32(0); i < n; i++ {
		hTrans.WriteU32(uint32(w.Transforms.Entity[i]))
		hTrans.WriteI64(int64(w.Transforms.Pos[i].X))
		hTrans.WriteI64(int64(w.Transforms.Pos[i].Y))
		hTrans.WriteU16(uint16(w.Transforms.Facing[i]))
	}

	hSched.WriteBytes(w.Sched.Save(nil))

	return reg.Sum(&statehash.Snapshot{})
}

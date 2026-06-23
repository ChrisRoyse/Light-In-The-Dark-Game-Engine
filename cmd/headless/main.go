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
	"bytes"
	"flag"
	"fmt"
	"os"
	"strconv"
	"strings"
	"time"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/buildinfo"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/obs"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/prng"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/statehash"
)

func fatalf(format string, a ...any) {
	fmt.Fprintf(os.Stderr, "error: "+format+"\n", a...)
	os.Exit(1)
}

func main() {
	mapPath := flag.String("map", "", "map file (map loading lands with M5; only the empty built-in world exists today)")
	seed := flag.Uint64("seed", 1, "world seed: places the built-in unit layout")
	ticks := flag.Int("ticks", 10000, "number of sim ticks to run")
	cmdsPath := flag.String("cmds", "", "command stream file: lines of 'tick kind unitIdx x y [target] [data]' (kind: 0=move 1=stop 2=hold 3=patrol 4=attack 5=harvest 6=follow 7=build)")
	units := flag.Int("units", 256, "units to spawn in the built-in layout")
	dumpPath := flag.String("dump", "", "write the full state-dump JSON here at run end (R-FSV-2)")
	eventLogPath := flag.String("eventlog", "", "stream the structured event log here as JSONL (R-FSV-3)")
	replayPath := flag.String("replay", "", "record this run to a .litdreplay file (header + commands + hash checkpoints)")
	verifyPath := flag.String("verify", "", "verify a .litdreplay: re-run its inputs, compare the full checkpoint trace")
	crashTest := flag.Int("crash-test", 0, "dev-only: panic right after this tick to exercise the crash reporter (#185)")
	crashDir := flag.String("crash-dir", "", "crash-report directory (default: user config dir/litd)")
	crashCapture := flag.Bool("crash-capture", true, "write a local crash dump on panic; false = stderr stack only, no file (#185)")
	reportDir := flag.String("report-on-exit", "", "write a debug-report bundle (litd-report-*.zip) to this dir at run end (#250)")
	showVersion := flag.Bool("version", false, "print version, commit, and build date, then exit (#184)")
	flag.Parse()

	if *showVersion {
		fmt.Println(buildinfo.Get())
		return
	}

	// Fail closed: no map format exists yet, so any -map value would
	// silently mean "ignored". Refuse instead.
	if *mapPath != "" {
		fatalf("-map is not implemented yet (M5); refusing to silently ignore %q", *mapPath)
	}
	if *verifyPath != "" {
		verifyReplay(*verifyPath)
		return
	}
	if *ticks < 0 {
		fatalf("-ticks must be >= 0")
	}

	w, ids := buildWorld(*seed, *units)

	// Crash-report capture (#185): install the recover hook before any work
	// runs so a panic anywhere below is captured to a local dump and the
	// process still dies nonzero. The reporter reads the live sim tick and a
	// small log ring at crash time. defer-LIFO means the file-close defers
	// below flush during unwind, then Recover writes the dump and exits.
	var clog *obs.Logger
	if *crashTest > 0 {
		clog = obs.New(1024)
	}
	reporter := obs.NewReporter(*crashDir, *crashCapture)
	reporter.Log = clog
	reporter.Tick = w.Tick
	defer reporter.Recover()

	var evw *bufio.Writer
	if *eventLogPath != "" {
		f, err := os.Create(*eventLogPath)
		if err != nil {
			fatalf("%v", err)
		}
		defer f.Close()
		evw = bufio.NewWriter(f)
		w.AttachEventLog(evw)
	}

	cmds, err := loadCommands(*cmdsPath, len(ids))
	if err != nil {
		fatalf("%v", err)
	}

	rec := &sim.Replay{
		Version:  sim.ReplayFormatVersion,
		Seed:     *seed,
		Roster:   uint32(*units),
		Interval: sim.DefaultCheckpointInterval,
		Ticks:    uint32(*ticks),
		Commands: cmds,
	}
	// The report bundle needs checkpoints in its replay so the extracted
	// replay is -verify-able, so -report-on-exit forces trace capture too.
	wantTrace := *replayPath != "" || *reportDir != ""

	start := time.Now()
	trace := runWorld(w, ids, cmds, *ticks, wantTrace, *crashTest, clog)
	elapsed := time.Since(start)

	reg := sim.NewHashRegistry()
	var snap statehash.Snapshot
	w.HashState(reg, &snap)
	var shb strings.Builder
	fmt.Fprintf(&shb, "hash: %016x\n", snap.Top)
	for i, name := range sim.HashSystems {
		fmt.Fprintf(&shb, "sub: %-10s %016x\n", name, snap.Subs[i])
	}
	fmt.Print(shb.String())
	if evw != nil {
		if err := evw.Flush(); err != nil {
			fatalf("event log flush: %v", err)
		}
		if err := w.EventLogErr(); err != nil {
			fatalf("event log write: %v", err)
		}
	}
	if *replayPath != "" {
		rec.Checkpoints = trace
		f, err := os.Create(*replayPath)
		if err != nil {
			fatalf("%v", err)
		}
		bw := bufio.NewWriter(f)
		if err := rec.Encode(bw); err != nil {
			fatalf("replay encode: %v", err)
		}
		if err := bw.Flush(); err != nil {
			fatalf("%v", err)
		}
		if err := f.Close(); err != nil {
			fatalf("%v", err)
		}
		fmt.Printf("replay: %s (%d commands, %d checkpoints)\n", *replayPath, len(rec.Commands), len(rec.Checkpoints))
	}
	if *dumpPath != "" {
		f, err := os.Create(*dumpPath)
		if err != nil {
			fatalf("%v", err)
		}
		if err := w.DumpState(f); err != nil {
			fatalf("state dump: %v", err)
		}
		if err := f.Close(); err != nil {
			fatalf("%v", err)
		}
		// read-only proof, printed every run: the dump cannot have
		// mutated state if the hash recomputes identically
		var after statehash.Snapshot
		w.HashState(reg, &after)
		fmt.Printf("hash-after-dump: %016x (read-only: %v)\n", after.Top, after.Top == snap.Top)
	}
	if *reportDir != "" {
		rec.Checkpoints = trace
		var replayBuf, stateBuf bytes.Buffer
		if err := rec.Encode(&replayBuf); err != nil {
			fatalf("report replay encode: %v", err)
		}
		if err := w.DumpState(&stateBuf); err != nil {
			fatalf("report state dump: %v", err)
		}
		in := obs.ReportInputs{
			StateDumpJSON: stateBuf.Bytes(),
			StateHash:     shb.String(),
			Replay:        replayBuf.Bytes(),
		}
		path, warnings, err := obs.WriteReport(*reportDir, in, time.Now)
		if err != nil {
			fatalf("report bundle: %v", err)
		}
		fmt.Printf("report: %s\n", path)
		for _, wmsg := range warnings {
			fmt.Printf("report WARNING: %s\n", wmsg)
		}
	}
	fmt.Printf("ticks: %d\n", w.Tick())
	fmt.Printf("units: %d\n", w.UnitCount())
	if *ticks > 0 {
		fmt.Printf("elapsed: %s\n", elapsed.Round(time.Microsecond))
		fmt.Printf("ticks/sec: %.0f\n", float64(*ticks)/elapsed.Seconds())
	}
}

// buildWorld spawns the built-in layout: -seed places n units
// deterministically on a 512x512 board through a seeded PRNG
// (R-SIM-2). Units are full combatants — alternating teams, health,
// movement, a melee weapon, acquisition — so runs produce real
// order/damage/death traffic for the R-FSV-3 log and replays.
func buildWorld(seed uint64, n int) (*sim.World, []sim.EntityID) {
	w := sim.NewWorld(sim.Caps{})
	w.SetSeed(seed)
	if err := w.BindDamageMatrix([][]int32{{1000}}); err != nil {
		fatalf("%v", err)
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
	rng := prng.New(seed, 0)
	ids := make([]sim.EntityID, 0, n)
	for i := 0; i < n; i++ {
		pos := fixed.Vec2{
			X: fixed.FromInt(int32(rng.Uint32() % 512)),
			Y: fixed.FromInt(int32(rng.Uint32() % 512)),
		}
		facing := fixed.Angle(rng.Uint32() % 65536)
		id, ok := w.CreateUnit(pos, facing)
		if !ok {
			fatalf("unit cap reached at %d/%d", i, n)
		}
		team := uint8(i % 2)
		if !w.Owners.Add(w.Ents, id, team, team, team) ||
			!w.Healths.Add(w.Ents, id, 100*fixed.One, 0, 0, 0) ||
			!w.Combats.Add(w.Ents, id) ||
			!w.Orders.Add(w.Ents, id) ||
			!w.Movements.Add(w.Ents, w.Transforms, id, fixed.One*7/2, 2048) {
			fatalf("component add failed for unit %d", i)
		}
		if !w.SetWeapon(id, 0, &weapon, 0, data.EffectList{}) {
			fatalf("weapon set failed for unit %d", i)
		}
		w.Combats.AcquisitionRange[w.Combats.Row(id)] = fixed.FromInt(24)
		ids = append(ids, id)
	}
	return w, ids
}

// runWorld drives the tick loop, issuing commands at their tick
// boundary (the same deterministic driver position as a script; kind
// 0 = move, anything else fails closed) and capturing the checkpoint
// trace when asked.
// applyReplayCommand issues one replay command's order to the world, mapping it
// through sim.ReplayCommand.Apply. Shared by runWorld + verifyReplay so the
// record and verify paths apply a command identically — a divergence there
// would be a self-inflicted desync, not a real one. Apply skips a command whose
// orderer is dead/out-of-range, or a target-taking order whose target cannot be
// resolved (matching the prior move-only dead-orderer skip), and dispatches
// player-level economy kinds (#404) directly to the world.
func applyReplayCommand(w *sim.World, ids []sim.EntityID, c *sim.ReplayCommand) {
	c.Apply(w, func(idx uint32) (sim.EntityID, bool) {
		if int(idx) < len(ids) && w.Ents.Alive(ids[idx]) {
			return ids[idx], true
		}
		return 0, false
	})
}

func runWorld(w *sim.World, ids []sim.EntityID, cmds []sim.ReplayCommand, ticks int, trace bool, crashTest int, clog *obs.Logger) []sim.ReplayCheckpoint {
	var cps []sim.ReplayCheckpoint
	var reg *statehash.Registry
	var snap statehash.Snapshot
	if trace {
		cps = make([]sim.ReplayCheckpoint, 0, ticks/int(sim.DefaultCheckpointInterval)+1)
		reg = sim.NewHashRegistry()
	}
	var crashMsg obs.MsgID
	if clog != nil {
		crashMsg = clog.Register("crash-test loop at tick {0}")
	}
	next := 0
	for t := 1; t <= ticks; t++ {
		for next < len(cmds) && cmds[next].Tick == uint32(t) {
			applyReplayCommand(w, ids, &cmds[next])
			next++
		}
		w.Step()
		if clog != nil {
			clog.Log(w.Tick(), 0, obs.Info, obs.ChSimTick, crashMsg, int64(t), 0, 0, 0)
		}
		if crashTest > 0 && t == crashTest {
			// Induced crash for #185 FSV: panic after the tick has been
			// committed so w.Tick() reads the crash tick in the dump.
			panic(fmt.Sprintf("crash-test: induced panic at tick %d", t))
		}
		if trace && uint32(t)%sim.DefaultCheckpointInterval == 0 {
			w.HashState(reg, &snap)
			cps = append(cps, sim.CheckpointFrom(uint32(t), &snap))
		}
	}
	return cps
}

// verifyReplay re-runs a replay's inputs from its header and compares
// the FULL checkpoint trace, reporting the first divergent checkpoint
// with its tick and culprit sub-hash system. Exit 0 = verified.
func verifyReplay(path string) {
	f, err := os.Open(path)
	if err != nil {
		fatalf("%v", err)
	}
	defer f.Close()
	rep, err := sim.DecodeReplay(bufio.NewReader(f))
	if err != nil {
		fatalf("%v", err)
	}
	// header gate: this driver binds no data tables and no map, so any
	// nonzero hash means the replay came from a different content set
	if rep.Fingerprint != 0 {
		fatalf("replay data-table fingerprint %016x does not match this run's (none bound) — refusing before tick 0", rep.Fingerprint)
	}
	if rep.MapHash != 0 {
		fatalf("replay map hash %016x: no map format exists in this build — refusing before tick 0", rep.MapHash)
	}
	fmt.Printf("header: version=%d seed=%d roster=%d interval=%d ticks=%d commands=%d checkpoints=%d\n",
		rep.Version, rep.Seed, rep.Roster, rep.Interval, rep.Ticks, len(rep.Commands), len(rep.Checkpoints))

	w, ids := buildWorld(rep.Seed, int(rep.Roster))
	reg := sim.NewHashRegistry()
	var snap statehash.Snapshot
	next := 0
	cpi := 0
	diverged := false
	for t := uint32(1); t <= rep.Ticks; t++ {
		for next < len(rep.Commands) && rep.Commands[next].Tick == t {
			applyReplayCommand(w, ids, &rep.Commands[next])
			next++
		}
		w.Step()
		if t%rep.Interval == 0 && cpi < len(rep.Checkpoints) {
			cp := &rep.Checkpoints[cpi]
			if cp.Tick != t {
				fatalf("checkpoint %d recorded at tick %d, expected %d", cpi, cp.Tick, t)
			}
			w.HashState(reg, &snap)
			culprit, match := sim.CompareCheckpoint(cp, &snap)
			status := "match"
			if !match {
				status = "DIVERGED culprit=" + culprit
			}
			fmt.Printf("checkpoint t%-6d recorded=%016x computed=%016x %s\n", cp.Tick, cp.Top, snap.Top, status)
			if !match {
				diverged = true
				break
			}
			cpi++
		}
	}
	if diverged {
		fmt.Println("verify: FAILED")
		os.Exit(2)
	}
	if cpi != len(rep.Checkpoints) {
		fatalf("trace incomplete: compared %d of %d checkpoints", cpi, len(rep.Checkpoints))
	}
	fmt.Printf("verify: OK (%d checkpoints match)\n", cpi)
}

// loadCommands parses the -cmds stream: whitespace-separated lines of
// `tick kind unitIdx x y [target] [data]` (unitIdx indexes the built-in spawn
// order). kind is a sim.Replay* constant (0=move, 1=stop, 2=hold, 3=patrol,
// 4=attack, 5=harvest, 6=follow, 7=build). target is a roster index for
// target-taking orders (-1 or omitted = none); data is the build unit-type id.
// Malformed lines — bad fields, unknown kind, out-of-range index, a harvest/
// follow without a target — are errors, not skips (fail closed).
func loadCommands(path string, roster int) ([]sim.ReplayCommand, error) {
	if path == "" {
		return nil, nil
	}
	f, err := os.Open(path)
	if err != nil {
		return nil, err
	}
	defer f.Close()

	var out []sim.ReplayCommand
	sc := bufio.NewScanner(f)
	line := 0
	lastTick := uint32(0)
	for sc.Scan() {
		line++
		text := strings.TrimSpace(sc.Text())
		if text == "" || text[0] == '#' {
			continue
		}
		flds := strings.Fields(text)
		if len(flds) < 5 || len(flds) > 7 {
			return nil, fmt.Errorf("%s:%d: want 5-7 fields `tick kind unitIdx x y [target] [data]`, got %d: %q", path, line, len(flds), text)
		}
		nums := make([]int, len(flds))
		for i, s := range flds {
			n, err := strconv.Atoi(s)
			if err != nil {
				return nil, fmt.Errorf("%s:%d: field %d %q is not an integer: %v", path, line, i, s, err)
			}
			nums[i] = n
		}
		tickN, kindN, unitIdx, x, y := nums[0], nums[1], nums[2], nums[3], nums[4]
		if tickN < 0 {
			return nil, fmt.Errorf("%s:%d: negative tick %d", path, line, tickN)
		}
		tick := uint32(tickN)
		if tick < lastTick {
			return nil, fmt.Errorf("%s:%d: ticks must be non-decreasing (%d after %d)", path, line, tick, lastTick)
		}
		lastTick = tick
		if kindN < 0 || uint8(kindN) > sim.ReplayMaxKind {
			return nil, fmt.Errorf("%s:%d: unknown command kind %d (max %d)", path, line, kindN, sim.ReplayMaxKind)
		}
		kind := uint8(kindN)
		// Player-level AI economy kinds (train/harvest/place) carry no orderer unit
		// and are addressed by player — the unit-order text format cannot express
		// them. Refuse rather than mis-apply with a dummy unit index.
		if (&sim.ReplayCommand{Kind: kind}).IsPlayerCommand() {
			return nil, fmt.Errorf("%s:%d: kind %d is a player-level AI command not authorable in the unit-order text format", path, line, kind)
		}
		if unitIdx < 0 || unitIdx >= roster {
			return nil, fmt.Errorf("%s:%d: unit index %d out of range [0,%d)", path, line, unitIdx, roster)
		}
		target := sim.NoRosterRef
		if len(flds) >= 6 {
			if t := nums[5]; t >= 0 {
				if t >= roster {
					return nil, fmt.Errorf("%s:%d: target index %d out of range [0,%d)", path, line, t, roster)
				}
				target = uint32(t)
			}
		}
		if (kind == sim.ReplayHarvest || kind == sim.ReplayFollow) && target == sim.NoRosterRef {
			return nil, fmt.Errorf("%s:%d: kind %d requires a target unit index", path, line, kind)
		}
		var data uint16
		if len(flds) == 7 {
			if d := nums[6]; d < 0 || d > 0xFFFF {
				return nil, fmt.Errorf("%s:%d: data %d out of range [0,65535]", path, line, d)
			} else {
				data = uint16(d)
			}
		}
		out = append(out, sim.ReplayCommand{
			Tick:   tick,
			Kind:   kind,
			Unit:   uint32(unitIdx),
			Target: target,
			Data:   data,
			X:      int64(fixed.FromInt(int32(x))),
			Y:      int64(fixed.FromInt(int32(y))),
		})
	}
	if err := sc.Err(); err != nil {
		return nil, err
	}
	return out, nil
}

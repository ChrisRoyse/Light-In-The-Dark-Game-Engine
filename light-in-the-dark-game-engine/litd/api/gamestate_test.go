package litd

import (
	"bytes"
	"fmt"
	"strings"
	"testing"
	"time"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/driver"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
)

type ingestingStepper struct {
	w *sim.World
}

func (s *ingestingStepper) Step() {
	s.w.IngestStagedCommands()
	s.w.Step()
}

func newDriverGame(t *testing.T) (*sim.World, *Game, *driver.Loop) {
	t.Helper()
	w := sim.NewWorld(sim.Caps{Units: 16})
	l := driver.New(&ingestingStepper{w: w})
	return w, newGameWithDriver(w, l), l
}

func apiOrderUnit(t *testing.T, w *sim.World, g *Game, player uint8, pos Vec2) (Unit, sim.EntityID) {
	t.Helper()
	id, ok := w.CreateUnit(vec(pos), 0)
	if !ok ||
		!w.Owners.Add(w.Ents, id, player, player, player) ||
		!w.Orders.Add(w.Ents, id) ||
		!w.Movements.Add(w.Ents, w.Transforms, id, 10*fixed.One, 65535) {
		t.Fatalf("unit setup failed")
	}
	return Unit{id: id, g: g}, id
}

func stageMoveCommand(t *testing.T, w *sim.World, player uint8, seq uint16, id sim.EntityID, pt Vec2) {
	t.Helper()
	r := sim.CommandRecord{
		Version:   sim.CommandVersion,
		Player:    player,
		Seq:       seq,
		Opcode:    sim.OpMove,
		UnitCount: 1,
		Point:     vec(pt),
	}
	r.Units[0] = id
	if !w.StageCommand(r) {
		t.Fatalf("StageCommand(seq=%d) refused", seq)
	}
}

func orderHeadDump(w *sim.World, id sim.EntityID) string {
	r := w.Orders.Row(id)
	if r < 0 {
		return fmt.Sprintf("tick=%d missing-order-row applied=%d rejected=%d", w.Tick(), w.CmdApplied(), w.CmdRejected())
	}
	p := w.Orders.Point[r]
	return fmt.Sprintf("tick=%d kind=%d phase=%d queue=%d point=(%.1f,%.1f) applied=%d rejected=%d",
		w.Tick(), w.Orders.Kind[r], w.Orders.Phase[r], w.QueueDepth(id),
		toFloat(p.X), toFloat(p.Y), w.CmdApplied(), w.CmdRejected())
}

func TestGameStatePauseQueuesOrdersUntilResume(t *testing.T) {
	w, g, l := newDriverGame(t)
	_, id := apiOrderUnit(t, w, g, 0, Vec2{X: 64, Y: 64})
	var eventLog bytes.Buffer
	w.AttachEventLog(&eventLog)
	var commandLog []string
	w.OnCommandRecord = func(tick uint32, r *sim.CommandRecord, actors []sim.EntityID) {
		commandLog = append(commandLog, fmt.Sprintf("t%d p%d seq%d op%d actors=%d",
			tick, r.Player, r.Seq, r.Opcode, len(actors)))
	}

	before := orderHeadDump(w, id)
	beforeTotal := l.TotalSteps()
	beforePaused := l.Paused()
	beforeSpeed := l.Speed()
	g.Pause()
	stageMoveCommand(t, w, 0, 0, id, Vec2{X: 300, Y: 64})
	stageMoveCommand(t, w, 0, 1, id, Vec2{X: 400, Y: 64})
	pausedStepsA, _ := l.Frame(driver.TickDuration)
	pausedStepsB, _ := l.Frame(10 * driver.TickDuration)
	paused := orderHeadDump(w, id)
	pausedTick := w.Tick()
	pausedApplied := w.CmdApplied()
	pausedTotal := l.TotalSteps()
	pausedDriverPaused := l.Paused()
	pausedCommandLog := append([]string{}, commandLog...)
	pausedEventLog := eventLog.String()
	g.Resume()
	resumeSteps, resumeAlpha := l.Frame(driver.TickDuration)
	after := orderHeadDump(w, id)
	afterTotal := l.TotalSteps()
	afterPaused := l.Paused()
	afterEventLog := eventLog.String()

	t.Logf("FSV api pause/orders BEFORE: %s totalSteps=%d paused=%v speed=%.2f", before, beforeTotal, beforePaused, beforeSpeed)
	t.Logf("FSV api pause/orders PAUSED: %s frameSteps=%d,%d totalSteps=%d paused=%v commandLog=%v eventLog=%q",
		paused, pausedStepsA, pausedStepsB, pausedTotal, pausedDriverPaused, pausedCommandLog, pausedEventLog)
	t.Logf("FSV api pause/orders RESUMED: %s frameSteps=%d alpha=%.3f totalSteps=%d paused=%v commandLog=%v eventLog=%q",
		after, resumeSteps, resumeAlpha, afterTotal, afterPaused, commandLog, afterEventLog)

	if pausedStepsA != 0 || pausedStepsB != 0 || pausedTick != 0 || pausedApplied != 0 ||
		pausedTotal != 0 || !pausedDriverPaused || len(pausedCommandLog) != 0 || pausedEventLog != "" {
		t.Fatalf("pause did not freeze SoT: steps=%d,%d tick=%d applied=%d total=%d paused=%v commandLog=%v eventLog=%q",
			pausedStepsA, pausedStepsB, pausedTick, pausedApplied, pausedTotal, pausedDriverPaused, pausedCommandLog, pausedEventLog)
	}
	if !strings.Contains(paused, "tick=0 kind=0") {
		t.Fatalf("order head changed during pause: %s", paused)
	}
	if resumeSteps != 1 || afterTotal != 1 || w.Tick() != 1 || w.CmdApplied() != 2 {
		t.Fatalf("resume should advance/apply exactly one tick: frameSteps=%d totalSteps=%d tick=%d applied=%d",
			resumeSteps, afterTotal, w.Tick(), w.CmdApplied())
	}
	r := w.Orders.Row(id)
	if w.Orders.Kind[r] != sim.OrderMove || w.Orders.Point[r] != vec(Vec2{X: 400, Y: 64}) {
		t.Fatalf("resume did not apply final move order: %s", after)
	}
	if got, want := strings.Join(commandLog, "|"), "t1 p0 seq0 op0 actors=1|t1 p0 seq1 op0 actors=1"; got != want {
		t.Fatalf("command application order = %q, want %q", got, want)
	}
	if strings.Count(eventLog.String(), `"name":"order-issued"`) != 2 {
		t.Fatalf("event log should contain two order-issued records: %q", eventLog.String())
	}
}

func TestGameStateElapsedTimeExcludesPausedSpan(t *testing.T) {
	w, g, l := newDriverGame(t)
	l.Frame(5 * driver.TickDuration)
	beforePause := fmt.Sprintf("tick=%d elapsed=%.2f totalSteps=%d", w.Tick(), g.ElapsedTime(), l.TotalSteps())
	g.Pause()
	pausedSteps := 0
	for i := 0; i < 600; i++ {
		steps, _ := l.Frame(16600 * time.Microsecond)
		pausedSteps += steps
	}
	duringPause := fmt.Sprintf("tick=%d elapsed=%.2f totalSteps=%d", w.Tick(), g.ElapsedTime(), l.TotalSteps())
	g.Resume()
	resumeSteps, _ := l.Frame(2 * driver.TickDuration)
	afterResume := fmt.Sprintf("tick=%d elapsed=%.2f totalSteps=%d", w.Tick(), g.ElapsedTime(), l.TotalSteps())

	t.Logf("FSV api pause elapsed BEFORE: %s", beforePause)
	t.Logf("FSV api pause elapsed PAUSED: %s pausedSteps=%d", duringPause, pausedSteps)
	t.Logf("FSV api pause elapsed RESUMED: %s resumeSteps=%d", afterResume, resumeSteps)

	if beforePause != "tick=5 elapsed=0.25 totalSteps=5" {
		t.Fatalf("unexpected pre-pause state: %s", beforePause)
	}
	if duringPause != beforePause || pausedSteps != 0 {
		t.Fatalf("elapsed/ticks advanced during pause: before=%s paused=%s pausedSteps=%d", beforePause, duringPause, pausedSteps)
	}
	if afterResume != "tick=7 elapsed=0.35 totalSteps=7" || resumeSteps != 2 {
		t.Fatalf("resume elapsed wrong: after=%s resumeSteps=%d", afterResume, resumeSteps)
	}
}

func TestGameStateNilDriverReportsNoop(t *testing.T) {
	w := sim.NewWorld(sim.Caps{})
	g := newGame(w)
	g.SetDebug(true)
	var reports []string
	g.OnInvalidHandle(func(s string) { reports = append(reports, s) })

	before := fmt.Sprintf("tick=%d elapsed=%.2f", w.Tick(), g.ElapsedTime())
	g.Pause()
	g.Resume()
	g.SetSpeed(GameSpeedFast)
	after := fmt.Sprintf("tick=%d elapsed=%.2f", w.Tick(), g.ElapsedTime())

	t.Logf("FSV api nil-driver BEFORE: %s", before)
	t.Logf("FSV api nil-driver AFTER:  %s reports=%v", after, reports)

	if after != before {
		t.Fatalf("nil driver changed sim state: before=%s after=%s", before, after)
	}
	if len(reports) != 3 ||
		!strings.Contains(reports[0], "Game.Pause") ||
		!strings.Contains(reports[1], "Game.Resume") ||
		!strings.Contains(reports[2], "Game.SetSpeed") {
		t.Fatalf("nil-driver reports = %v, want Pause/Resume/SetSpeed", reports)
	}
}

func TestGameStateSetSpeedRejectsZero(t *testing.T) {
	_, g, l := newDriverGame(t)
	g.SetDebug(true)
	var reports []string
	g.OnInvalidHandle(func(s string) { reports = append(reports, s) })

	before := l.Speed()
	g.SetSpeed(GameSpeedFast)
	fast := l.Speed()
	g.SetSpeed(0)
	afterZero := l.Speed()

	t.Logf("FSV api speed BEFORE: %.2f", before)
	t.Logf("FSV api speed FAST:   %.2f", fast)
	t.Logf("FSV api speed ZERO:   %.2f reports=%v", afterZero, reports)

	if before != 1.0 || fast != float64(GameSpeedFast) {
		t.Fatalf("valid speed path wrong: before=%.2f fast=%.2f", before, fast)
	}
	if afterZero != fast {
		t.Fatalf("SetSpeed(0) changed driver speed: %.2f -> %.2f", fast, afterZero)
	}
	if len(reports) != 1 || !strings.Contains(reports[0], "Game.SetSpeed") {
		t.Fatalf("SetSpeed(0) report = %v, want Game.SetSpeed", reports)
	}
}

func TestGameStateMatchConfigGetters(t *testing.T) {
	g := newGame(sim.NewWorld(sim.Caps{}))
	g.SetDebug(true)
	var reports []string
	g.OnInvalidHandle(func(s string) { reports = append(reports, s) })
	g.match.flags = MapFlagFogAlwaysVisible | MapFlagLockSpeed | MapFlagRandomRaces
	g.match.teams = 2
	g.match.startLocations[0] = Vec2{X: 100, Y: 200}
	g.match.startLocations[1] = Vec2{X: 300, Y: 400}

	loc0 := g.StartLocation(0)
	loc1 := g.StartLocation(1)
	invalid := g.StartLocation(sim.MaxPlayers)
	t.Logf("FSV api match config flags: always=%v lockSpeed=%v observers=%v combo=%v",
		g.MapFlag(MapFlagFogAlwaysVisible), g.MapFlag(MapFlagLockSpeed),
		g.MapFlag(MapFlagObservers), g.MapFlag(MapFlagFogAlwaysVisible|MapFlagLockSpeed))
	t.Logf("FSV api match config starts: loc0=%+v loc1=%+v invalid=%+v reports=%v", loc0, loc1, invalid, reports)
	t.Logf("FSV api match config teams=%d replay=%v nilTeams=%d nilReplay=%v",
		g.Teams(), g.IsReplay(), (*Game)(nil).Teams(), (*Game)(nil).IsReplay())

	if !g.MapFlag(MapFlagFogAlwaysVisible) || !g.MapFlag(MapFlagLockSpeed) ||
		!g.MapFlag(MapFlagFogAlwaysVisible|MapFlagLockSpeed) || g.MapFlag(MapFlagObservers) {
		t.Fatal("MapFlag getter returned wrong values")
	}
	if loc0 != (Vec2{X: 100, Y: 200}) || loc1 != (Vec2{X: 300, Y: 400}) || !invalid.IsZero() {
		t.Fatalf("StartLocation getter wrong: loc0=%+v loc1=%+v invalid=%+v", loc0, loc1, invalid)
	}
	if g.Teams() != 2 || g.IsReplay() {
		t.Fatalf("Teams/IsReplay wrong: teams=%d replay=%v", g.Teams(), g.IsReplay())
	}
	if len(reports) != 1 || !strings.Contains(reports[0], "Game.StartLocation") {
		t.Fatalf("invalid StartLocation report = %v", reports)
	}
}

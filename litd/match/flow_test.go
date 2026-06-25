package match_test

// #201 match-flow FSV. SoT is independent of the thing under test:
//   - production: g.AllUnits() — the footman entities that physically exist,
//     counted directly, NOT read back from Flow.Stats (which would be circular).
//   - result: the public Player.Result() the flow consumes.
//   - terminal screen: the captured OnUIScreen event (the actual emitted chrome).
//   - duration: the number of play-phase Advance(1) ticks we issue, known a priori.
// X+X=Y: train 3 footmen + kill 1 => Stats{trained:3, lost:1}; a known play tick
// count => that exact DurationTicks. Headless — no GL.

import (
	"testing"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/asset/locale"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/match"
)

const footmanTrainTicks = 40

// matchGame builds a headless game with a footman/barracks roster, an economy,
// and a barracks owned by player 0 with gold to spare. Returns the game, the
// local player, the barracks unit, and the footman type — all via public api.
func matchGame(t *testing.T) (*api.Game, api.Player, api.Unit, api.UnitType) {
	t.Helper()
	g, err := api.NewGame(api.GameOptions{MaxUnits: 64, Seed: 7})
	if err != nil {
		t.Fatalf("NewGame: %v", err)
	}
	if err := g.DefineEconomy(2); err != nil {
		t.Fatalf("DefineEconomy: %v", err)
	}
	// footman MUST be index 0 so barracks.Trains{0} references it.
	if err := g.DefineUnits([]data.Unit{
		{ID: "footman", Life: 100, CollisionSize: 16, Costs: []int64{50, 0}, TrainTicks: footmanTrainTicks, FoodCost: 2},
		{ID: "barracks", Life: 1000, CollisionSize: 64, FoodProvided: 20, Trains: []uint16{0}},
	}); err != nil {
		t.Fatalf("DefineUnits: %v", err)
	}
	p0 := g.Player(0)
	p0.SetGold(1000)
	barracks := g.CreateUnit(p0, g.UnitType("barracks"), api.Vec2{X: 200, Y: 200}, api.Deg(0))
	if !barracks.Valid() {
		t.Fatal("barracks spawn failed")
	}
	return g, p0, barracks, g.UnitType("footman")
}

// countFootmen is the independent production SoT: footman entities owned by p
// that currently exist in the world.
func countFootmen(g *api.Game, p api.Player, footman api.UnitType) int {
	n := 0
	for _, u := range g.AllUnits(func(api.UnitView) bool { return true }) {
		if u.Type() == footman && u.Owner().Slot() == p.Slot() {
			n++
		}
	}
	return n
}

// advance ticks the sim n times, polling the flow each tick (as the game loop
// would), and returns how many play-phase ticks elapsed.
func advance(g *api.Game, f *match.Flow, n int) {
	for i := 0; i < n; i++ {
		g.Advance(1)
		f.Poll()
	}
}

func TestMatchFlowVictoryFSV(t *testing.T) {
	g, p0, barracks, footman := matchGame(t)
	flow := match.NewFlow(g, p0)

	var screens []api.UIScreenEvent
	g.OnUIScreen(func(e api.UIScreenEvent) { screens = append(screens, e) })

	// Phase walk: setup -> countdown -> play.
	if flow.Phase() != match.PhaseSetup {
		t.Fatalf("fresh flow phase=%s, want setup", flow.Phase())
	}
	if !flow.Begin(match.Setup{Faction: match.FactionVigil, Opponent: match.FactionUnbound}) || flow.Phase() != match.PhaseCountdown {
		t.Fatalf("Begin: phase=%s, want countdown", flow.Phase())
	}
	if !flow.StartPlay() || flow.Phase() != match.PhasePlay {
		t.Fatalf("StartPlay: phase=%s, want play", flow.Phase())
	}
	startTick := g.Tick()
	t.Logf("FSV play start: phase=%s tick=%d stats=%+v", flow.Phase(), startTick, flow.Stats())

	// Trigger: queue 3 footmen, then run the queue to completion. SoT BEFORE.
	if got := countFootmen(g, p0, footman); got != 0 {
		t.Fatalf("footmen before training = %d, want 0", got)
	}
	for i := 0; i < 3; i++ {
		if !barracks.Train(footman) {
			t.Fatalf("Train #%d refused with 1000 gold / 20 food", i)
		}
	}
	playTicks := 0
	// 3 sequential trains @40 ticks + slack; poll each tick.
	for i := 0; i < 3*footmanTrainTicks+20; i++ {
		g.Advance(1)
		flow.Poll()
		playTicks++
	}
	// SoT AFTER (independent): 3 footman entities exist; flow counted 3 trained.
	gotUnits := countFootmen(g, p0, footman)
	t.Logf("FSV after training: footmen(AllUnits)=%d flow.UnitsTrained=%d FoodUsed=%d", gotUnits, flow.Stats().UnitsTrained, p0.FoodUsed())
	if gotUnits != 3 {
		t.Fatalf("footmen produced = %d, want 3 (independent SoT)", gotUnits)
	}
	if flow.Stats().UnitsTrained != 3 {
		t.Fatalf("flow UnitsTrained = %d, want 3", flow.Stats().UnitsTrained)
	}

	// Trigger: kill exactly one footman. SoT: count drops to 2, flow lost==1.
	var victim api.Unit
	for _, u := range g.AllUnits(func(api.UnitView) bool { return true }) {
		if u.Type() == footman && u.Owner().Slot() == p0.Slot() {
			victim = u
			break
		}
	}
	victim.Kill()
	for i := 0; i < 5; i++ {
		g.Advance(1)
		flow.Poll()
		playTicks++
	}
	t.Logf("FSV after 1 kill: footmen=%d flow.UnitsLost=%d", countFootmen(g, p0, footman), flow.Stats().UnitsLost)
	if got := countFootmen(g, p0, footman); got != 2 {
		t.Fatalf("footmen after 1 kill = %d, want 2 (independent SoT)", got)
	}
	if flow.Stats().UnitsLost != 1 {
		t.Fatalf("flow UnitsLost = %d, want 1", flow.Stats().UnitsLost)
	}

	// Trigger the result; latch to terminal. Count the extra play ticks.
	g.Victory(p0)
	latched := false
	for i := 0; i < 10; i++ {
		g.Advance(1)
		playTicks++
		if flow.Poll() {
			latched = true
			break
		}
	}
	if !latched || flow.Phase() != match.PhaseTerminal {
		t.Fatalf("victory did not latch terminal: phase=%s", flow.Phase())
	}
	// Duration X=Y: flow froze g.Tick()-startTick; that equals every play tick we issued.
	wantDuration := int(g.Tick() - startTick)
	t.Logf("FSV terminal: result=%v duration=%d (issued play ticks=%d, tickDelta=%d)",
		flow.Result(), flow.Stats().DurationTicks, playTicks, wantDuration)
	if flow.Result() != api.ResultWon {
		t.Fatalf("result = %v, want Won", flow.Result())
	}
	if flow.Stats().DurationTicks != wantDuration || flow.Stats().DurationTicks != playTicks {
		t.Fatalf("duration = %d, want %d (=%d issued ticks)", flow.Stats().DurationTicks, wantDuration, playTicks)
	}

	// Terminal screen SoT: a Show event for "terminal" with the victory title key
	// and an exit button keyed to terminal.exit.
	if len(screens) == 0 {
		t.Fatal("no UIScreen event emitted on terminal")
	}
	last := screens[len(screens)-1]
	if last.Kind != api.UIScreenShow || last.Screen.ID != match.TerminalScreenID {
		t.Fatalf("terminal screen event = %+v, want Show/terminal", last)
	}
	if last.Screen.TitleKey != string(locale.TerminalVictory) {
		t.Fatalf("terminal title key = %q, want %q", last.Screen.TitleKey, locale.TerminalVictory)
	}
	if len(last.Screen.Buttons) != 1 || last.Screen.Buttons[0].LabelKey != string(locale.TerminalExit) {
		t.Fatalf("terminal exit button = %+v, want one keyed %q", last.Screen.Buttons, locale.TerminalExit)
	}

	// Exit tears down: phase->exit, a Hide for "terminal" emitted.
	if !flow.ExitToMenu() || flow.Phase() != match.PhaseExit {
		t.Fatalf("ExitToMenu: phase=%s, want exit", flow.Phase())
	}
	last = screens[len(screens)-1]
	if last.Kind != api.UIScreenHide || last.Screen.ID != match.TerminalScreenID {
		t.Fatalf("exit screen event = %+v, want Hide/terminal", last)
	}
	t.Logf("FSV victory path complete: phase=%s screens=%d", flow.Phase(), len(screens))
}

func TestMatchFlowDefeatFSV(t *testing.T) {
	g, p0, _, _ := matchGame(t)
	flow := match.NewFlow(g, p0)
	var screens []api.UIScreenEvent
	g.OnUIScreen(func(e api.UIScreenEvent) { screens = append(screens, e) })

	flow.Begin(match.Setup{})
	flow.StartPlay()
	startTick := g.Tick()
	advance(g, flow, 10) // 10 play ticks, no production
	g.Defeat(p0, "routed")
	latched := false
	for i := 0; i < 10; i++ {
		g.Advance(1)
		if flow.Poll() {
			latched = true
			break
		}
	}
	if !latched || flow.Phase() != match.PhaseTerminal {
		t.Fatalf("defeat did not latch terminal: phase=%s", flow.Phase())
	}
	t.Logf("FSV defeat: result=%v duration=%d", flow.Result(), flow.Stats().DurationTicks)
	if flow.Result() != api.ResultLost {
		t.Fatalf("result = %v, want Lost", flow.Result())
	}
	if flow.Stats().DurationTicks != int(g.Tick()-startTick) {
		t.Fatalf("duration = %d, want %d", flow.Stats().DurationTicks, int(g.Tick()-startTick))
	}
	last := screens[len(screens)-1]
	if last.Screen.TitleKey != string(locale.TerminalDefeat) {
		t.Fatalf("defeat title key = %q, want %q", last.Screen.TitleKey, locale.TerminalDefeat)
	}
}

func TestMatchFlowResetFSV(t *testing.T) {
	// Edge 1: a second match in the same process starts clean (no leaked stats,
	// result, or start tick). BEFORE/AFTER dumps prove the reset.
	g, p0, barracks, footman := matchGame(t)
	flow := match.NewFlow(g, p0)

	// Match A: train 2, win.
	flow.Begin(match.Setup{Faction: match.FactionVigil})
	flow.StartPlay()
	barracks.Train(footman)
	barracks.Train(footman)
	advance(g, flow, 2*footmanTrainTicks+10)
	g.Victory(p0)
	advance(g, flow, 10)
	aStats := flow.Stats()
	t.Logf("FSV match A terminal: phase=%s result=%v stats=%+v", flow.Phase(), flow.Result(), aStats)
	if flow.Phase() != match.PhaseTerminal || aStats.UnitsTrained != 2 {
		t.Fatalf("match A wrong: phase=%s stats=%+v", flow.Phase(), aStats)
	}
	flow.ExitToMenu()

	// Reset: dump BEFORE (match A's terminal state) and AFTER (zeroed).
	t.Logf("FSV pre-reset: phase=%s result=%v stats=%+v setup=%+v", flow.Phase(), flow.Result(), flow.Stats(), flow.Setup())
	flow.Reset()
	t.Logf("FSV post-reset: phase=%s result=%v stats=%+v setup=%+v", flow.Phase(), flow.Result(), flow.Stats(), flow.Setup())
	if flow.Phase() != match.PhaseSetup {
		t.Fatalf("post-reset phase=%s, want setup", flow.Phase())
	}
	if flow.Result() != api.ResultPlaying {
		t.Fatalf("post-reset result=%v, want Playing", flow.Result())
	}
	if (flow.Stats() != match.Stats{}) {
		t.Fatalf("post-reset stats=%+v, want zero", flow.Stats())
	}
	if (flow.Setup() != match.Setup{}) {
		t.Fatalf("post-reset setup=%+v, want zero", flow.Setup())
	}

	// Match B runs independently and counts only its own unit. A genuine second
	// match is a NEW game/World: match A ended with g.Victory(p0), which latches
	// p0=Won permanently in the SIM (flow.Reset clears only flow state, not the
	// sim's terminal result), so the same game cannot host a replay — a fresh
	// Poll would see Won and terminal instantly. Stats are now drained from the
	// non-hashing render-event snapshot only DURING PhasePlay (#665), so a fresh
	// match on a fresh sim is the correct way to prove no cross-match stat carry.
	gB, p0B, barracksB, footmanB := matchGame(t)
	flowB := match.NewFlow(gB, p0B)
	flowB.Begin(match.Setup{Faction: match.FactionUnbound})
	flowB.StartPlay()
	barracksB.Train(footmanB)
	advance(gB, flowB, footmanTrainTicks+10)
	if flowB.Stats().UnitsTrained != 1 {
		t.Fatalf("match B UnitsTrained = %d, want 1 (independent fresh match)", flowB.Stats().UnitsTrained)
	}
	t.Logf("FSV match B independent (fresh game): stats=%+v (A's reset flow zeroed at line above; no leak)", flowB.Stats())
}

func TestMatchFlowTeardownFSV(t *testing.T) {
	// Edge 2: quit mid-match (Reset during PhasePlay) stops stat accrual — no
	// counting after teardown. Stats are pull-drained per tick during PhasePlay
	// (#665); a torn-down flow returns to PhaseSetup and is no longer polled, so
	// it simply stops counting (there is no subscription that could orphan-count).
	g, p0, barracks, footman := matchGame(t)
	flow := match.NewFlow(g, p0)
	flow.Begin(match.Setup{})
	flow.StartPlay()

	barracks.Train(footman)
	advance(g, flow, footmanTrainTicks+10)
	trainedBefore := flow.Stats().UnitsTrained
	if trainedBefore != 1 {
		t.Fatalf("pre-teardown trained = %d, want 1", trainedBefore)
	}

	// Quit mid-match: Reset returns to setup and zeroes stats.
	flow.Reset()
	if flow.Phase() != match.PhaseSetup {
		t.Fatalf("post-quit phase=%s, want setup", flow.Phase())
	}
	if flow.Stats().UnitsTrained != 0 {
		t.Fatalf("post-reset stats not zeroed: %+v", flow.Stats())
	}

	// SoT: produce more units AFTER teardown (flow stays in setup, not polled).
	// A torn-down flow must not count: with no per-tick Poll there is no drain,
	// so UnitsTrained stays 0 even though units really spawn.
	before := countFootmen(g, p0, footman)
	barracks.Train(footman)
	barracks.Train(footman)
	for i := 0; i < 2*footmanTrainTicks+20; i++ {
		g.Advance(1) // flow is torn down — deliberately not polled
	}
	produced := countFootmen(g, p0, footman) - before
	t.Logf("FSV teardown: produced %d more footmen after Reset; flow.UnitsTrained=%d (want 0 — not polled, no drain)", produced, flow.Stats().UnitsTrained)
	if produced < 1 {
		t.Fatalf("no production after teardown — test is vacuous, got delta %d", produced)
	}
	if flow.Stats().UnitsTrained != 0 {
		t.Fatalf("orphan subscription counted %d trains after teardown", flow.Stats().UnitsTrained)
	}
}

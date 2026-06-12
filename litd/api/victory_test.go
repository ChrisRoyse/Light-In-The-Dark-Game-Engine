package litd

import (
	"bytes"
	"fmt"
	"log"
	"strings"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
)

func apiPlayer(g *Game, idx int32) Player {
	return Player{idx: idx, g: g}
}

func apiResults(g *Game) [sim.MaxPlayers]uint8 {
	var out [sim.MaxPlayers]uint8
	if g == nil || g.w == nil {
		return out
	}
	for p := uint8(0); p < sim.MaxPlayers; p++ {
		out[p] = g.w.PlayerResult(p)
	}
	return out
}

func apiResultDump(g *Game) string {
	if g == nil || g.w == nil {
		return "nil-game"
	}
	return fmt.Sprintf("tick=%d results=%v", g.w.Tick(), apiResults(g))
}

func TestVictoryEventFiresOnceAndResultReadable(t *testing.T) {
	if EventVictory != 12 || EventDefeat != 13 {
		t.Fatalf("public event ABI changed: victory=%d defeat=%d want 12/13", EventVictory, EventDefeat)
	}

	w := sim.NewWorld(sim.Caps{})
	g := newGame(w)
	p := apiPlayer(g, 2)
	var eventLog bytes.Buffer
	w.AttachEventLog(&eventLog)

	var trace []string
	g.OnVictory(func(e Event) {
		ep := e.Player()
		trace = append(trace, fmt.Sprintf("kind=%d player=%d valid=%v handlerResult=%d sot=%d",
			e.Kind(), ep.idx, ep.Valid(), ep.Result(), g.w.PlayerResult(uint8(ep.idx))))
	})

	before := apiResultDump(g)
	g.Victory(p)
	g.Victory(p)
	staged := apiResultDump(g)
	w.Step()
	after := apiResultDump(g)
	w.Step()
	quiet := apiResultDump(g)

	t.Logf("FSV api victory BEFORE: %s", before)
	t.Logf("FSV api victory STAGED: %s", staged)
	t.Logf("FSV api victory AFTER:  %s trace=%v eventLog=%q", after, trace, eventLog.String())
	t.Logf("FSV api victory QUIET:  %s trace=%v eventLog=%q", quiet, trace, eventLog.String())

	if got := len(trace); got != 1 {
		t.Fatalf("OnVictory fired %d times, want exactly one trace=%v", got, trace)
	}
	if want := "kind=12 player=2 valid=true handlerResult=1 sot=1"; trace[0] != want {
		t.Fatalf("victory handler trace = %q, want %q", trace[0], want)
	}
	if got := p.Result(); got != ResultWon {
		t.Fatalf("Player.Result after victory = %d, want %d", got, ResultWon)
	}
	if got := g.w.PlayerResult(2); got != sim.ResultWon {
		t.Fatalf("sim PlayerResult(2) = %d, want %d", got, sim.ResultWon)
	}
	if strings.Count(eventLog.String(), `"name":"victory"`) != 1 || !strings.Contains(eventLog.String(), `"arg":2`) {
		t.Fatalf("event log does not contain exactly one victory for player 2: %q", eventLog.String())
	}
}

func TestDefeatEventAndMessageLog(t *testing.T) {
	w := sim.NewWorld(sim.Caps{})
	g := newGame(w)
	p := apiPlayer(g, 5)
	var eventLog bytes.Buffer
	w.AttachEventLog(&eventLog)

	oldWriter := log.Writer()
	oldFlags := log.Flags()
	oldPrefix := log.Prefix()
	var processLog bytes.Buffer
	log.SetOutput(&processLog)
	log.SetFlags(0)
	log.SetPrefix("")
	defer func() {
		log.SetOutput(oldWriter)
		log.SetFlags(oldFlags)
		log.SetPrefix(oldPrefix)
	}()

	var trace []string
	g.OnDefeat(func(e Event) {
		ep := e.Player()
		trace = append(trace, fmt.Sprintf("kind=%d player=%d result=%d sot=%d",
			e.Kind(), ep.idx, ep.Result(), g.w.PlayerResult(uint8(ep.idx))))
	})

	before := apiResultDump(g)
	g.Defeat(p, "synthetic defeat reason")
	staged := apiResultDump(g)
	w.Step()
	after := apiResultDump(g)
	msg := strings.TrimSpace(processLog.String())

	t.Logf("FSV api defeat BEFORE: %s", before)
	t.Logf("FSV api defeat STAGED: %s", staged)
	t.Logf("FSV api defeat AFTER:  %s trace=%v eventLog=%q processLog=%q", after, trace, eventLog.String(), msg)

	if got := len(trace); got != 1 {
		t.Fatalf("OnDefeat fired %d times, want exactly one trace=%v", got, trace)
	}
	if want := "kind=13 player=5 result=2 sot=2"; trace[0] != want {
		t.Fatalf("defeat handler trace = %q, want %q", trace[0], want)
	}
	if got := p.Result(); got != ResultLost {
		t.Fatalf("Player.Result after defeat = %d, want %d", got, ResultLost)
	}
	if !strings.Contains(msg, `player=5`) || !strings.Contains(msg, `synthetic defeat reason`) {
		t.Fatalf("defeat reason was not routed to process log: %q", msg)
	}
	if strings.Contains(eventLog.String(), "synthetic defeat reason") {
		t.Fatalf("presentation defeat message leaked into sim event log: %q", eventLog.String())
	}
	if strings.Count(eventLog.String(), `"name":"defeat"`) != 1 || !strings.Contains(eventLog.String(), `"arg":5`) {
		t.Fatalf("event log does not contain exactly one defeat for player 5: %q", eventLog.String())
	}
}

func TestVictoryZeroValuePlayerNoopReports(t *testing.T) {
	w := sim.NewWorld(sim.Caps{})
	g := newGame(w)
	g.SetDebug(true)
	var reports []string
	g.OnInvalidHandle(func(s string) { reports = append(reports, s) })
	var eventLog bytes.Buffer
	w.AttachEventLog(&eventLog)

	beforeDump := apiResultDump(g)
	beforeResults := apiResults(g)
	g.Victory(Player{})
	g.Defeat(Player{}, "must not log")
	w.Step()
	afterDump := apiResultDump(g)
	afterResults := apiResults(g)

	t.Logf("FSV api victory zero-player BEFORE: %s", beforeDump)
	t.Logf("FSV api victory zero-player AFTER:  %s reports=%v eventLog=%q", afterDump, reports, eventLog.String())

	if beforeResults != afterResults {
		t.Fatalf("zero-value Player changed sim results: before=%v after=%v", beforeResults, afterResults)
	}
	if eventLog.Len() != 0 {
		t.Fatalf("zero-value Player emitted events: %q", eventLog.String())
	}
	if len(reports) != 2 || !strings.Contains(reports[0], "Game.Victory") || !strings.Contains(reports[1], "Game.Defeat") {
		t.Fatalf("zero-value Player reports = %v, want Game.Victory and Game.Defeat", reports)
	}
}

func apiConflictTrace(t *testing.T, victoryFirst bool) string {
	t.Helper()
	w := sim.NewWorld(sim.Caps{})
	g := newGame(w)
	p := apiPlayer(g, 4)
	var eventLog bytes.Buffer
	w.AttachEventLog(&eventLog)

	var trace []string
	g.OnVictory(func(e Event) {
		ep := e.Player()
		trace = append(trace, fmt.Sprintf("victory:p=%d result=%d", ep.idx, ep.Result()))
	})
	g.OnDefeat(func(e Event) {
		ep := e.Player()
		trace = append(trace, fmt.Sprintf("defeat:p=%d result=%d", ep.idx, ep.Result()))
	})

	before := apiResultDump(g)
	if victoryFirst {
		g.Victory(p)
		g.Defeat(p, "")
	} else {
		g.Defeat(p, "")
		g.Victory(p)
	}
	staged := apiResultDump(g)
	w.Step()
	after := apiResultDump(g)
	return fmt.Sprintf("before={%s} staged={%s} after={%s} trace=%s eventLog=%s",
		before, staged, after, strings.Join(trace, "|"), strings.TrimSpace(eventLog.String()))
}

func TestVictoryDefeatSameTickDeterministicThroughAPI(t *testing.T) {
	v1 := apiConflictTrace(t, true)
	v2 := apiConflictTrace(t, true)
	d1 := apiConflictTrace(t, false)
	d2 := apiConflictTrace(t, false)

	t.Logf("FSV api conflict victory-first run1: %s", v1)
	t.Logf("FSV api conflict victory-first run2: %s", v2)
	t.Logf("FSV api conflict defeat-first run1:  %s", d1)
	t.Logf("FSV api conflict defeat-first run2:  %s", d2)

	if v1 != v2 || d1 != d2 {
		t.Fatalf("same-tick API conflict not deterministic:\n%q\n%q\n%q\n%q", v1, v2, d1, d2)
	}
	if !strings.Contains(v1, "results=[0 0 0 0 1") || !strings.Contains(v1, "trace=victory:p=4 result=1") || !strings.Contains(v1, `"name":"victory"`) {
		t.Fatalf("victory-first outcome wrong: %s", v1)
	}
	if !strings.Contains(d1, "results=[0 0 0 0 2") || !strings.Contains(d1, "trace=defeat:p=4 result=2") || !strings.Contains(d1, `"name":"defeat"`) {
		t.Fatalf("defeat-first outcome wrong: %s", d1)
	}
}

func TestVictoryForPlayerScopesPlayerEvents(t *testing.T) {
	w := sim.NewWorld(sim.Caps{})
	g := newGame(w)
	p2 := apiPlayer(g, 2)
	p3 := apiPlayer(g, 3)
	var trace []string
	g.OnVictory(func(e Event) {
		trace = append(trace, fmt.Sprintf("p2-scope:p=%d", e.Player().idx))
	}, ForPlayer(p2))
	g.OnVictory(func(e Event) {
		trace = append(trace, fmt.Sprintf("p3-scope:p=%d", e.Player().idx))
	}, ForPlayer(p3))

	before := apiResultDump(g)
	g.Victory(p2)
	staged := apiResultDump(g)
	w.Step()
	after := apiResultDump(g)

	t.Logf("FSV api victory ForPlayer BEFORE: %s", before)
	t.Logf("FSV api victory ForPlayer STAGED: %s", staged)
	t.Logf("FSV api victory ForPlayer AFTER:  %s trace=%v", after, trace)

	if got, want := strings.Join(trace, "|"), "p2-scope:p=2"; got != want {
		t.Fatalf("ForPlayer scope on victory = %q, want %q", got, want)
	}
}

func TestEndMatchDefeatsAllPlayingPlayers(t *testing.T) {
	w := sim.NewWorld(sim.Caps{})
	g := newGame(w)
	var eventLog bytes.Buffer
	w.AttachEventLog(&eventLog)
	var trace []string
	g.OnDefeat(func(e Event) {
		ep := e.Player()
		trace = append(trace, fmt.Sprintf("p=%d result=%d", ep.idx, ep.Result()))
	})

	beforeDump := apiResultDump(g)
	beforeResults := apiResults(g)
	g.EndMatch()
	stagedDump := apiResultDump(g)
	stagedResults := apiResults(g)
	w.Step()
	afterDump := apiResultDump(g)
	afterResults := apiResults(g)

	t.Logf("FSV api endmatch BEFORE: %s", beforeDump)
	t.Logf("FSV api endmatch STAGED: %s", stagedDump)
	t.Logf("FSV api endmatch AFTER:  %s trace=%v eventLog=%q", afterDump, trace, eventLog.String())

	if beforeResults != stagedResults {
		t.Fatalf("EndMatch changed visible results before phase-6 resolution: before=%v staged=%v", beforeResults, stagedResults)
	}
	for p, got := range afterResults {
		if got != sim.ResultLost {
			t.Fatalf("player %d result=%d, want lost; dump=%s", p, got, afterDump)
		}
	}
	if len(trace) != sim.MaxPlayers {
		t.Fatalf("EndMatch emitted %d defeat traces, want %d: %v", len(trace), sim.MaxPlayers, trace)
	}
	if strings.Count(eventLog.String(), `"name":"defeat"`) != sim.MaxPlayers {
		t.Fatalf("EndMatch event log does not contain %d defeats: %q", sim.MaxPlayers, eventLog.String())
	}
}

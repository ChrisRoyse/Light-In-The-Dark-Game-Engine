package sim

import (
	"fmt"
	"strings"
	"testing"
)

const (
	resultVictoryHandler HandlerID = 3101
	resultDefeatHandler  HandlerID = 3102
)

func captureResultEvents(w *World, trace *[]string) {
	w.RegisterHandler(resultVictoryHandler, func(w *World, e Event) {
		*trace = append(*trace, fmt.Sprintf("t%d victory p=%d", w.Tick(), e.Arg))
	})
	w.RegisterHandler(resultDefeatHandler, func(w *World, e Event) {
		*trace = append(*trace, fmt.Sprintf("t%d defeat p=%d result=%d", w.Tick(), e.Arg, w.results[e.Arg]))
	})
	w.Subscribe(EvVictory, resultVictoryHandler)
	w.Subscribe(EvDefeat, resultDefeatHandler)
}

func resultDump(w *World) string {
	return fmt.Sprintf("tick=%d results=%v pending=%v eventCount=%d", w.Tick(), w.results, w.resultPending, w.eventCount)
}

func TestGameStateDoubleVictoryOneEvent(t *testing.T) {
	w := NewWorld(Caps{})
	var trace []string
	captureResultEvents(w, &trace)

	before := resultDump(w)
	ok1 := w.SetVictory(2)
	ok2 := w.SetVictory(2)
	staged := resultDump(w)
	w.Step()
	after := resultDump(w)

	t.Logf("FSV gamestate double victory BEFORE: %s", before)
	t.Logf("FSV gamestate double victory STAGED: %s ok1=%v ok2=%v", staged, ok1, ok2)
	t.Logf("FSV gamestate double victory AFTER:  %s events=%v", after, trace)

	if !ok1 || ok2 {
		t.Fatalf("double victory staging got ok1=%v ok2=%v, want true,false", ok1, ok2)
	}
	if w.results[2] != ResultWon {
		t.Fatalf("player 2 result=%d want won", w.results[2])
	}
	if fmt.Sprint(trace) != fmt.Sprint([]string{"t1 victory p=2"}) {
		t.Fatalf("events=%v want one victory", trace)
	}
	if w.eventCount != 0 || w.resultPending[2] != ResultPlaying {
		t.Fatalf("ring/pending not cleared: %s", resultDump(w))
	}
}

func TestGameStateVictoryThenDefeatLatch(t *testing.T) {
	w := NewWorld(Caps{})
	var trace []string
	captureResultEvents(w, &trace)

	if !w.SetVictory(3) {
		t.Fatal("initial victory rejected")
	}
	before := resultDump(w)
	w.Step()
	won := resultDump(w)
	okDefeat := w.SetDefeat(3)
	staged := resultDump(w)
	w.Step()
	after := resultDump(w)

	t.Logf("FSV gamestate latch BEFORE: %s", before)
	t.Logf("FSV gamestate latch WON:    %s", won)
	t.Logf("FSV gamestate latch STAGED: %s okDefeat=%v", staged, okDefeat)
	t.Logf("FSV gamestate latch AFTER:  %s events=%v", after, trace)

	if okDefeat {
		t.Fatal("defeat after terminal victory was accepted")
	}
	if w.results[3] != ResultWon {
		t.Fatalf("player 3 result=%d want won", w.results[3])
	}
	if fmt.Sprint(trace) != fmt.Sprint([]string{"t1 victory p=3"}) {
		t.Fatalf("events=%v want only initial victory", trace)
	}
}

func sameTickConflict(victoryFirst bool) string {
	w := NewWorld(Caps{})
	var trace []string
	captureResultEvents(w, &trace)
	var okA, okB bool
	if victoryFirst {
		okA = w.SetVictory(4)
		okB = w.SetDefeat(4)
	} else {
		okA = w.SetDefeat(4)
		okB = w.SetVictory(4)
	}
	w.Step()
	return fmt.Sprintf("okA=%v okB=%v %s events=%s", okA, okB, resultDump(w), strings.Join(trace, "|"))
}

func TestGameStateSameTickConflictFirstStagedWins(t *testing.T) {
	v1 := sameTickConflict(true)
	v2 := sameTickConflict(true)
	d1 := sameTickConflict(false)
	d2 := sameTickConflict(false)
	t.Logf("FSV gamestate conflict victory-first run1: %s", v1)
	t.Logf("FSV gamestate conflict victory-first run2: %s", v2)
	t.Logf("FSV gamestate conflict defeat-first run1:  %s", d1)
	t.Logf("FSV gamestate conflict defeat-first run2:  %s", d2)

	if v1 != v2 || d1 != d2 {
		t.Fatalf("same-tick conflict not deterministic:\n%q\n%q\n%q\n%q", v1, v2, d1, d2)
	}
	if !strings.Contains(v1, "results=[0 0 0 0 1") || !strings.Contains(v1, "victory p=4") || !strings.Contains(v1, "okA=true okB=false") {
		t.Fatalf("victory-first outcome wrong: %s", v1)
	}
	if !strings.Contains(d1, "results=[0 0 0 0 2") || !strings.Contains(d1, "defeat p=4 result=2") || !strings.Contains(d1, "okA=true okB=false") {
		t.Fatalf("defeat-first outcome wrong: %s", d1)
	}
}

func TestGameStateSingleWinner(t *testing.T) {
	w := NewWorld(Caps{})
	var trace []string
	captureResultEvents(w, &trace)

	ok3 := w.SetVictory(3)
	ok1 := w.SetVictory(1)
	before := resultDump(w)
	w.Step()
	afterFirst := resultDump(w)
	ok2 := w.SetVictory(2)
	w.Step()
	afterSecond := resultDump(w)

	t.Logf("FSV gamestate single-winner STAGED: %s ok3=%v ok1=%v", before, ok3, ok1)
	t.Logf("FSV gamestate single-winner AFTER1: %s events=%v", afterFirst, trace)
	t.Logf("FSV gamestate single-winner AFTER2: %s ok2=%v events=%v", afterSecond, ok2, trace)

	if !ok3 || ok1 || ok2 {
		t.Fatalf("winner staging got ok3=%v ok1=%v ok2=%v, want true,false,false", ok3, ok1, ok2)
	}
	winners := 0
	for p := 0; p < MaxPlayers; p++ {
		if w.results[p] == ResultWon {
			winners++
		}
	}
	if winners != 1 || w.results[3] != ResultWon {
		t.Fatalf("winner latch wrong: winners=%d dump=%s", winners, afterSecond)
	}
	if fmt.Sprint(trace) != fmt.Sprint([]string{"t1 victory p=3"}) {
		t.Fatalf("events=%v want one victory for player 3", trace)
	}
}

func TestGameStateAllPlayersTerminalQuiet(t *testing.T) {
	w := NewWorld(Caps{})
	var trace []string
	captureResultEvents(w, &trace)
	for p := uint8(0); p < MaxPlayers; p++ {
		var ok bool
		switch {
		case p == 0:
			ok = w.SetVictory(p)
		case p%2 == 0:
			ok = w.SetLeft(p)
		default:
			ok = w.SetDefeat(p)
		}
		if !ok {
			t.Fatalf("stage terminal result for player %d failed", p)
		}
	}
	before := resultDump(w)
	w.Step()
	after := resultDump(w)
	firstEvents := append([]string{}, trace...)
	trace = trace[:0]
	for i := 0; i < 100; i++ {
		w.Step()
	}
	quiet := resultDump(w)
	t.Logf("FSV gamestate all-terminal BEFORE: %s", before)
	t.Logf("FSV gamestate all-terminal AFTER:  %s events=%v", after, firstEvents)
	t.Logf("FSV gamestate all-terminal QUIET:  %s events=%v", quiet, trace)

	if w.results[0] != ResultWon {
		t.Fatalf("player 0 result=%d want won", w.results[0])
	}
	for p := 1; p < MaxPlayers; p++ {
		if w.results[p] == ResultPlaying {
			t.Fatalf("player %d still playing: %s", p, quiet)
		}
	}
	if len(firstEvents) != MaxPlayers {
		t.Fatalf("first terminal pass emitted %d events, want %d: %v", len(firstEvents), MaxPlayers, firstEvents)
	}
	if len(trace) != 0 || w.eventCount != 0 {
		t.Fatalf("terminal state kept emitting: trace=%v dump=%s", trace, quiet)
	}
}

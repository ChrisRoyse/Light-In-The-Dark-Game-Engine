package luabind

// #558 FSV — the save-safe Lua timer surface. SoT = the fire trace a Lua
// callback records (read back in Go), the sim StateHash for save/load
// parity, and the persisted Lua counter across a save boundary.

import (
	"bytes"
	"testing"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	lua "github.com/yuin/gopher-lua"
)

// traceGame builds a script game, registers the save-safe timer prelude
// (as LoadWorld would), and installs a Go `mark(x)` global that appends x
// to the returned slice — the observable fire trace.
func traceGame(t *testing.T) (*gameForTrace, *lua.LState) {
	t.Helper()
	g, L, reg := newScriptGame(t)
	if err := RegisterTimerPrelude(L, reg); err != nil {
		t.Fatalf("RegisterTimerPrelude: %v", err)
	}
	gt := &gameForTrace{g: g}
	L.SetGlobal("mark", L.NewFunction(func(L *lua.LState) int {
		gt.marks = append(gt.marks, int(L.CheckNumber(1)))
		return 0
	}))
	return gt, L
}

type gameForTrace struct {
	g     *api.Game
	marks []int
}

func runLua(t *testing.T, L *lua.LState, src string) {
	t.Helper()
	if err := L.DoString(src); err != nil {
		t.Fatalf("lua: %v", err)
	}
}

func TestLuaTimerAfterFires(t *testing.T) {
	gt, L := traceGame(t)
	runLua(t, L, `After(0.10, function() mark(1) end)`) // 2 ticks
	gt.g.Advance(1)
	if len(gt.marks) != 0 {
		t.Fatalf("After fired early: %v", gt.marks)
	}
	gt.g.Advance(2)
	if len(gt.marks) != 1 || gt.marks[0] != 1 {
		t.Fatalf("After trace=%v, want [1]", gt.marks)
	}
}

func TestLuaTimerEveryAndCancel(t *testing.T) {
	gt, L := traceGame(t)
	runLua(t, L, `t = Every(0.05, function() mark(9) end)`) // every tick
	gt.g.Advance(3)
	if len(gt.marks) != 3 {
		t.Fatalf("Every fired %d times, want 3: %v", len(gt.marks), gt.marks)
	}
	runLua(t, L, `CancelTimer(t)`)
	gt.g.Advance(5)
	if len(gt.marks) != 3 {
		t.Fatalf("Every fired after CancelTimer: %v", gt.marks)
	}
}

func TestLuaTimerTimes(t *testing.T) {
	gt, L := traceGame(t)
	runLua(t, L, `Times(0.05, 4, function(h, i) mark(i) end)`)
	gt.g.Advance(20)
	want := []int{1, 2, 3, 4}
	if len(gt.marks) != len(want) {
		t.Fatalf("Times trace=%v, want %v", gt.marks, want)
	}
	for i := range want {
		if gt.marks[i] != want[i] {
			t.Fatalf("Times trace=%v, want %v", gt.marks, want)
		}
	}
}

func TestLuaTimerPausedSkips(t *testing.T) {
	gt, L := traceGame(t)
	runLua(t, L, `t = Every(0.05, function() mark(1) end)`)
	gt.g.Advance(2)
	runLua(t, L, `SetTimerPaused(t, true)`)
	gt.g.Advance(10)
	paused := len(gt.marks)
	if paused != 2 {
		t.Fatalf("paused Every kept firing: %d marks", paused)
	}
	runLua(t, L, `SetTimerPaused(t, false)`)
	gt.g.Advance(3)
	if len(gt.marks) <= paused {
		t.Fatalf("resumed Every never fired again: %d", len(gt.marks))
	}
}

func TestLuaTimerEveryOwnedAutoCancel(t *testing.T) {
	gt, L := traceGame(t)
	// Create a unit, run an owned timer, then kill the unit; the timer must stop.
	runLua(t, L, `
		u = Game_CreateUnit(Game_Player(0), Game_UnitType("hfoo"), {x=10,y=10}, 0)
		EveryOwned(u, 0.05, function() mark(1) end)`)
	gt.g.Advance(3)
	if len(gt.marks) == 0 {
		t.Fatal("owned timer never fired")
	}
	runLua(t, L, `Unit_Kill(u)`)
	// Death is deferred to phase-7 cleanup, so the polled owner check may
	// allow one in-flight fire on the kill tick; let it settle, then prove
	// it has permanently stopped.
	gt.g.Advance(2)
	settled := len(gt.marks)
	gt.g.Advance(10)
	if len(gt.marks) != settled {
		t.Fatalf("owned timer kept firing after owner death: %d -> %d", settled, len(gt.marks))
	}
}

func TestLuaTimerSubTickFiresNextTick(t *testing.T) {
	gt, L := traceGame(t)
	runLua(t, L, `After(0, function() mark(1) end)`) // <=0 → next tick
	gt.g.Advance(1)
	if len(gt.marks) != 1 {
		t.Fatalf("After(0) trace=%v, want [1] on next tick", gt.marks)
	}
}

// TestLuaTimerTimesSaveLoadFSV — the headline #558 property: a Times timer
// midway through its run survives save/load and fires exactly its full
// count, in order, across the save boundary. SoT = a PERSISTED Lua
// counter (round-trips via the coroutine/world-globals persister) and the
// final StateHash vs an unbroken run.
func TestLuaTimerTimesSaveLoadFSV(t *testing.T) {
	const fp = uint64(0xC0DEF00D)
	// fn increments a persisted global counter each fire; we read it back.
	const script = `
		fires = 0
		Times(0.05, 5, function(h, i) fires = fires + 1 end)`

	// preludeGame builds a game with the timer prelude registered (as
	// LoadWorld would) so spawned coroutines are persistable.
	preludeGame := func() (*api.Game, *lua.LState, *ChunkRegistry) {
		g, L, reg := newScriptGame(t)
		if err := RegisterTimerPrelude(L, reg); err != nil {
			t.Fatalf("RegisterTimerPrelude: %v", err)
		}
		return g, L, reg
	}

	// Unbroken reference run to 10 ticks.
	gRef, LRef, regRef := preludeGame()
	defer LRef.Close()
	defer regRef.Close()
	runRegisteredChunk(t, LRef, regRef, script)
	gRef.Advance(10)
	refHash := gRef.StateHash()
	if fires := int(lua.LVAsNumber(LRef.GetGlobal("fires"))); fires != 5 {
		t.Fatalf("unbroken Times fired %d, want 5", fires)
	}

	// Save at tick 2 (2 of 5 fired, coroutine still parked).
	gA, LA, regA := preludeGame()
	defer LA.Close()
	defer regA.Close()
	runRegisteredChunk(t, LA, regA, script)
	gA.Advance(2)
	var simBlob, scrBlob bytes.Buffer
	if err := gA.SaveState(&simBlob, fp); err != nil {
		t.Fatalf("SaveState: %v", err)
	}
	if err := SaveScripts(LA, regA, &scrBlob); err != nil {
		t.Fatalf("SaveScripts: %v", err)
	}

	// Restore into a fresh runtime: register (not run) the world chunk +
	// the prelude so both proto sets resolve, then LoadScripts restores
	// the live coroutine.
	gB, LB, regB := newScriptGame(t)
	defer LB.Close()
	defer regB.Close()
	if err := RegisterTimerPrelude(LB, regB); err != nil {
		t.Fatalf("RegisterTimerPrelude(B): %v", err)
	}
	if _, err := regB.Register("world", script); err != nil {
		t.Fatalf("re-register world: %v", err)
	}
	if err := gB.LoadState(bytes.NewReader(simBlob.Bytes()), fp); err != nil {
		t.Fatalf("LoadState: %v", err)
	}
	if err := LoadScripts(LB, regB, bytes.NewReader(scrBlob.Bytes())); err != nil {
		t.Fatalf("LoadScripts: %v", err)
	}
	gB.Advance(8) // saved at tick 2; +8 reaches tick 10, same as the unbroken run
	if got := gB.StateHash(); got != refHash {
		t.Fatalf("Times save/load hash %#x != unbroken %#x", got, refHash)
	}
	if fires := int(lua.LVAsNumber(LB.GetGlobal("fires"))); fires != 5 {
		t.Fatalf("restored Times fired %d total across the save, want 5", fires)
	}
	t.Logf("FSV #558: Times(0.05,5) saved@2 → fresh restore → finished; fires=5, hash %#x == unbroken", refHash)
}

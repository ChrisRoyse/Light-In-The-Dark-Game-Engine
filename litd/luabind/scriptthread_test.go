package luabind

// FSV for Lua-coroutine ↔ scheduler integration (#269). SoT = the live sim
// state a Lua script mutates across ticks: a script that mutates a unit,
// PolledWaits, then mutates again must show the first mutation immediately
// (synchronous run to the first wait) and the second ONLY after Game.Advance
// reaches the wake tick. Verified by reading the unit back through the Go api.

import (
	"testing"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	lua "github.com/yuin/gopher-lua"
)

func scriptGame(t *testing.T) (*api.Game, api.Unit) {
	t.Helper()
	g, err := api.NewGame(api.GameOptions{MaxUnits: 16, Seed: 23})
	if err != nil {
		t.Fatalf("NewGame: %v", err)
	}
	if err := g.DefineUnits([]data.Unit{
		{ID: "hfoo", Life: 100, MoveSpeedPerTick: 8 * fixed.One, TurnRatePerTick: 65535, CollisionSize: 16},
	}); err != nil {
		t.Fatalf("DefineUnits: %v", err)
	}
	u := g.CreateUnit(g.Player(1), g.UnitType("hfoo"), api.Vec2{X: 0, Y: 0}, api.Deg(0))
	return g, u
}

func TestLuaThreadResumesAcrossTicksFSV(t *testing.T) {
	g, u := scriptGame(t)
	L := lua.NewState()
	defer L.Close()
	if err := Register(L, g); err != nil {
		t.Fatalf("Register: %v", err)
	}
	hero := L.NewUserData()
	hero.Value = u
	L.SetGlobal("hero", hero)

	// A Lua thread: set life 10, wait 100ms (= 2 ticks), set life 20.
	if err := L.DoString(`Run(function()
		Unit_SetLife(hero, 10)
		PolledWait(0.1)
		Unit_SetLife(hero, 20)
	end)`); err != nil {
		t.Fatalf("DoString Run: %v", err)
	}

	// Run drove the coroutine synchronously to its first PolledWait: life is 10,
	// one thread parked on the scheduler.
	if got := u.Life(); got != 10 {
		t.Fatalf("after Run (pre-wait): Life=%v, want 10", got)
	}
	if g.SuspendedThreadCount() != 1 {
		t.Fatalf("expected 1 suspended thread after PolledWait, got %d", g.SuspendedThreadCount())
	}
	t.Logf("FSV pre-advance: Lua thread set Life=10 then parked (suspended=%d)", g.SuspendedThreadCount())

	// One tick: not yet (100ms = 2 ticks).
	g.Advance(1)
	if got := u.Life(); got != 10 {
		t.Fatalf("after Advance(1): Life=%v, want still 10", got)
	}

	// Second tick reaches the wake: the coroutine resumes and finishes.
	g.Advance(1)
	if got := u.Life(); got != 20 {
		t.Fatalf("after Advance(2): Life=%v, want 20 (Lua thread did not resume)", got)
	}
	if g.SuspendedThreadCount() != 0 {
		t.Fatalf("Lua thread should have finished, suspended=%d", g.SuspendedThreadCount())
	}
	t.Logf("FSV post-advance: Lua thread resumed at wake tick, Life=20 (suspended=0)")

	// Host LState currency was restored: the main thread can still run code.
	if err := L.DoString(`x = 1 + 1`); err != nil {
		t.Fatalf("host LState broken after thread drive: %v", err)
	}
}

func TestLuaThreadErrorIsSurfacedFSV(t *testing.T) {
	g, u := scriptGame(t)
	L := lua.NewState()
	defer L.Close()
	if err := Register(L, g); err != nil {
		t.Fatalf("Register: %v", err)
	}
	hero := L.NewUserData()
	hero.Value = u
	L.SetGlobal("hero", hero)

	var got []string
	OnScriptError(L, func(err error) { got = append(got, err.Error()) })

	// Error at spawn (before any wait): must surface, not be swallowed.
	if err := L.DoString(`Run(function() error("boom-immediate") end)`); err != nil {
		t.Fatalf("DoString (spawn-error thread): %v", err)
	}
	if len(got) != 1 {
		t.Fatalf("immediate script error not surfaced: handler calls=%d", len(got))
	}
	t.Logf("FSV spawn-error surfaced: %q", got[0])

	// Error on a POST-WAIT resume: the failure happens during Advance, after the
	// thread parked. It must still surface.
	got = nil
	if err := L.DoString(`Run(function()
		Unit_SetLife(hero, 5)
		PolledWait(0.05)
		error("boom-after-wait")
	end)`); err != nil {
		t.Fatalf("DoString (resume-error thread): %v", err)
	}
	if len(got) != 0 {
		t.Fatalf("error fired before the wait resumed: %v", got)
	}
	g.Advance(1) // 0.05s = 1 tick: reaches the wake, the resume raises
	if len(got) != 1 {
		t.Fatalf("post-wait script error not surfaced after Advance: calls=%d", len(got))
	}
	if g.SuspendedThreadCount() != 0 {
		t.Fatalf("errored thread should have retired, suspended=%d", g.SuspendedThreadCount())
	}
	t.Logf("FSV resume-error surfaced after Advance: %q", got[0])
}

func TestLuaThreadsDeterministicFSV(t *testing.T) {
	// Spawn several threads with different and identical wait durations, all
	// mutating sim state, then advance. Two runs of the identical scenario must
	// produce the same authoritative-state digest — cooperative scheduling and
	// same-tick resume order are deterministic (the scheduler's (wakeTick, seq)
	// total order), independent of goroutine scheduling.
	run := func() uint64 {
		g, err := api.NewGame(api.GameOptions{MaxUnits: 16, Seed: 29})
		if err != nil {
			t.Fatalf("NewGame: %v", err)
		}
		if err := g.DefineUnits([]data.Unit{
			{ID: "hfoo", Life: 100, MoveSpeedPerTick: 8 * fixed.One, TurnRatePerTick: 65535, CollisionSize: 16},
		}); err != nil {
			t.Fatalf("DefineUnits: %v", err)
		}
		L := lua.NewState()
		defer L.Close()
		if err := Register(L, g); err != nil {
			t.Fatalf("Register: %v", err)
		}
		set := func(name string, u api.Unit) {
			ud := L.NewUserData()
			ud.Value = u
			L.SetGlobal(name, ud)
		}
		set("u1", g.CreateUnit(g.Player(1), g.UnitType("hfoo"), api.Vec2{X: 10, Y: 0}, api.Deg(0)))
		set("u2", g.CreateUnit(g.Player(1), g.UnitType("hfoo"), api.Vec2{X: 20, Y: 0}, api.Deg(0)))
		set("u3", g.CreateUnit(g.Player(2), g.UnitType("hfoo"), api.Vec2{X: 30, Y: 0}, api.Deg(0)))

		// u1 and u3 both wait 0.05 (same wake tick -> resume in spawn/seq order);
		// u2 waits 0.1.
		if err := L.DoString(`
			Run(function() Unit_SetLife(u1, 11); PolledWait(0.05); Unit_SetLife(u1, 21) end)
			Run(function() Unit_SetLife(u2, 12); PolledWait(0.1);  Unit_SetLife(u2, 22) end)
			Run(function() Unit_SetPosition(u3, {x=5,y=5}); PolledWait(0.05); Unit_SetPosition(u3, {x=9,y=9}) end)
		`); err != nil {
			t.Fatalf("DoString multi-thread: %v", err)
		}
		g.Advance(4)
		if n := g.SuspendedThreadCount(); n != 0 {
			t.Fatalf("all threads should have finished after Advance(4), suspended=%d", n)
		}
		return g.StateHash()
	}

	h1, h2 := run(), run()
	t.Logf("FSV determinism: run1=%#x run2=%#x", h1, h2)
	if h1 != h2 {
		t.Fatalf("identical multi-thread scenario produced different state: %#x != %#x", h1, h2)
	}
}

func TestLuaThreadNestedSpawnFSV(t *testing.T) {
	g, err := api.NewGame(api.GameOptions{MaxUnits: 16, Seed: 31})
	if err != nil {
		t.Fatalf("NewGame: %v", err)
	}
	if err := g.DefineUnits([]data.Unit{
		{ID: "hfoo", Life: 100, MoveSpeedPerTick: 8 * fixed.One, TurnRatePerTick: 65535, CollisionSize: 16},
	}); err != nil {
		t.Fatalf("DefineUnits: %v", err)
	}
	u1 := g.CreateUnit(g.Player(1), g.UnitType("hfoo"), api.Vec2{X: 0, Y: 0}, api.Deg(0))
	u2 := g.CreateUnit(g.Player(1), g.UnitType("hfoo"), api.Vec2{X: 1, Y: 0}, api.Deg(0))

	L := lua.NewState()
	defer L.Close()
	if err := Register(L, g); err != nil {
		t.Fatalf("Register: %v", err)
	}
	OnScriptError(L, func(e error) { t.Errorf("unexpected script error: %v", e) })
	set := func(n string, u api.Unit) {
		ud := L.NewUserData()
		ud.Value = u
		L.SetGlobal(n, ud)
	}
	set("u1", u1)
	set("u2", u2)

	// A thread that, mid-body, spawns ANOTHER thread; both then PolledWait.
	if err := L.DoString(`Run(function()
		Unit_SetLife(u1, 7)
		Run(function() Unit_SetLife(u2, 8); PolledWait(0.05); Unit_SetLife(u2, 88) end)
		PolledWait(0.05)
		Unit_SetLife(u1, 77)
	end)`); err != nil {
		t.Fatalf("DoString nested: %v", err)
	}
	// Both threads parked at their first wait, with their pre-wait writes applied.
	if u1.Life() != 7 || u2.Life() != 8 {
		t.Fatalf("pre-advance: u1=%v u2=%v, want 7/8", u1.Life(), u2.Life())
	}
	if g.SuspendedThreadCount() != 2 {
		t.Fatalf("expected 2 suspended (outer + nested), got %d", g.SuspendedThreadCount())
	}
	t.Logf("FSV nested pre-advance: u1=7 u2=8 suspended=2")

	g.Advance(1) // 0.05s = 1 tick: both wake
	if u1.Life() != 77 || u2.Life() != 88 {
		t.Fatalf("post-advance: u1=%v u2=%v, want 77/88 (nested resume failed)", u1.Life(), u2.Life())
	}
	if g.SuspendedThreadCount() != 0 {
		t.Fatalf("both threads should have finished, suspended=%d", g.SuspendedThreadCount())
	}
	t.Logf("FSV nested post-advance: u1=77 u2=88 suspended=0 — nested thread resumed correctly")
}

func TestLuaThreadNoWaitRunsToCompletionFSV(t *testing.T) {
	g, u := scriptGame(t)
	L := lua.NewState()
	defer L.Close()
	if err := Register(L, g); err != nil {
		t.Fatalf("Register: %v", err)
	}
	hero := L.NewUserData()
	hero.Value = u
	L.SetGlobal("hero", hero)

	// No PolledWait: the thread runs to completion synchronously inside Run.
	if err := L.DoString(`Run(function() Unit_SetLife(hero, 42) end)`); err != nil {
		t.Fatalf("DoString: %v", err)
	}
	if got := u.Life(); got != 42 {
		t.Fatalf("no-wait thread: Life=%v, want 42", got)
	}
	if g.SuspendedThreadCount() != 0 {
		t.Fatalf("no-wait thread should leave nothing suspended, got %d", g.SuspendedThreadCount())
	}
	t.Logf("FSV no-wait: Lua thread ran to completion in Run, Life=42, suspended=0")
}

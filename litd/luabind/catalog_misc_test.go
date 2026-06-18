package luabind

// Game_Every (repeating timer) + misc game/player getters (#267). SoT: for
// Game_Every, the callback's cumulative effect across deterministic Advance
// windows (fires once per interval, stoppable via the Timer it receives); for the
// getters, the Go api reading.

import (
	"testing"

	lua "github.com/yuin/gopher-lua"
)

func TestGameEveryAndMiscGettersFSV(t *testing.T) {
	g, _ := confGame(t, 41)
	p1 := g.Player(1)
	L := lua.NewState()
	defer L.Close()
	if err := Register(L, g); err != nil {
		t.Fatalf("Register: %v", err)
	}
	pud := L.NewUserData()
	pud.Value = p1
	L.SetGlobal("p1", pud)

	// Game_Every: a 1s (20-tick) repeating timer. The callback bumps a counter and
	// records that it received a valid Timer handle.
	if err := L.DoString(`_cnt = 0; _gotTimer = false
		_t = Game_Every(1.0, function(t) _cnt = _cnt + 1; _gotTimer = Valid(t) end)`); err != nil {
		t.Fatalf("Game_Every: %v", err)
	}
	if !lua.LVAsBool(L.GetGlobal("_gotTimer")) && lua.LVAsNumber(L.GetGlobal("_cnt")) != 0 {
		t.Fatal("callback ran before any advance")
	}
	g.Advance(70) // 3.5s → fires at ticks 20, 40, 60 = 3 times
	if c := int(lua.LVAsNumber(L.GetGlobal("_cnt"))); c != 3 {
		t.Fatalf("Game_Every fired %d times over 3.5s, want 3", c)
	}
	if !lua.LVAsBool(L.GetGlobal("_gotTimer")) {
		t.Fatal("Game_Every callback did not receive a valid Timer handle")
	}
	t.Logf("FSV #267 Game_Every: fired 3 times over 3.5s, callback got a valid Timer")

	// Teeth: stop the timer; no further fires.
	if err := L.DoString(`Timer_Stop(_t)`); err != nil {
		t.Fatalf("Timer_Stop: %v", err)
	}
	g.Advance(60) // 3 more seconds
	if c := int(lua.LVAsNumber(L.GetGlobal("_cnt"))); c != 3 {
		t.Fatalf("Game_Every kept firing after Timer_Stop: count=%d, want 3", c)
	}
	t.Logf("FSV #267 Game_Every: after Timer_Stop, count stays 3 (no further fires)")

	// Misc getters: cross-check Lua against the Go api.
	if err := L.DoString(`_rep = Game_IsReplay(); _tod = Game_TimeOfDaySuspended()
		_nv = Game_NeutralVictim(); _ne = Game_NeutralExtra()
		_nv_valid = Valid(_nv); _ne_valid = Valid(_ne)`); err != nil {
		t.Fatalf("misc getters: %v", err)
	}
	if lua.LVAsBool(L.GetGlobal("_rep")) != g.IsReplay() {
		t.Fatalf("Game_IsReplay Lua=%v Go=%v", lua.LVAsBool(L.GetGlobal("_rep")), g.IsReplay())
	}
	if lua.LVAsBool(L.GetGlobal("_tod")) != g.TimeOfDaySuspended() {
		t.Fatalf("Game_TimeOfDaySuspended Lua=%v Go=%v", lua.LVAsBool(L.GetGlobal("_tod")), g.TimeOfDaySuspended())
	}
	if !lua.LVAsBool(L.GetGlobal("_nv_valid")) || !lua.LVAsBool(L.GetGlobal("_ne_valid")) {
		t.Fatal("NeutralVictim/NeutralExtra returned invalid players")
	}

	// Player_AlliedVictory get/set round-trip.
	if err := L.DoString(`Player_SetAlliedVictory(p1, true); _av = Player_AlliedVictory(p1)`); err != nil {
		t.Fatalf("AlliedVictory: %v", err)
	}
	if !lua.LVAsBool(L.GetGlobal("_av")) {
		t.Fatal("Player_AlliedVictory = false after SetAlliedVictory(true)")
	}
	if !p1.AlliedVictory() {
		t.Fatal("Go SoT: p1.AlliedVictory() = false after Lua set true")
	}
	t.Logf("FSV #267 misc getters: IsReplay/TimeOfDaySuspended Lua==Go; Neutral players valid; AlliedVictory set→get true (Lua+Go)")
}

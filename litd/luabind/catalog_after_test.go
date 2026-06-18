package luabind

// Game_After binding (#267): schedule a one-shot Lua callback after N seconds of
// game time, via the same deterministic scheduler the coroutine bridge uses. SoT
// = the callback's sim effect (hero life), observed BEFORE the wake tick (not yet
// fired) and AFTER (fired) — the trigger->process->outcome chain.

import (
	"testing"

	lua "github.com/yuin/gopher-lua"
)

func TestGameAfterBindingFSV(t *testing.T) {
	g, hero := confGame(t, 13)
	L := lua.NewState()
	defer L.Close()
	if err := Register(L, g); err != nil {
		t.Fatalf("Register: %v", err)
	}
	ud := L.NewUserData()
	ud.Value = hero
	L.SetGlobal("hero", ud)

	// Schedule: after 2s of game time (= 40 ticks at 20 tps), set hero life to 7.
	if err := L.DoString(`_t = Game_After(2.0, function() Unit_SetLife(hero, 7) end)
		_valid = Valid(_t)`); err != nil {
		t.Fatalf("Game_After: %v", err)
	}
	if !lua.LVAsBool(L.GetGlobal("_valid")) {
		t.Fatal("Game_After did not return a valid Timer handle")
	}
	if hero.Life() != 100 {
		t.Fatalf("hero life perturbed before any advance: %v", hero.Life())
	}

	// Teeth: 1s (20 ticks) < 2s wake — callback must NOT have fired yet.
	g.Advance(20)
	if hero.Life() != 100 {
		t.Fatalf("callback fired early: hero life=%v at 1s, want 100 (wake at 2s)", hero.Life())
	}

	// Past the wake tick: callback fires, hero life becomes 7.
	g.Advance(30) // total 50 ticks = 2.5s >= 2s
	if hero.Life() != 7 {
		t.Fatalf("callback did not fire by 2.5s: hero life=%v, want 7", hero.Life())
	}
	t.Logf("FSV #267 Game_After: life 100 before, 100 at 1s (not yet), 7 after 2.5s (fired); Timer handle valid")
}

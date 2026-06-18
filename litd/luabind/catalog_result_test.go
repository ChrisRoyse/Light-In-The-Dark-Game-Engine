package luabind

// Player_Result binding (#200/#346): the read side of the match-result surface.
// SoT = the sim per-player result store, read via the Go api Player.Result()
// after results are staged from Lua (Game_Victory/Defeat) and resolved by the
// deterministic phase-6 pass (one Advance). Closes the gap where a script could
// stage a result but never read it.

import (
	"testing"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	lua "github.com/yuin/gopher-lua"
)

func TestPlayerResultBindingFSV(t *testing.T) {
	g, _ := confGame(t, 19)
	L := lua.NewState()
	defer L.Close()
	if err := Register(L, g); err != nil {
		t.Fatalf("Register: %v", err)
	}
	p1, p2, p3 := g.Player(1), g.Player(2), g.Player(3)
	for name, p := range map[string]api.Player{"p1": p1, "p2": p2, "p3": p3} {
		ud := L.NewUserData()
		ud.Value = p
		L.SetGlobal(name, ud)
	}
	luaResult := func(global string) int {
		if err := L.DoString("_r = Player_Result(" + global + ")"); err != nil {
			t.Fatalf("Player_Result(%s): %v", global, err)
		}
		return int(lua.LVAsNumber(L.GetGlobal("_r")))
	}

	// BEFORE: nobody has a terminal result.
	if r := luaResult("p1"); r != int(api.ResultPlaying) {
		t.Fatalf("p1 result before = %d, want Playing(%d)", r, int(api.ResultPlaying))
	}

	// Stage from Lua: p1 wins, p3 loses; p2 untouched. Resolve in phase 6.
	if err := L.DoString(`Game_Victory(p1); Game_Defeat(p3, "synthetic defeat reason")`); err != nil {
		t.Fatalf("stage results: %v", err)
	}
	g.Advance(1) // deterministic phase-6 result pass

	// AFTER: Lua read must match the sim store (Go Player.Result()) for each.
	cases := []struct {
		name string
		p    api.Player
		want api.MatchResult
	}{
		{"p1", p1, api.ResultWon},
		{"p2", p2, api.ResultPlaying},
		{"p3", p3, api.ResultLost},
	}
	for _, c := range cases {
		lr := luaResult(c.name)
		gr := c.p.Result() // SoT: the sim results array via the Go api
		if lr != int(gr) {
			t.Fatalf("%s: Lua Player_Result=%d disagrees with Go SoT=%d", c.name, lr, int(gr))
		}
		if api.MatchResult(lr) != c.want {
			t.Fatalf("%s result = %d, want %d", c.name, lr, int(c.want))
		}
		t.Logf("FSV #200 result %s: Lua=%d Go=%d (want %d) — match", c.name, lr, int(gr), int(c.want))
	}

	// Latch edge: p1 already Won — a later Defeat must NOT overwrite it.
	if err := L.DoString(`Game_Defeat(p1, "too late")`); err != nil {
		t.Fatalf("late defeat: %v", err)
	}
	g.Advance(1)
	if r := luaResult("p1"); api.MatchResult(r) != api.ResultWon {
		t.Fatalf("latch broken: p1 result = %d after late Defeat, want Won(%d)", r, int(api.ResultWon))
	}
	if gr := p1.Result(); gr != api.ResultWon {
		t.Fatalf("latch broken at SoT: Go p1.Result()=%d, want Won", int(gr))
	}
	t.Logf("FSV #200 latch: p1 stays Won(%d) after a late Defeat (Lua + Go SoT agree)", int(api.ResultWon))
}

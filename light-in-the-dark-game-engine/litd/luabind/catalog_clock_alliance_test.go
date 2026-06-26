package luabind

// Game_ElapsedTime + Player_SetAlliance + Alliance_* constants (#267). SoT: the
// sim clock (Game.ElapsedTime) and the player alliance bitmask (Player.AllianceWith
// / IsAlly), read via the Go api after driving them from Lua.

import (
	"testing"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	lua "github.com/yuin/gopher-lua"
)

func TestGameElapsedTimeBindingFSV(t *testing.T) {
	g, _ := confGame(t, 21)
	L := lua.NewState()
	defer L.Close()
	if err := Register(L, g); err != nil {
		t.Fatalf("Register: %v", err)
	}
	read := func() float64 {
		if err := L.DoString(`_t = Game_ElapsedTime()`); err != nil {
			t.Fatalf("Game_ElapsedTime: %v", err)
		}
		return float64(lua.LVAsNumber(L.GetGlobal("_t")))
	}
	if read() != 0 || g.ElapsedTime() != 0 {
		t.Fatalf("BEFORE: lua=%v go=%v, want 0/0", read(), g.ElapsedTime())
	}
	g.Advance(20) // 1.0s
	if lt, gt := read(), g.ElapsedTime(); lt != 1.0 || gt != 1.0 {
		t.Fatalf("after 1s: lua=%v go=%v, want 1.0/1.0", lt, gt)
	}
	g.Advance(10) // +0.5s
	if lt := read(); lt != 1.5 {
		t.Fatalf("after 1.5s: lua=%v, want 1.5", lt)
	}
	t.Logf("FSV #267 ElapsedTime: 0 → 1.0 → 1.5 (Lua == Go sim clock)")
}

func TestPlayerSetAllianceBindingFSV(t *testing.T) {
	g, _ := confGame(t, 22)
	p1, p2 := g.Player(1), g.Player(2)
	L := lua.NewState()
	defer L.Close()
	if err := Register(L, g); err != nil {
		t.Fatalf("Register: %v", err)
	}
	for n, p := range map[string]api.Player{"p1": p1, "p2": p2} {
		ud := L.NewUserData()
		ud.Value = p
		L.SetGlobal(n, ud)
	}

	// Constant resolves to the real api bit.
	if err := L.DoString(`assert(Alliance_Passive == 1, "Alliance_Passive")`); err != nil {
		t.Fatalf("Alliance_Passive: %v", err)
	}

	// Set passive + shared vision from Lua (bits combined by arithmetic).
	if err := L.DoString(`Player_SetAlliance(p1, p2, Alliance_Passive + Alliance_SharedVision)`); err != nil {
		t.Fatalf("Player_SetAlliance: %v", err)
	}
	want := api.AllyPassive | api.AllySharedVision
	if got := p1.AllianceWith(p2); got != want {
		t.Fatalf("AllianceWith = %#x, want %#x (passive|sharedvision)", uint16(got), uint16(want))
	}
	if !p1.IsAlly(p2) {
		t.Fatal("p1 not ally of p2 after setting AllyPassive")
	}
	t.Logf("FSV #267 SetAlliance: Lua set passive|sharedvision → AllianceWith=%#x, IsAlly=true (Go SoT)", uint16(p1.AllianceWith(p2)))

	// Edge: clear all flags → no longer allied (fail back to hostile).
	if err := L.DoString(`Player_SetAlliance(p1, p2, 0)`); err != nil {
		t.Fatalf("clear alliance: %v", err)
	}
	if p1.AllianceWith(p2) != 0 || p1.IsAlly(p2) {
		t.Fatalf("after clearing: AllianceWith=%#x IsAlly=%v, want 0/false", uint16(p1.AllianceWith(p2)), p1.IsAlly(p2))
	}
	t.Logf("FSV #267 SetAlliance edge: cleared to 0 → AllianceWith=0, IsAlly=false")
}

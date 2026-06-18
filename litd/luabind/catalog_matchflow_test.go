package luabind

// Match-flow state-machine integration FSV (#201, dogfooding #267): loads
// worlds/match-flow — written purely against the bound surface (Game_Every,
// Player_Result, Storage) — and drives it headlessly through setup→countdown→
// play→terminal, consuming the win/lose result substrate (#200/#344). SoT = the
// flow state + result/duration published to Storage, cross-checked against the
// Go result api. UI/screens/locale are out of scope (gated, screenshot-verified).

import (
	"path/filepath"
	"testing"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	lua "github.com/yuin/gopher-lua"
)

const (
	flowSetup     = 0
	flowCountdown = 1
	flowPlay      = 2
	flowTerminal  = 3
)

func matchFlowGame(t *testing.T, seed int64) *api.Game {
	t.Helper()
	g, err := api.NewGame(api.GameOptions{MaxUnits: 16, Seed: seed})
	if err != nil {
		t.Fatalf("NewGame: %v", err)
	}
	// A minimal unit so the result substrate has players with armies if needed.
	if err := g.DefineUnits([]data.Unit{
		{ID: "hfoo", Life: 100, MoveSpeedPerTick: 8 * fixed.One, TurnRatePerTick: 65535, CollisionSize: 16},
	}); err != nil {
		t.Fatalf("DefineUnits: %v", err)
	}
	L := lua.NewState()
	if err := Register(L, g); err != nil {
		t.Fatalf("Register: %v", err)
	}
	reg := NewChunkRegistry()
	t.Cleanup(func() { L.Close(); reg.Close() })
	if _, err := LoadWorld(L, reg, filepath.Join("..", "..", "worlds", "match-flow")); err != nil {
		t.Fatalf("LoadWorld(match-flow): %v", err)
	}
	return g
}

func flowInt(g *api.Game, key string) int { v, _ := g.Storage().GetInt("match", key); return v }

func TestMatchFlowWorldFSV(t *testing.T) {
	g := matchFlowGame(t, 5)

	// Starts in SETUP.
	if s := flowInt(g, "state"); s != flowSetup {
		t.Fatalf("initial state=%d, want SETUP(%d)", s, flowSetup)
	}
	// SETUP(5t) → COUNTDOWN.
	g.Advance(6)
	if s := flowInt(g, "state"); s != flowCountdown {
		t.Fatalf("after 6t state=%d, want COUNTDOWN(%d)", s, flowCountdown)
	}
	// COUNTDOWN(20t) → PLAY.
	g.Advance(22)
	if s := flowInt(g, "state"); s != flowPlay {
		t.Fatalf("after countdown state=%d, want PLAY(%d)", s, flowPlay)
	}
	playStart := flowInt(g, "startedat")
	if playStart <= 0 {
		t.Fatalf("startedat=%d not recorded entering PLAY", playStart)
	}
	// Stays in PLAY while no result.
	g.Advance(40)
	if s := flowInt(g, "state"); s != flowPlay {
		t.Fatalf("state=%d during play with no result, want PLAY", s)
	}

	// Trigger a victory for the local player (player 1) — the win/lose substrate
	// latches ResultWon, the flow must transition to TERMINAL with result=Won.
	g.Victory(g.Player(1))
	g.Advance(2)
	if g.Player(1).Result() != api.ResultWon {
		t.Fatalf("victory not latched in sim: result=%d", int(g.Player(1).Result()))
	}
	if s := flowInt(g, "state"); s != flowTerminal {
		t.Fatalf("after victory state=%d, want TERMINAL(%d)", s, flowTerminal)
	}
	if r := flowInt(g, "result"); r != int(api.ResultWon) {
		t.Fatalf("terminal result=%d, want Won(%d)", r, int(api.ResultWon))
	}
	if d := flowInt(g, "duration"); d <= 0 {
		t.Fatalf("match duration=%d, want >0 ticks of play", d)
	}
	t.Logf("FSV #201 victory flow: SETUP→COUNTDOWN→PLAY→TERMINAL; result=Won, duration=%d ticks", flowInt(g, "duration"))

	// TERMINAL latches — further ticks don't change state/result.
	g.Advance(20)
	if flowInt(g, "state") != flowTerminal || flowInt(g, "result") != int(api.ResultWon) {
		t.Fatal("TERMINAL not latched")
	}

	// --- Defeat path (fresh match) → result=Lost. ---
	g2 := matchFlowGame(t, 6)
	g2.Advance(30) // through setup+countdown into PLAY
	if flowInt(g2, "state") != flowPlay {
		t.Fatalf("defeat run: state=%d, want PLAY", flowInt(g2, "state"))
	}
	g2.Defeat(g2.Player(1), "test defeat")
	g2.Advance(2)
	if flowInt(g2, "state") != flowTerminal || flowInt(g2, "result") != int(api.ResultLost) {
		t.Fatalf("defeat flow: state=%d result=%d, want TERMINAL/Lost(%d)", flowInt(g2, "state"), flowInt(g2, "result"), int(api.ResultLost))
	}
	t.Logf("FSV #201 defeat flow: fresh match → PLAY → Defeat → TERMINAL result=Lost")

	// --- Reset edge: a second fresh match starts cleanly at SETUP (no leak). ---
	g3 := matchFlowGame(t, 7)
	if s, r := flowInt(g3, "state"), flowInt(g3, "result"); s != flowSetup || r != 0 {
		t.Fatalf("second match not clean: state=%d result=%d, want SETUP/0", s, r)
	}
	t.Logf("FSV #201 reset: a fresh match begins at SETUP with no carried result (clean per-match state)")
}

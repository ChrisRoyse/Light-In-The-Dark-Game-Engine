package match_test

// Full State Verification for the N-player roster generalization (#636,
// ultimate-test-plan Phase 0). SoT = the Flow's roster contents + the phase at
// each transition over a stepped match. Proves: a roster is built from a
// MatchSpec, drives the unchanged phase machine to a terminal result, and that
// the 2-faction hardcode is gone (3-player roster, in order). Edges: 0 players
// and duplicate slot are loud refusals that do NOT transition.

import (
	"testing"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/match"
)

func rosterGame(t *testing.T) *api.Game {
	t.Helper()
	g, err := api.NewGame(api.GameOptions{MaxUnits: 16, Seed: 3})
	if err != nil {
		t.Fatalf("NewGame: %v", err)
	}
	if err := g.DefineUnits([]data.Unit{
		{ID: "hfoo", Life: 100, MoveSpeedPerTick: 8 * fixed.One, TurnRatePerTick: 65535, CollisionSize: 16},
	}); err != nil {
		t.Fatalf("DefineUnits: %v", err)
	}
	return g
}

func TestRosterFromSpecDrivesFlowFSV(t *testing.T) {
	g := rosterGame(t)
	flow := match.NewFlow(g, g.Player(0))

	// Build the roster from a validated 2-player MatchSpec (proves the source).
	const spec2 = `
victory = "score"
[[players]]
slot = 1
race = "unbound"
controller = "cpu"
difficulty = "insane"
ai_strategy = "data/ai/unbound.toml"
[[players]]
slot = 0
race = "vigil"
controller = "user"
`
	spec, err := match.LoadMatchSpec([]byte(spec2))
	if err != nil {
		t.Fatalf("LoadMatchSpec: %v", err)
	}
	roster := match.RosterFromSpec(spec)
	t.Logf("FSV roster BEFORE Begin: flow.Roster()=%v phase=%s", flow.Roster(), flow.Phase())
	if len(flow.Roster()) != 0 || flow.Phase() != match.PhaseSetup {
		t.Fatalf("pre-begin: roster=%v phase=%s, want empty/setup", flow.Roster(), flow.Phase())
	}

	if !flow.BeginRoster(roster) || flow.Phase() != match.PhaseCountdown {
		t.Fatalf("BeginRoster: accepted=%v phase=%s, want true/countdown", flow.Phase() == match.PhaseCountdown, flow.Phase())
	}
	got := flow.Roster()
	t.Logf("FSV roster AFTER Begin: %v phase=%s", got, flow.Phase())
	// Slot-ascending order, both players present with their spec fields.
	if len(got) != 2 || got[0].Slot != 0 || got[1].Slot != 1 {
		t.Fatalf("roster order wrong: %v", got)
	}
	if got[0].Race != "vigil" || got[0].Controller != match.ControllerUser {
		t.Fatalf("slot 0 = %+v, want vigil/user", got[0])
	}
	if got[1].Race != "unbound" || got[1].Controller != match.ControllerCPU || got[1].Difficulty != api.DifficultyInsane {
		t.Fatalf("slot 1 = %+v, want unbound/cpu/insane", got[1])
	}

	// Drive the unchanged phase machine through to a terminal result.
	if !flow.StartPlay() || flow.Phase() != match.PhasePlay {
		t.Fatalf("StartPlay: phase=%s, want play", flow.Phase())
	}
	g.Advance(5)
	g.Victory(g.Player(0))
	g.Advance(1)
	flow.Poll()
	t.Logf("FSV terminal: phase=%s result=%v", flow.Phase(), flow.Result())
	if flow.Phase() != match.PhaseTerminal || flow.Result() != api.ResultWon {
		t.Fatalf("terminal: phase=%s result=%v, want terminal/Won", flow.Phase(), flow.Result())
	}
}

func TestRosterThreePlayerInOrderFSV(t *testing.T) {
	g := rosterGame(t)
	flow := match.NewFlow(g, g.Player(0))

	// Hand-built 3-player roster, intentionally out of slot order, to prove the
	// 2-faction hardcode is gone and the roster is stored slot-ascending.
	roster := []match.PlayerSlot{
		{Slot: 2, Race: "vigil", Controller: match.ControllerCPU, Difficulty: api.DifficultyEasy, AIStrategy: "data/ai/vigil.toml"},
		{Slot: 0, Race: "unbound", Controller: match.ControllerUser, Difficulty: api.DifficultyNormal},
		{Slot: 1, Race: "vigil", Controller: match.ControllerCPU, Difficulty: api.DifficultyInsane, AIStrategy: "data/ai/vigil.toml"},
	}
	t.Logf("FSV 3p BEFORE: input slots [2,0,1]")
	if !flow.BeginRoster(roster) {
		t.Fatal("BeginRoster(3 players) refused a valid roster")
	}
	got := flow.Roster()
	t.Logf("FSV 3p AFTER: stored slots %d,%d,%d", got[0].Slot, got[1].Slot, got[2].Slot)
	if len(got) != 3 || got[0].Slot != 0 || got[1].Slot != 1 || got[2].Slot != 2 {
		t.Fatalf("3-player roster not in ascending order: %v", got)
	}
}

func TestRosterFailClosedEdgesFSV(t *testing.T) {
	// Edge 1: empty roster → loud refusal, no transition.
	g := rosterGame(t)
	flow := match.NewFlow(g, g.Player(0))
	t.Logf("EDGE empty BEFORE: phase=%s", flow.Phase())
	if flow.BeginRoster(nil) {
		t.Fatal("BeginRoster(nil) returned true — empty roster must be refused")
	}
	t.Logf("EDGE empty AFTER: accepted=false phase=%s", flow.Phase())
	if flow.Phase() != match.PhaseSetup || len(flow.Roster()) != 0 {
		t.Fatalf("after empty refusal: phase=%s roster=%v, want setup/empty", flow.Phase(), flow.Roster())
	}

	// Edge 2: duplicate slot → refusal, no transition.
	dup := []match.PlayerSlot{
		{Slot: 0, Race: "vigil", Controller: match.ControllerUser},
		{Slot: 0, Race: "unbound", Controller: match.ControllerCPU, AIStrategy: "x"},
	}
	g2 := rosterGame(t)
	flow2 := match.NewFlow(g2, g2.Player(0))
	if flow2.BeginRoster(dup) {
		t.Fatal("BeginRoster(dup slot) returned true — must be refused")
	}
	t.Logf("EDGE dup-slot AFTER: accepted=false phase=%s roster=%v", flow2.Phase(), flow2.Roster())
	if flow2.Phase() != match.PhaseSetup || len(flow2.Roster()) != 0 {
		t.Fatalf("after dup refusal: phase=%s roster=%v, want setup/empty", flow2.Phase(), flow2.Roster())
	}

	// Edge 3: legacy 2-faction Begin still works AND now exposes a roster
	// (migration, not break).
	g3 := rosterGame(t)
	flow3 := match.NewFlow(g3, g3.Player(0))
	if !flow3.Begin(match.Setup{Faction: match.FactionVigil, Opponent: match.FactionUnbound}) {
		t.Fatal("legacy Begin refused")
	}
	r := flow3.Roster()
	t.Logf("EDGE legacy-Begin: roster=%v", r)
	if len(r) != 2 || r[0].Race != "vigil" || r[0].Controller != match.ControllerUser ||
		r[1].Race != "unbound" || r[1].Controller != match.ControllerCPU {
		t.Fatalf("legacy Begin roster = %v, want [vigil/user@0, unbound/cpu@1]", r)
	}
}

package litd_test

// D4 helpers/melee dogfood FSV (#258). SoT = the sim player-resource store,
// the unit store (counts by owner/type), and the player-result store read
// back through the public api — proving the melee setup library writes real,
// deterministic match state using only public verbs + data tables. Shares
// the dogfoodGame/cell/stepWorld harness with helpers_dogfood_test.go.

import (
	"testing"

	litd "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api/helpers/melee"
)

// TestMeleeLoadRealFactionsFSV — the tracked data/melee/*.toml tables load
// and validate; SoT = the parsed Faction fields vs the file contents.
func TestMeleeLoadRealFactionsFSV(t *testing.T) {
	vigil, err := melee.LoadFaction("../../data/melee/vigil.toml")
	if err != nil {
		t.Fatalf("LoadFaction vigil: %v", err)
	}
	unbound, err := melee.LoadFaction("../../data/melee/unbound.toml")
	if err != nil {
		t.Fatalf("LoadFaction unbound: %v", err)
	}
	t.Logf("FSV vigil=%+v", *vigil)
	t.Logf("FSV unbound=%+v", *unbound)
	if vigil.Name != "Vigil" || vigil.TownHall != "htow" || vigil.Workers.Code != "hpea" || vigil.Workers.Count != 5 {
		t.Fatalf("vigil parse wrong: %+v", *vigil)
	}
	if vigil.Gold != 500 || vigil.Lumber != 150 || vigil.FoodCap != 12 {
		t.Fatalf("vigil resources wrong: %+v", *vigil)
	}
	if len(vigil.Extra) != 0 {
		t.Fatalf("vigil should have no extra units: %+v", vigil.Extra)
	}
	if unbound.TownHall != "ugol" || unbound.Workers.Code != "uaco" || len(unbound.Extra) != 1 ||
		unbound.Extra[0].Code != "ushd" || unbound.Extra[0].Count != 1 {
		t.Fatalf("unbound parse wrong: %+v", *unbound)
	}
}

// TestMeleeStartingResourcesFSV — SetGold/SetLumber/SetFoodCap from the table.
// SoT: the player resource store via Player.Gold/Lumber/FoodCap.
func TestMeleeStartingResourcesFSV(t *testing.T) {
	_, g := dogfoodGame(t)
	p := g.Player(1)
	t.Logf("FSV before: gold=%d lumber=%d foodCap=%d", p.Gold(), p.Lumber(), p.FoodCap())

	f := &melee.Faction{Name: "T", TownHall: "htow", Gold: 500, Lumber: 150, FoodCap: 12}
	melee.StartingResources(g, p, f)
	t.Logf("FSV after: gold=%d lumber=%d foodCap=%d (want 500/150/12)", p.Gold(), p.Lumber(), p.FoodCap())
	if p.Gold() != 500 || p.Lumber() != 150 || p.FoodCap() != 12 {
		t.Fatalf("StartingResources wrong: gold=%d lumber=%d foodCap=%d", p.Gold(), p.Lumber(), p.FoodCap())
	}
}

// TestMeleeStartingUnitsFSV — spawns the town hall + worker squad + extras at
// the player's start location. SoT: sim unit counts by type/owner and the
// spawn position near the start point.
func TestMeleeStartingUnitsFSV(t *testing.T) {
	w, g := dogfoodGame(t)
	p := g.Player(2)
	start := cell(50, 50)
	p.SetStartLocation(start)

	f := &melee.Faction{
		Name: "Unbound", TownHall: "ugol",
		Workers: melee.Squad{Code: "uaco", Count: 5},
		Extra:   []melee.Squad{{Code: "ushd", Count: 1}},
	}
	units, err := melee.StartingUnits(g, p, f)
	if err != nil {
		t.Fatalf("StartingUnits: %v", err)
	}

	// SoT: count sim units of each bound type owned by player 2.
	count := func(code string) int {
		want, _ := w.UnitTypeID(code)
		n := 0
		for _, id := range w.AppendAllUnits(nil) {
			or, tr := w.Owners.Row(id), w.UnitTypes.Row(id)
			if or >= 0 && tr >= 0 && w.Owners.Player[or] == 2 && w.UnitTypes.TypeID[tr] == want {
				n++
			}
		}
		return n
	}
	town, work, extra := count("ugol"), count("uaco"), count("ushd")
	t.Logf("FSV StartingUnits: returned=%d sim townhall=%d workers=%d extra=%d (want 1/5/1)", len(units), town, work, extra)
	if len(units) != 7 {
		t.Fatalf("returned %d units, want 7 (1 town + 5 workers + 1 extra)", len(units))
	}
	if town != 1 || work != 5 || extra != 1 {
		t.Fatalf("sim unit counts town=%d work=%d extra=%d, want 1/5/1", town, work, extra)
	}
	// Position SoT: the town hall spawned at the start location.
	th := units[0]
	pos := th.Position()
	t.Logf("FSV town-hall pos=%v start=%v", pos, start)
	if pos.DistanceTo(start) > 1.0 {
		t.Fatalf("town hall spawned at %v, want start %v", pos, start)
	}
}

// TestMeleeStartingUnitsMissingRowFSV — edge (2): a faction whose town-hall
// code is not bound fails LOUDLY and spawns nothing (no partial base).
// SoT: a non-nil error AND the sim unit count unchanged.
func TestMeleeStartingUnitsMissingRowFSV(t *testing.T) {
	w, g := dogfoodGame(t)
	p := g.Player(1)
	before := len(w.AppendAllUnits(nil))

	// Town hall code absent from the bound unit table.
	bad := &melee.Faction{Name: "Broken", TownHall: "zzzz", Workers: melee.Squad{Code: "hpea", Count: 3}}
	units, err := melee.StartingUnits(g, p, bad)
	after := len(w.AppendAllUnits(nil))
	t.Logf("FSV missing-row: err=%v returned=%d simBefore=%d simAfter=%d", err, len(units), before, after)
	if err == nil {
		t.Fatal("StartingUnits with an unbound town-hall code returned nil error (silent skip) — must fail loudly")
	}
	if units != nil {
		t.Fatalf("StartingUnits error path returned %d units, want nil", len(units))
	}
	if after != before {
		t.Fatalf("StartingUnits error path spawned units: before=%d after=%d (must be all-or-nothing)", before, after)
	}

	// A bound town hall but an unbound WORKER code also aborts before spawning.
	bad2 := &melee.Faction{Name: "Broken2", TownHall: "htow", Workers: melee.Squad{Code: "qqqq", Count: 2}}
	_, err2 := melee.StartingUnits(g, p, bad2)
	if err2 == nil || len(w.AppendAllUnits(nil)) != before {
		t.Fatalf("unbound worker code: err=%v simNow=%d (want non-nil err, count %d)", err2, len(w.AppendAllUnits(nil)), before)
	}
}

// TestMeleeVictoryDefeatConditionsFSV — last-standing: when a player's last
// unit dies, it is defeated and the lone survivor wins. SoT: Player.Result.
func TestMeleeVictoryDefeatConditionsFSV(t *testing.T) {
	w, g := dogfoodGame(t)
	p1, p2 := g.Player(1), g.Player(2)
	p1.SetStartLocation(cell(30, 30))
	p2.SetStartLocation(cell(60, 60))

	f1 := &melee.Faction{Name: "A", TownHall: "htow", Workers: melee.Squad{Code: "hpea", Count: 2}}
	f2 := &melee.Faction{Name: "B", TownHall: "ugol", Workers: melee.Squad{Code: "uaco", Count: 2}}
	u1, err1 := melee.StartingUnits(g, p1, f1)
	u2, err2 := melee.StartingUnits(g, p2, f2)
	if err1 != nil || err2 != nil {
		t.Fatalf("setup spawn errors: %v %v", err1, err2)
	}

	melee.VictoryDefeatConditions(g, []litd.Player{p1, p2})
	t.Logf("FSV at setup (both have units): p1=%v p2=%v (want Playing/Playing)", p1.Result(), p2.Result())
	if p1.Result() != litd.ResultPlaying || p2.Result() != litd.ResultPlaying {
		t.Fatalf("a player decided at setup with units present: p1=%v p2=%v", p1.Result(), p2.Result())
	}
	_ = u1

	// Wipe player 2: kill every unit it owns, then step so phase-5 resolves
	// the kills and fires the death events the conditions listen on.
	for _, u := range u2 {
		u.Kill()
	}
	stepWorld(w, 2)
	t.Logf("FSV after wiping p2: p1.Result=%v p2.Result=%v p2.unitCount=%d (want Won/Lost/0)",
		p1.Result(), p2.Result(), melee.PlayerUnitCount(g, p2))
	if melee.PlayerUnitCount(g, p2) != 0 {
		t.Fatalf("player 2 still has %d units after wipe", melee.PlayerUnitCount(g, p2))
	}
	if p2.Result() != litd.ResultLost {
		t.Fatalf("player 2 result = %v, want Lost", p2.Result())
	}
	if p1.Result() != litd.ResultWon {
		t.Fatalf("player 1 result = %v, want Won (lone survivor)", p1.Result())
	}
}

// TestMeleeVictoryDefeatImmediateFSV — a player listed with zero units is
// defeated at install time (the initial check), and a lone unit-holder wins.
func TestMeleeVictoryDefeatImmediateFSV(t *testing.T) {
	w, g := dogfoodGame(t)
	p1, p3 := g.Player(1), g.Player(3)
	p1.SetStartLocation(cell(30, 30))
	f1 := &melee.Faction{Name: "A", TownHall: "htow", Workers: melee.Squad{Code: "hpea", Count: 1}}
	if _, err := melee.StartingUnits(g, p1, f1); err != nil {
		t.Fatalf("setup: %v", err)
	}
	// p3 gets NO units.
	melee.VictoryDefeatConditions(g, []litd.Player{p1, p3})
	stepWorld(w, 1)
	t.Logf("FSV immediate: p1=%v (units=%d) p3=%v (units=%d) — want Won/Lost",
		p1.Result(), melee.PlayerUnitCount(g, p1), p3.Result(), melee.PlayerUnitCount(g, p3))
	if p3.Result() != litd.ResultLost {
		t.Fatalf("zero-unit player 3 not defeated at setup: %v", p3.Result())
	}
	if p1.Result() != litd.ResultWon {
		t.Fatalf("lone survivor player 1 not victorious: %v", p1.Result())
	}
}

// TestMeleeStandardFSV — the umbrella sets resources + units for every setup
// and installs conditions; deterministic across two fresh worlds.
func TestMeleeStandardFSV(t *testing.T) {
	vigil, err := melee.LoadFaction("../../data/melee/vigil.toml")
	if err != nil {
		t.Fatalf("load vigil: %v", err)
	}
	unbound, err := melee.LoadFaction("../../data/melee/unbound.toml")
	if err != nil {
		t.Fatalf("load unbound: %v", err)
	}

	run := func() (int, int, int, int) {
		w, g := dogfoodGame(t)
		p1, p2 := g.Player(1), g.Player(2)
		p1.SetStartLocation(cell(30, 30))
		p2.SetStartLocation(cell(60, 60))
		if err := melee.Standard(g, []melee.Setup{{Player: p1, Faction: vigil}, {Player: p2, Faction: unbound}}); err != nil {
			t.Fatalf("Standard: %v", err)
		}
		_ = w
		return p1.Gold(), melee.PlayerUnitCount(g, p1), p2.Gold(), melee.PlayerUnitCount(g, p2)
	}
	g1, n1, g2, n2 := run()
	r1g1, r1n1, r1g2, r1n2 := run()
	t.Logf("FSV Standard: p1 gold=%d units=%d (vigil: 500/6=1town+5work); p2 gold=%d units=%d (unbound: 500/7)", g1, n1, g2, n2)
	if g1 != 500 || n1 != 6 {
		t.Fatalf("vigil setup wrong: gold=%d units=%d, want 500/6", g1, n1)
	}
	if g2 != 500 || n2 != 7 {
		t.Fatalf("unbound setup wrong: gold=%d units=%d, want 500/7", g2, n2)
	}
	if g1 != r1g1 || n1 != r1n1 || g2 != r1g2 || n2 != r1n2 {
		t.Fatalf("Standard nondeterministic across runs")
	}
}

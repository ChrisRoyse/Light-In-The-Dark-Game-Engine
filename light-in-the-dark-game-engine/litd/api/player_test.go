package litd

import (
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
)

// TestPlayersConformance — the players-and-forces canonical surface end
// to end. SoT: the sim player table read back directly (w.Resources,
// w.Alliance, w.PlayerName…), not just the API return values.
func TestPlayersConformance(t *testing.T) {
	w := sim.NewWorld(sim.Caps{Units: 8})
	if !w.BindEconomy(2) {
		t.Fatalf("BindEconomy failed")
	}
	g := newGame(w)

	p0 := g.Player(0)
	p1 := g.Player(1)

	// D5 resource accessors — write through API, read SoT directly.
	p0.SetGold(500)
	p0.SetLumber(120)
	p0.SetFoodCap(40)
	t.Logf("FSV p0 gold api=%d sot=%d ; lumber api=%d sot=%d ; foodcap api=%d sot=%d",
		p0.Gold(), w.Resources(0, 0), p0.Lumber(), w.Resources(0, 1), p0.FoodCap(), w.FoodCap(0))
	if p0.Gold() != 500 || w.Resources(0, 0) != 500 {
		t.Fatalf("gold: api=%d sot=%d, want 500", p0.Gold(), w.Resources(0, 0))
	}
	if p0.Lumber() != 120 || w.Resources(0, 1) != 120 {
		t.Fatalf("lumber: api=%d sot=%d, want 120", p0.Lumber(), w.Resources(0, 1))
	}
	if p0.FoodCap() != 40 || w.FoodCap(0) != 40 {
		t.Fatalf("foodcap: api=%d sot=%d, want 40", p0.FoodCap(), w.FoodCap(0))
	}

	// Metadata accessors.
	p0.SetName("synthetic_commander")
	p0.SetRace(RaceHuman)
	p0.SetColor(4)
	p0.SetTeam(2)
	p0.SetController(ControllerComputer)
	p0.SetStartLocation(Vec2{640, 320})
	p0.SetAlliedVictory(true)
	sx, sy := w.PlayerStart(0)
	t.Logf("FSV p0 name=%q race=%d color=%d team=%d controller=%d start=(%v) sotStart=(%d,%d) av=%v sotController=%d",
		p0.Name(), p0.Race(), p0.Color(), p0.Team(), p0.Controller(), p0.StartLocation(),
		int64(sx)>>32, int64(sy)>>32, p0.AlliedVictory(), w.Controller(0))
	if p0.Name() != "synthetic_commander" || w.PlayerName(0) != "synthetic_commander" {
		t.Fatalf("name mismatch: api=%q sot=%q", p0.Name(), w.PlayerName(0))
	}
	if p0.Race() != RaceHuman || p0.Color() != 4 || p0.Team() != 2 {
		t.Fatalf("metadata mismatch: race=%d color=%d team=%d", p0.Race(), p0.Color(), p0.Team())
	}
	if p0.Controller() != ControllerComputer || w.Controller(0) != sim.ControllerComputer {
		t.Fatalf("controller mismatch: api=%d sot=%d", p0.Controller(), w.Controller(0))
	}
	if p0.StartLocation() != (Vec2{640, 320}) {
		t.Fatalf("start loc mismatch: %v", p0.StartLocation())
	}
	if !p0.AlliedVictory() {
		t.Fatalf("allied victory not set")
	}

	// Enumeration: all 16 slots, ascending.
	all := g.AllPlayers()
	t.Logf("FSV AllPlayers count=%d first=%d last=%d", len(all), all[0].Slot(), all[len(all)-1].Slot())
	if len(all) != sim.MaxPlayers || all[0].Slot() != 0 || all[len(all)-1].Slot() != sim.MaxPlayers-1 {
		t.Fatalf("AllPlayers wrong: n=%d", len(all))
	}
	// Filtered: only computer-controlled (just p0).
	comps := g.Players(func(p Player) bool { return p.Controller() == ControllerComputer })
	t.Logf("FSV computer players=%v", slotsOf(comps))
	if len(comps) != 1 || comps[0].Slot() != 0 {
		t.Fatalf("computer filter wrong: %v", slotsOf(comps))
	}
	_ = p1
}

// TestPlayerAllianceAsymmetryAPIFSV — edge (1): one-directional alliance.
// SoT: w.Alliance read directly plus IsAlly/IsEnemy both directions.
func TestPlayerAllianceAsymmetryAPIFSV(t *testing.T) {
	w := sim.NewWorld(sim.Caps{})
	g := newGame(w)
	a, b := g.Player(3), g.Player(7)

	t.Logf("FSV before: a.IsAlly(b)=%v b.IsAlly(a)=%v sot[a][b]=%#x", a.IsAlly(b), b.IsAlly(a), w.Alliance(3, 7))
	a.SetAlliance(b, AllyPassive|AllySharedVision)
	t.Logf("FSV after a->b ally: a.IsAlly(b)=%v b.IsAlly(a)=%v a.IsEnemy(b)=%v b.IsEnemy(a)=%v sot[a][b]=%#x sot[b][a]=%#x",
		a.IsAlly(b), b.IsAlly(a), a.IsEnemy(b), b.IsEnemy(a), w.Alliance(3, 7), w.Alliance(7, 3))
	if !a.IsAlly(b) {
		t.Fatalf("a should be ally of b")
	}
	if b.IsAlly(a) {
		t.Fatalf("alliance leaked back to b->a (must be asymmetric)")
	}
	if !b.IsEnemy(a) {
		t.Fatalf("b should still be enemy of a")
	}
	if w.Alliance(3, 7) != uint16(AllyPassive|AllySharedVision) {
		t.Fatalf("sot alliance bitset wrong: %#x", w.Alliance(3, 7))
	}
	// Allies()/Enemies() reflect the relation.
	t.Logf("FSV a.Allies=%v a.Enemies(count)=%d", slotsOf(g.Allies(a)), len(g.Enemies(a)))
	allies := g.Allies(a)
	if len(allies) != 1 || allies[0].Slot() != 7 {
		t.Fatalf("Allies wrong: %v", slotsOf(allies))
	}
}

// TestPlayerSetGoldEdgesFSV — edge (2): SetGold on an invalid/zero-value
// player is a no-op; a real slot mutates the ledger. X+X=Y via the sim
// AddResource SoT.
func TestPlayerSetGoldEdgesFSV(t *testing.T) {
	w := sim.NewWorld(sim.Caps{})
	w.BindEconomy(2)
	g := newGame(w)

	// Zero-value / out-of-range player: no-op, no panic, ledger untouched.
	var zero Player
	zero.SetGold(999)
	oob := g.Player(99) // out of range -> zero-value
	oob.SetGold(999)
	t.Logf("FSV zero/oob SetGold inert: valid(zero)=%v valid(oob)=%v gold[0]=%d", zero.Valid(), oob.Valid(), w.Resources(0, 0))
	if zero.Valid() || oob.Valid() {
		t.Fatalf("zero/oob player reported valid")
	}
	if w.Resources(0, 0) != 0 {
		t.Fatalf("inert SetGold still wrote ledger: %d", w.Resources(0, 0))
	}

	// X+X=Y on the real slot: 2 then +2 via sim == 4.
	p := g.Player(0)
	p.SetGold(2)
	w.AddResource(0, 0, 2)
	t.Logf("FSV 2+2 gold: api=%d sot=%d", p.Gold(), w.Resources(0, 0))
	if p.Gold() != 4 {
		t.Fatalf("2+2 gold = %d, want 4", p.Gold())
	}
}

// TestPlayerNeutralSlotsFSV — edge (3): neutral players occupy fixed high
// slots. SoT: the slot indices the accessors return.
func TestPlayerNeutralSlotsFSV(t *testing.T) {
	w := sim.NewWorld(sim.Caps{})
	g := newGame(w)
	h, v, e, p := g.NeutralHostile(), g.NeutralVictim(), g.NeutralExtra(), g.NeutralPassive()
	t.Logf("FSV neutral slots: hostile=%d victim=%d extra=%d passive=%d (MaxPlayers=%d)",
		h.Slot(), v.Slot(), e.Slot(), p.Slot(), sim.MaxPlayers)
	if h.Slot() != sim.MaxPlayers-4 || v.Slot() != sim.MaxPlayers-3 ||
		e.Slot() != sim.MaxPlayers-2 || p.Slot() != sim.MaxPlayers-1 {
		t.Fatalf("neutral slots wrong: %d %d %d %d", h.Slot(), v.Slot(), e.Slot(), p.Slot())
	}
	if !h.Valid() || !p.Valid() {
		t.Fatalf("neutral players invalid")
	}
}

// TestForceMembershipFSV — edge (4): force membership before/after a
// player is removed. SoT: the force's player slice.
func TestForceMembershipFSV(t *testing.T) {
	w := sim.NewWorld(sim.Caps{})
	g := newGame(w)
	f := g.CreateForce()
	if !f.Valid() {
		t.Fatalf("fresh force invalid")
	}
	f.AddPlayer(g.Player(0))
	f.AddPlayer(g.Player(2))
	f.AddPlayer(g.Player(5))
	before := slotsOf(f.Players())
	t.Logf("FSV force before remove: members=%v count=%d contains(2)=%v", before, f.Count(), f.Contains(g.Player(2)))
	if f.Count() != 3 || !f.Contains(g.Player(2)) {
		t.Fatalf("force membership wrong before remove: %v", before)
	}

	f.RemovePlayer(g.Player(2)) // player "leaves"
	after := slotsOf(f.Players())
	t.Logf("FSV force after remove(2): members=%v count=%d contains(2)=%v", after, f.Count(), f.Contains(g.Player(2)))
	if f.Count() != 2 || f.Contains(g.Player(2)) {
		t.Fatalf("force still contains removed player: %v", after)
	}
	if len(after) != 2 || after[0] != 0 || after[1] != 5 {
		t.Fatalf("force slice after remove wrong: %v", after)
	}

	// Zero-value force is inert.
	var zf Force
	zf.AddPlayer(g.Player(0))
	t.Logf("FSV zero-value force valid=%v count=%d", zf.Valid(), zf.Count())
	if zf.Valid() || zf.Count() != 0 {
		t.Fatalf("zero-value force not inert")
	}
}

func slotsOf(ps []Player) []int {
	out := make([]int, len(ps))
	for i, p := range ps {
		out[i] = p.Slot()
	}
	return out
}

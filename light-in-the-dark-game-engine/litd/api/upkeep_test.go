package litd

import (
	"math"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
)

// TestUpkeepRateAPIFSV — Game.SetUpkeep + Player.UpkeepRate round-trip. SoT:
// the rate read back through the API and the sim getter. A Food=0 bracket is
// always active, so the rate is independent of live food usage here.
func TestUpkeepRateAPIFSV(t *testing.T) {
	w := sim.NewWorld(sim.Caps{})
	if !w.BindEconomy(2) {
		t.Fatal("BindEconomy failed")
	}
	g := newGame(w)
	p := g.Player(0)

	approx := func(a, b float64) bool { return math.Abs(a-b) < 1e-6 }

	// before: no brackets → 0 rate.
	t.Logf("FSV before: goldUpkeepRate=%.3f lumberUpkeepRate=%.3f", p.GoldUpkeepRate(), p.LumberUpkeepRate())
	if p.GoldUpkeepRate() != 0 || p.LumberUpkeepRate() != 0 {
		t.Fatal("fresh upkeep rate not 0")
	}

	ok := g.SetUpkeep([]UpkeepTier{{Food: 0, Rate: []float64{0.5, 0.25}}})
	t.Logf("FSV setUpkeep ok=%v gold=%.3f lumber=%.3f (want 0.5/0.25)", ok, p.GoldUpkeepRate(), p.LumberUpkeepRate())
	if !ok || !approx(p.GoldUpkeepRate(), 0.5) || !approx(p.LumberUpkeepRate(), 0.25) {
		t.Fatalf("upkeep rate wrong: gold=%.3f lumber=%.3f", p.GoldUpkeepRate(), p.LumberUpkeepRate())
	}
	// sim SoT agrees.
	if w.UpkeepRate(0, 0) != fromFloat(0.5) {
		t.Fatalf("sim upkeep rate = %d, want %d", int64(w.UpkeepRate(0, 0)), int64(fromFloat(0.5)))
	}

	// LostToUpkeep starts at 0 (deposit accumulation proven in sim layer).
	t.Logf("FSV lost: gold=%d lumber=%d (want 0/0)", p.GoldLostToUpkeep(), p.LumberLostToUpkeep())
	if p.GoldLostToUpkeep() != 0 || p.LumberLostToUpkeep() != 0 {
		t.Fatal("fresh lost-to-upkeep not 0")
	}

	// edge: clearing with an empty slice removes the tax.
	g.SetUpkeep(nil)
	t.Logf("FSV cleared: gold=%.3f (want 0)", p.GoldUpkeepRate())
	if p.GoldUpkeepRate() != 0 {
		t.Fatalf("upkeep not cleared: %.3f", p.GoldUpkeepRate())
	}
}

// TestTransferResourceAPIFSV — Game.TransferResource + Player.SetTaxRate
// against the ledger SoT (Player.Gold). p0 gives 50 gold to p1 at a 50% tax.
func TestTransferResourceAPIFSV(t *testing.T) {
	w := sim.NewWorld(sim.Caps{})
	if !w.BindEconomy(2) {
		t.Fatal("BindEconomy failed")
	}
	g := newGame(w)
	p0, p1 := g.Player(0), g.Player(1)

	p0.SetGold(100)
	p0.SetTaxRate(p1, resGold, 0.5)
	t.Logf("FSV before: p0=%d p1=%d taxRate=%.3f", p0.Gold(), p1.Gold(), p0.TaxRate(p1, resGold))
	if math.Abs(p0.TaxRate(p1, resGold)-0.5) > 1e-6 {
		t.Fatalf("tax rate read = %.3f, want 0.5", p0.TaxRate(p1, resGold))
	}

	delivered := g.TransferResource(p0, p1, resGold, 50)
	t.Logf("FSV after: delivered=%d p0=%d p1=%d (want 25/50/25)", delivered, p0.Gold(), p1.Gold())
	if delivered != 25 || p0.Gold() != 50 || p1.Gold() != 25 {
		t.Fatalf("transfer wrong: delivered=%d p0=%d p1=%d", delivered, p0.Gold(), p1.Gold())
	}

	// edge: invalid handle no-ops (no panic, returns 0).
	oob := g.Player(99)
	if d := g.TransferResource(p0, oob, resGold, 10); d != 0 || p0.Gold() != 50 {
		t.Fatalf("transfer to invalid mutated state: d=%d p0=%d", d, p0.Gold())
	}
	t.Logf("FSV invalid: transfer to oob delivered=0 p0=%d (unchanged)", p0.Gold())

	// edge: self-transfer no-op.
	if d := g.TransferResource(p0, p0, resGold, 10); d != 0 || p0.Gold() != 50 {
		t.Fatalf("self-transfer mutated: d=%d p0=%d", d, p0.Gold())
	}
	t.Logf("FSV self: self-transfer no-op p0=%d", p0.Gold())
}

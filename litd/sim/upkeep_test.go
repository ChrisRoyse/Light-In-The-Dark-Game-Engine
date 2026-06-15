package sim

// #375 upkeep + transfer-tax FSV. SoT = the per-player resource ledger
// (w.resources / Resources) and the upkeep-lost counters (UpkeepLost) after
// real harvest deposits and resource transfers. Conservation invariant
// (X+X=Y): for every deposit, kept + lost == gross mined.

import (
	"bytes"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/statehash"
)

// runHarvestToDepletion drives one worker harvesting one mine into one depot
// until the mine is exhausted (or a step budget runs out). Returns the
// number of steps taken.
func runHarvestToDepletion(t *testing.T, w *World, worker, mine EntityID) int {
	t.Helper()
	if !w.IssueOrder(worker, Order{Kind: OrderHarvest, Target: mine}, false) {
		t.Fatal("harvest order refused")
	}
	i := 0
	for ; i < 600 && w.Ents.Alive(mine); i++ {
		w.Step()
	}
	// a few extra steps to deliver the final carried load
	for j := 0; j < 60; j++ {
		w.Step()
	}
	return i
}

// TestUpkeepTaxAtDepositFSV — a 50% gold upkeep applied to real deposits.
// Mine 40 gold, capacity 10 → four 10-gold trips. SoT: gold counter + lost
// counter. Expect kept=20, lost=20, kept+lost=40 (conservation).
func TestUpkeepTaxAtDepositFSV(t *testing.T) {
	w := econWorld(t)
	worker := addWorker(t, w, pt2(100, 100))
	mine := addMine(t, w, pt2(140, 100), 40, false)
	addDepot(t, w, pt2(60, 100), 10)

	// worker food-cost is 1 (addWorker). Bracket at food>=1 taxes gold 50%.
	half := fixed.One / 2
	tier := UpkeepTier{Food: 1}
	tier.Rate[0] = half // gold
	if !w.BindUpkeep([]UpkeepTier{tier}) {
		t.Fatal("BindUpkeep failed")
	}

	t.Logf("FSV before: food=%d goldUpkeepRate=%#x (want %#x) gold=%d lost=%d",
		w.FoodUsed(0), uint64(w.UpkeepRate(0, 0)), uint64(half), w.Resources(0, 0), w.UpkeepLost(0, 0))
	if w.FoodUsed(0) != 1 {
		t.Fatalf("setup: foodUsed=%d want 1", w.FoodUsed(0))
	}
	if w.UpkeepRate(0, 0) != half {
		t.Fatalf("upkeep rate = %#x, want %#x", uint64(w.UpkeepRate(0, 0)), uint64(half))
	}

	runHarvestToDepletion(t, w, worker, mine)

	gold, lost := w.Resources(0, 0), w.UpkeepLost(0, 0)
	t.Logf("FSV after: gold=%d lost=%d gross=gold+lost=%d (want 20/20/40)", gold, lost, gold+lost)
	if gold != 20 {
		t.Fatalf("kept gold = %d, want 20", gold)
	}
	if lost != 20 {
		t.Fatalf("lost gold = %d, want 20", lost)
	}
	if gold+lost != 40 {
		t.Fatalf("conservation broken: kept+lost = %d, want 40", gold+lost)
	}
}

// TestUpkeepNoBracketFSV — edge: no brackets bound (the default) levies no
// tax; the deposit path is byte-identical to pre-#375. SoT: gold==40, lost==0.
func TestUpkeepNoBracketFSV(t *testing.T) {
	w := econWorld(t)
	worker := addWorker(t, w, pt2(100, 100))
	mine := addMine(t, w, pt2(140, 100), 40, false)
	addDepot(t, w, pt2(60, 100), 10)
	t.Logf("FSV before: upkeepCount=%d rate=%#x", w.upkeepCount, uint64(w.UpkeepRate(0, 0)))
	runHarvestToDepletion(t, w, worker, mine)
	t.Logf("FSV after: gold=%d lost=%d (want 40/0)", w.Resources(0, 0), w.UpkeepLost(0, 0))
	if w.Resources(0, 0) != 40 || w.UpkeepLost(0, 0) != 0 {
		t.Fatalf("untaxed deposit wrong: gold=%d lost=%d", w.Resources(0, 0), w.UpkeepLost(0, 0))
	}
}

// TestUpkeepBelowThresholdFSV — edge: food below the bracket threshold →
// rate 0, no tax. Worker food=1; bracket starts at food>=5.
func TestUpkeepBelowThresholdFSV(t *testing.T) {
	w := econWorld(t)
	worker := addWorker(t, w, pt2(100, 100))
	mine := addMine(t, w, pt2(140, 100), 40, false)
	addDepot(t, w, pt2(60, 100), 10)
	tier := UpkeepTier{Food: 5}
	tier.Rate[0] = fixed.One // would tax 100% if reached
	if !w.BindUpkeep([]UpkeepTier{tier}) {
		t.Fatal("BindUpkeep failed")
	}
	t.Logf("FSV before: food=%d threshold=5 rate=%#x (want 0)", w.FoodUsed(0), uint64(w.UpkeepRate(0, 0)))
	if w.UpkeepRate(0, 0) != 0 {
		t.Fatalf("rate below threshold = %#x, want 0", uint64(w.UpkeepRate(0, 0)))
	}
	runHarvestToDepletion(t, w, worker, mine)
	t.Logf("FSV after: gold=%d lost=%d (want 40/0)", w.Resources(0, 0), w.UpkeepLost(0, 0))
	if w.Resources(0, 0) != 40 || w.UpkeepLost(0, 0) != 0 {
		t.Fatalf("below-threshold taxed: gold=%d lost=%d", w.Resources(0, 0), w.UpkeepLost(0, 0))
	}
}

// TestUpkeepClampFullFSV — edge: a rate above 1.0 clamps to 1.0 → the whole
// deposit is withheld. SoT: gold==0, lost==40.
func TestUpkeepClampFullFSV(t *testing.T) {
	w := econWorld(t)
	worker := addWorker(t, w, pt2(100, 100))
	mine := addMine(t, w, pt2(140, 100), 40, false)
	addDepot(t, w, pt2(60, 100), 10)
	tier := UpkeepTier{Food: 1}
	tier.Rate[0] = fixed.One * 2 // 200% → clamps to 100%
	if !w.BindUpkeep([]UpkeepTier{tier}) {
		t.Fatal("BindUpkeep failed")
	}
	t.Logf("FSV before: requested rate=%#x clamped=%#x (want %#x)", uint64(fixed.One*2), uint64(w.UpkeepRate(0, 0)), uint64(fixed.One))
	if w.UpkeepRate(0, 0) != fixed.One {
		t.Fatalf("rate not clamped: %#x", uint64(w.UpkeepRate(0, 0)))
	}
	runHarvestToDepletion(t, w, worker, mine)
	t.Logf("FSV after: gold=%d lost=%d (want 0/40)", w.Resources(0, 0), w.UpkeepLost(0, 0))
	if w.Resources(0, 0) != 0 || w.UpkeepLost(0, 0) != 40 {
		t.Fatalf("full clamp wrong: gold=%d lost=%d", w.Resources(0, 0), w.UpkeepLost(0, 0))
	}
}

// TestUpkeepTierSelectionFSV — multi-bracket tier selection read directly
// against synthetic food usage. SoT: UpkeepRate for gold at each food level.
func TestUpkeepTierSelectionFSV(t *testing.T) {
	w := econWorld(t)
	q, h, tq := fixed.One/4, fixed.One/2, fixed.One*3/4
	t1 := UpkeepTier{Food: 10}
	t1.Rate[0] = q // 25% at food 10..49
	t2 := UpkeepTier{Food: 50}
	t2.Rate[0] = h // 50% at food 50..99
	t3 := UpkeepTier{Food: 100}
	t3.Rate[0] = tq // 75% at food >=100
	if !w.BindUpkeep([]UpkeepTier{t1, t2, t3}) {
		t.Fatal("BindUpkeep failed")
	}
	cases := []struct {
		food int32
		want fixed.F64
	}{
		{0, 0},    // below first bracket
		{9, 0},    // just below first
		{10, q},   // at first
		{49, q},   // top of first
		{50, h},   // at second
		{99, h},   // top of second
		{100, tq}, // at third
		{500, tq}, // above all
	}
	for _, c := range cases {
		w.foodUsed[0] = c.food
		got := w.UpkeepRate(0, 0)
		t.Logf("FSV food=%-3d rate=%#x (want %#x)", c.food, uint64(got), uint64(c.want))
		if got != c.want {
			t.Fatalf("food=%d rate=%#x want %#x", c.food, uint64(got), uint64(c.want))
		}
	}
}

// TestBindUpkeepRejectsNonAscendingFSV — edge: a non-ascending tier list is
// refused and leaves the table unchanged.
func TestBindUpkeepRejectsNonAscendingFSV(t *testing.T) {
	w := econWorld(t)
	good := UpkeepTier{Food: 10}
	good.Rate[0] = fixed.One / 2
	if !w.BindUpkeep([]UpkeepTier{good}) {
		t.Fatal("valid bind refused")
	}
	bad := []UpkeepTier{{Food: 50}, {Food: 50}} // equal → not strictly ascending
	t.Logf("FSV before: upkeepCount=%d", w.upkeepCount)
	if w.BindUpkeep(bad) {
		t.Fatal("non-ascending bind accepted")
	}
	t.Logf("FSV after refused bind: upkeepCount=%d (want 1, unchanged)", w.upkeepCount)
	if w.upkeepCount != 1 {
		t.Fatalf("refused bind mutated table: count=%d", w.upkeepCount)
	}
}

// TestTransferResourceTaxFSV — inter-player transfer tax. p0 gives 50 gold
// to p1 at a 50% tax (exactly representable in Q32.32). SoT: both ledgers.
// p0 -= 50, p1 += 25, 25 destroyed.
func TestTransferResourceTaxFSV(t *testing.T) {
	w := econWorld(t)
	w.SetResource(0, 0, 100)
	w.SetTaxRate(0, 1, 0, fixed.One/2) // 50%
	t.Logf("FSV before: p0=%d p1=%d taxRate=%#x", w.Resources(0, 0), w.Resources(1, 0), uint64(w.TaxRate(0, 1, 0)))

	delivered := w.TransferResource(0, 1, 0, 50)
	p0, p1 := w.Resources(0, 0), w.Resources(1, 0)
	t.Logf("FSV after: delivered=%d p0=%d p1=%d (want 25/50/25)", delivered, p0, p1)
	if delivered != 25 || p0 != 50 || p1 != 25 {
		t.Fatalf("transfer wrong: delivered=%d p0=%d p1=%d", delivered, p0, p1)
	}
	// conservation: debited(50) == delivered(25) + destroyed(25)
	if (100-p0) != delivered+25 {
		t.Fatalf("conservation broken: debited=%d delivered=%d", 100-p0, delivered)
	}
}

// TestTransferResourceEdgesFSV — transfer edge cases against the ledger SoT.
func TestTransferResourceEdgesFSV(t *testing.T) {
	w := econWorld(t)

	// edge: zero tax → full delivery.
	w.SetResource(0, 0, 80)
	if d := w.TransferResource(0, 1, 0, 30); d != 30 || w.Resources(1, 0) != 30 {
		t.Fatalf("zero-tax transfer: delivered=%d p1=%d want 30/30", d, w.Resources(1, 0))
	}
	t.Logf("FSV zero-tax: p0=%d p1=%d delivered=30", w.Resources(0, 0), w.Resources(1, 0))

	// edge: amount over balance clamps to available.
	w2 := econWorld(t)
	w2.SetResource(0, 0, 40)
	d := w2.TransferResource(0, 1, 0, 1000)
	t.Logf("FSV over-balance: requested 1000 avail 40 -> delivered=%d p0=%d p1=%d", d, w2.Resources(0, 0), w2.Resources(1, 0))
	if d != 40 || w2.Resources(0, 0) != 0 || w2.Resources(1, 0) != 40 {
		t.Fatalf("over-balance clamp wrong: d=%d p0=%d p1=%d", d, w2.Resources(0, 0), w2.Resources(1, 0))
	}

	// edge: self-transfer is a no-op.
	w3 := econWorld(t)
	w3.SetResource(0, 0, 50)
	if d := w3.TransferResource(0, 0, 0, 20); d != 0 || w3.Resources(0, 0) != 50 {
		t.Fatalf("self-transfer mutated: d=%d p0=%d", d, w3.Resources(0, 0))
	}
	t.Logf("FSV self-transfer: no-op p0=%d", w3.Resources(0, 0))

	// edge: 100% tax → receiver gets nothing, giver still debited.
	w4 := econWorld(t)
	w4.SetResource(0, 0, 60)
	w4.SetTaxRate(0, 1, 0, fixed.One)
	d4 := w4.TransferResource(0, 1, 0, 60)
	t.Logf("FSV full-tax: delivered=%d p0=%d p1=%d (want 0/0/0)", d4, w4.Resources(0, 0), w4.Resources(1, 0))
	if d4 != 0 || w4.Resources(0, 0) != 0 || w4.Resources(1, 0) != 0 {
		t.Fatalf("full-tax wrong: d=%d p0=%d p1=%d", d4, w4.Resources(0, 0), w4.Resources(1, 0))
	}
}

// TestUpkeepSaveRoundTripFSV — upkeep brackets, lost counters, and the tax
// matrix survive save(v25)→load and the full-World hash matches. SoT: the
// reloaded values + the state hash.
func TestUpkeepSaveRoundTripFSV(t *testing.T) {
	w := econWorld(t)
	tier := UpkeepTier{Food: 7}
	tier.Rate[0] = fixed.One / 2
	tier.Rate[1] = fixed.One / 4
	if !w.BindUpkeep([]UpkeepTier{tier}) {
		t.Fatal("BindUpkeep failed")
	}
	w.upkeepLost[0][0] = 123
	w.upkeepLost[2][1] = 77
	w.SetTaxRate(0, 1, 0, fixed.One*3/10)
	w.SetTaxRate(3, 5, 1, fixed.One/8)

	reg := NewHashRegistry()
	var before statehash.Snapshot
	w.HashState(reg, &before)

	var buf bytes.Buffer
	const fp = 0xABCD
	if err := w.SaveState(&buf, fp); err != nil {
		t.Fatalf("save: %v", err)
	}
	w2 := econWorld(t)
	if err := w2.LoadState(bytes.NewReader(buf.Bytes()), fp); err != nil {
		t.Fatalf("load: %v", err)
	}
	var after statehash.Snapshot
	w2.HashState(reg, &after)

	t.Logf("FSV reload: count=%d food0=%d rate00=%#x lost00=%d lost21=%d tax010=%#x tax351=%#x",
		w2.upkeepCount, w2.upkeepFood[0], uint64(w2.upkeepRate[0][0]),
		w2.upkeepLost[0][0], w2.upkeepLost[2][1], uint64(w2.TaxRate(0, 1, 0)), uint64(w2.TaxRate(3, 5, 1)))
	t.Logf("FSV hash: orig=%016x reload=%016x", before.Top, after.Top)

	if w2.upkeepCount != 1 || w2.upkeepFood[0] != 7 ||
		w2.upkeepRate[0][0] != fixed.One/2 || w2.upkeepRate[0][1] != fixed.One/4 {
		t.Fatalf("brackets not restored: count=%d food=%d", w2.upkeepCount, w2.upkeepFood[0])
	}
	if w2.upkeepLost[0][0] != 123 || w2.upkeepLost[2][1] != 77 {
		t.Fatalf("lost counters not restored: %d %d", w2.upkeepLost[0][0], w2.upkeepLost[2][1])
	}
	if w2.TaxRate(0, 1, 0) != fixed.One*3/10 || w2.TaxRate(3, 5, 1) != fixed.One/8 {
		t.Fatalf("tax matrix not restored")
	}
	if before.Top != after.Top {
		t.Fatalf("post-load hash mismatch: %016x vs %016x", before.Top, after.Top)
	}
}

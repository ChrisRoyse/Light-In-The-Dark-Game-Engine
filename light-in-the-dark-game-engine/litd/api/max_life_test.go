package litd

// Max-life buff-stat FSV (#522 stat + #523 WC3 current-rises policy). SoT =
// Unit.MaxLife()/Life()/LifePercent() via the public api, and the regen system.
//
// Current-rises model (#523, WC3-accurate): applying a +max-life mod raises the
// CAP and current life by the same amount (a full unit stays full); removing it
// lowers the cap and current life by the bonus, floored at 1 so a max drop never
// kills. Regen/heal still fill toward the buffed cap. The applied bonus is
// reconstructed on load (not serialized), so a save/load round-trip of a damaged
// buffed unit must NOT double-apply the bonus.

import (
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
)

// maxLifeWorld binds a "bigbody" buff whose max-life Add is +addLife integer
// life points (converted to fixed bits, the Healths.MaxLife unit).
func maxLifeWorld(t *testing.T, addLife int64) (*sim.World, *Game) {
	t.Helper()
	w := sim.NewWorld(sim.Caps{Units: 8})
	defs := []data.BuffType{
		{ID: "bigbody", DurationTicks: 10000, Stacking: data.StackRefresh, MaxStacks: 1,
			Mods: []data.StatMod{{Stat: data.StatMaxLife, Add: addLife << 32, Permille: 1000}}},
	}
	if !w.BindBuffTypes(defs) {
		t.Fatal("BindBuffTypes failed")
	}
	return w, newGame(w)
}

// TestMaxLifeCurrentRisesFSV: a full unit stays full when the cap lifts, and
// loses the bonus current when it drops (#523).
func TestMaxLifeCurrentRisesFSV(t *testing.T) {
	w, g := maxLifeWorld(t, 50) // +50 max life
	big := g.BuffType("bigbody")
	if big.IsZero() {
		t.Fatal("BuffType(bigbody) null")
	}
	u := regenUnit(t, w, g, 64, fixed.FromInt(100), 0) // 100/100, no regen

	t.Logf("BASE: %.0f/%.0f pct=%.1f", u.Life(), u.MaxLife(), u.LifePercent())
	if u.Life() != 100 || u.MaxLife() != 100 {
		t.Fatalf("base = %.0f/%.0f, want 100/100", u.Life(), u.MaxLife())
	}

	// Apply +50: full unit RISES to stay full at the new cap.
	u.ApplyBuff(big)
	t.Logf("AFTER +50: %.0f/%.0f pct=%.1f (want 150/150/100)", u.Life(), u.MaxLife(), u.LifePercent())
	if u.Life() != 150 || u.MaxLife() != 150 {
		t.Fatalf("current did not rise with cap: %.0f/%.0f, want 150/150 (#523)", u.Life(), u.MaxLife())
	}
	if u.LifePercent() != 100 {
		t.Fatalf("full unit not 100%%: %.1f", u.LifePercent())
	}

	// Damage by 30 → 120/150, then remove: cap AND current drop by the 50 bonus.
	u.SetLife(120)
	u.RemoveBuff(big)
	t.Logf("damaged 120 then remove: %.0f/%.0f (want 70/100)", u.Life(), u.MaxLife())
	if u.MaxLife() != 100 || u.Life() != 70 {
		t.Fatalf("remove did not drop current by bonus: %.0f/%.0f, want 70/100", u.Life(), u.MaxLife())
	}
}

// TestMaxLifeRemoveFloorsAtOneFSV: dropping a +max-life buff can never kill —
// current life floors at 1, not 0 (#523, WC3 semantics).
func TestMaxLifeRemoveFloorsAtOneFSV(t *testing.T) {
	w, g := maxLifeWorld(t, 50)
	big := g.BuffType("bigbody")
	u := regenUnit(t, w, g, 64, fixed.FromInt(100), 0)

	u.ApplyBuff(big)   // 150/150
	u.SetLife(10)      // 10/150 — badly hurt
	u.RemoveBuff(big)  // bonus -50 would take 10→-40
	t.Logf("life 10 then remove +50: %.0f/%.0f (want 1/100 — never kills)", u.Life(), u.MaxLife())
	if u.Life() != 1 {
		t.Fatalf("max-loss should floor at 1, got %.0f (revive/kill bug)", u.Life())
	}
	if !u.Alive() {
		t.Fatal("unit killed by max-life loss — WC3 says it must survive at 1")
	}
}

// TestMaxLifeRegenFillsToBuffedCapFSV: regen heals into headroom opened below the
// buffed cap and saturates there (proves regen.go folds the buffed cap).
func TestMaxLifeRegenFillsToBuffedCapFSV(t *testing.T) {
	w, g := maxLifeWorld(t, 50)
	big := g.BuffType("bigbody")
	u := regenUnit(t, w, g, 64, fixed.FromInt(100), fixed.FromInt(2)) // 100/100, regen 2/tick

	u.ApplyBuff(big) // rises to 150/150 (full) — regen idle
	u.SetLife(130)   // damage into the buffed pool → 130/150
	step(w, 10)      // 2/tick × 10 = +20 → 150
	t.Logf("130 + regen 10 ticks: %.0f (want 150)", u.Life())
	if u.Life() != 150 {
		t.Fatalf("regen did not fill to buffed cap: %.0f, want 150", u.Life())
	}
	step(w, 5) // already full — must not overshoot
	if u.Life() != 150 {
		t.Fatalf("regen overshot buffed cap: %.0f, want 150", u.Life())
	}
}

func TestMaxLifeDeterminismFSV(t *testing.T) {
	run := func() uint64 {
		w, g := maxLifeWorld(t, 40)
		big := g.BuffType("bigbody")
		u := regenUnit(t, w, g, 64, fixed.FromInt(100), fixed.FromInt(1))
		u.ApplyBuff(big)
		u.SetLife(130)
		step(w, 5)
		return g.StateHash()
	}
	h1, h2 := run(), run()
	if h1 != h2 {
		t.Fatalf("non-deterministic max-life: h1=%#x h2=%#x", h1, h2)
	}
	t.Logf("FSV #523 determinism: two max-life runs identical hash=%#x", h1)
}

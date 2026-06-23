package litd

// Max-life buff-stat FSV (#522). SoT = Unit.MaxLife()/Life()/LifePercent() via
// the public api, and the regen system's fill. Proves a +max-life mod raises the
// effective cap at every read site, lets regen/heal/SetLife fill into the larger
// pool, and — the subtle edge — clamps current life back down when the mod is
// removed (a dropped +max buff must not strand life above the base cap).
//
// Model is cap-only (consistent with max-mana): applying +max-life raises the
// CAP; current life is unchanged and fills via regen/heal. The WC3 "current
// rises with the cap" policy is tracked separately (see #523).

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

func TestMaxLifeBuffFSV(t *testing.T) {
	w, g := maxLifeWorld(t, 50) // +50 max life
	big := g.BuffType("bigbody")
	if big.IsZero() {
		t.Fatal("BuffType(bigbody) null")
	}
	u := regenUnit(t, w, g, 64, fixed.FromInt(100), 0) // MaxLife 100, no regen, starts full

	// Base: full at 100/100.
	t.Logf("BASE: maxLife=%.0f life=%.0f pct=%.1f", u.MaxLife(), u.Life(), u.LifePercent())
	if u.MaxLife() != 100 || u.Life() != 100 {
		t.Fatalf("base maxLife/life = %.0f/%.0f, want 100/100", u.MaxLife(), u.Life())
	}

	// Apply +50 → cap reads 150 immediately; current life unchanged (cap-only),
	// so the unit is now 100/150 = 66.7%.
	u.ApplyBuff(big)
	t.Logf("AFTER +50 buff: maxLife=%.0f life=%.0f pct=%.1f (want 150/100/66.7)", u.MaxLife(), u.Life(), u.LifePercent())
	if u.MaxLife() != 150 {
		t.Fatalf("MAX-LIFE MOD NOT READ: maxLife=%.0f, want 150 (#522 fail-open)", u.MaxLife())
	}
	if u.Life() != 100 {
		t.Fatalf("cap-only: life moved to %.0f, want 100", u.Life())
	}
	if p := u.LifePercent(); p < 66.6 || p > 66.7 {
		t.Fatalf("LifePercent not against buffed cap: %.2f, want ~66.67", p)
	}

	// SetLife fills into the larger pool and clamps at the buffed cap.
	u.SetLife(150)
	if u.Life() != 150 {
		t.Fatalf("fill to buffed cap: life=%.0f, want 150", u.Life())
	}
	u.SetLife(200)
	t.Logf("SetLife(200) with cap 150: life=%.0f (want clamp 150)", u.Life())
	if u.Life() != 150 {
		t.Fatalf("SetLife did not clamp to buffed cap: life=%.0f, want 150", u.Life())
	}

	// Remove the buff → cap reverts to 100 AND current life clamps 150→100.
	u.RemoveBuff(big)
	t.Logf("AFTER remove: maxLife=%.0f life=%.0f (want 100/100)", u.MaxLife(), u.Life())
	if u.MaxLife() != 100 {
		t.Fatalf("cap did not revert: maxLife=%.0f, want 100", u.MaxLife())
	}
	if u.Life() != 100 {
		t.Fatalf("OVER-CAP AFTER REMOVAL: life=%.0f left above base cap 100 — clamp-on-removal failed", u.Life())
	}
}

// TestMaxLifeRegenFillsToBuffedCapFSV: a +max-life buff opens headroom that the
// regen SYSTEM then heals into — proving regen.go folds the buffed cap, not the
// base. A full unit (regen idle) starts regenerating again the moment the cap
// lifts above its current life.
func TestMaxLifeRegenFillsToBuffedCapFSV(t *testing.T) {
	w, g := maxLifeWorld(t, 50)
	big := g.BuffType("bigbody")
	u := regenUnit(t, w, g, 64, fixed.FromInt(100), fixed.FromInt(2)) // full 100/100, regen 2/tick

	// Full at base cap → regen idle.
	step(w, 5)
	if u.Life() != 100 {
		t.Fatalf("at base cap regen should be idle: life=%.0f, want 100", u.Life())
	}

	// +50 cap → 100/150, regen resumes filling toward 150.
	u.ApplyBuff(big)
	t.Logf("BEFORE fill: life=%.0f cap=%.0f", u.Life(), u.MaxLife())
	step(w, 10) // 2/tick × 10 = +20
	t.Logf("AFTER 10 ticks: life=%.0f (want 120)", u.Life())
	if u.Life() != 120 {
		t.Fatalf("regen did not fill into buffed cap: life=%.0f, want 120", u.Life())
	}

	// Fill to the cap and confirm it saturates at 150, not the base 100.
	step(w, 20) // would reach 160 unclamped
	t.Logf("AFTER 30 ticks total: life=%.0f (want clamp 150)", u.Life())
	if u.Life() != 150 {
		t.Fatalf("regen overshoot/short of buffed cap: life=%.0f, want 150", u.Life())
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
	t.Logf("FSV #522 determinism: two max-life runs identical hash=%#x", h1)
}

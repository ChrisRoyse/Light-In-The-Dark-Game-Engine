package litd

// Life-regeneration FSV (#520 + the regen-system root cause it exposed). SoT =
// the unit's life via the public Unit.Life() read, before and after stepping the
// sim. Proves: base regen now actually heals (it was dead data), it clamps at
// MaxLife and never revives a corpse, a regen-less unit is untouched (the
// golden-safety property), and a life-regen BUFF mod folds in through BuffedRegen
// (so the new stat is not a fail-open) and reverts on removal.

import (
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
)

// regenWorld binds a "warmth" buff whose life-regen Add is addPerTick life/tick
// (in per-tick fixed bits — the same units BuffedRegen folds against).
func regenWorld(t *testing.T, addPerTick int64) (*sim.World, *Game) {
	t.Helper()
	w := sim.NewWorld(sim.Caps{Units: 8})
	defs := []data.BuffType{
		{ID: "warmth", DurationTicks: 10000, Stacking: data.StackRefresh, MaxStacks: 1,
			Mods: []data.StatMod{{Stat: data.StatLifeRegen, Add: addPerTick, Permille: 1000}}},
	}
	if !w.BindBuffTypes(defs) {
		t.Fatal("BindBuffTypes failed")
	}
	return w, newGame(w)
}

// regenUnit spawns a unit at full life with the given per-tick base regen.
func regenUnit(t *testing.T, w *sim.World, g *Game, x int32, maxLife, regenPerTick fixed.F64) Unit {
	t.Helper()
	id, ok := w.CreateUnit(fixed.Vec2{X: fixed.FromInt(x), Y: fixed.FromInt(64)}, 0)
	if !ok || !w.Healths.Add(w.Ents, id, maxLife, regenPerTick, 0, 0) {
		t.Fatal("regenUnit failed")
	}
	return Unit{id: id, g: g}
}

func step(w *sim.World, n int) {
	for i := 0; i < n; i++ {
		w.Step()
	}
}

// TestBaseRegenHealsAndClampsFSV: base regen now heals (it never did before), and
// life saturates at MaxLife — no overshoot.
func TestBaseRegenHealsAndClampsFSV(t *testing.T) {
	w, g := regenWorld(t, 0)
	u := regenUnit(t, w, g, 64, fixed.FromInt(100), fixed.FromInt(2)) // 2 life/tick

	u.SetLife(50)
	t.Logf("BEFORE: life=%.0f (regen 2/tick)", u.Life())
	if u.Life() != 50 {
		t.Fatalf("setup: life=%.0f want 50", u.Life())
	}

	step(w, 10)
	t.Logf("AFTER 10 ticks: life=%.0f (want 70)", u.Life())
	if u.Life() != 70 {
		t.Fatalf("REGEN NOT APPLIED: life=%.0f after 10 ticks at 2/tick, want 70 (regen was dead data before #520)", u.Life())
	}

	step(w, 100) // would reach 50+220=270 unclamped
	t.Logf("AFTER 110 ticks: life=%.0f (want clamp at 100)", u.Life())
	if u.Life() != 100 {
		t.Fatalf("CLAMP FAILED: life=%.0f, want MaxLife 100", u.Life())
	}
}

// TestRegenNeverRevivesFSV: a unit killed (life 0) stays dead — regen must skip
// corpses, never heal a unit back from death.
func TestRegenNeverRevivesFSV(t *testing.T) {
	w, g := regenWorld(t, 0)
	u := regenUnit(t, w, g, 64, fixed.FromInt(100), fixed.FromInt(5)) // strong regen

	t.Logf("BEFORE kill: alive=%v life=%.0f", u.Alive(), u.Life())
	u.SetLife(0) // lethal
	step(w, 5)
	t.Logf("AFTER kill + 5 ticks: alive=%v life=%.0f (want dead, life 0)", u.Alive(), u.Life())
	if u.Alive() || u.Life() != 0 {
		t.Fatalf("CORPSE REVIVED: alive=%v life=%.0f — regen healed a dead unit", u.Alive(), u.Life())
	}
}

// TestRegenlessUnitUntouchedFSV: a unit with base regen 0 and no life-regen mod
// is never modified by the regen system. This is the property that keeps every
// regen-less determinism golden bit-identical.
func TestRegenlessUnitUntouchedFSV(t *testing.T) {
	w, g := regenWorld(t, 0)
	u := regenUnit(t, w, g, 64, fixed.FromInt(100), 0) // regen 0
	u.SetLife(50)
	step(w, 50)
	t.Logf("regen-less unit after 50 ticks: life=%.0f (want unchanged 50)", u.Life())
	if u.Life() != 50 {
		t.Fatalf("regen-less unit changed: life=%.0f, want 50 (golden-safety violated)", u.Life())
	}
}

// TestLifeRegenBuffAppliesFSV (#520 core): a life-regen BUFF heals a unit that has
// zero base regen — proving the new stat actually applies (not a fail-open) — and
// removing the buff stops the healing. Control sibling (no buff) stays put.
func TestLifeRegenBuffAppliesFSV(t *testing.T) {
	w, g := regenWorld(t, int64(3*fixed.One)) // warmth = +3 life/tick
	warmth := g.BuffType("warmth")
	if warmth.IsZero() {
		t.Fatal("BuffType(warmth) null")
	}
	u := regenUnit(t, w, g, 64, fixed.FromInt(100), 0)   // no base regen
	ctl := regenUnit(t, w, g, 200, fixed.FromInt(100), 0) // control: no buff ever
	u.SetLife(50)
	ctl.SetLife(50)

	// No buff yet → no heal for either.
	step(w, 10)
	t.Logf("no-buff 10 ticks: u.life=%.0f ctl.life=%.0f (want 50/50)", u.Life(), ctl.Life())
	if u.Life() != 50 || ctl.Life() != 50 {
		t.Fatalf("regen-0 units healed without a regen source: u=%.0f ctl=%.0f", u.Life(), ctl.Life())
	}

	// Apply warmth (+3/tick) to u only.
	u.ApplyBuff(warmth)
	step(w, 10)
	t.Logf("warmth 10 ticks: u.life=%.0f (want 80) ctl.life=%.0f (want 50)", u.Life(), ctl.Life())
	if u.Life() != 80 {
		t.Fatalf("LIFE-REGEN MOD IS A NO-OP: u.life=%.0f after 10 ticks of +3/tick warmth, want 80 (#520 fail-open)", u.Life())
	}
	if ctl.Life() != 50 {
		t.Fatalf("control healed without a buff: %.0f", ctl.Life())
	}

	// Remove warmth → healing stops (cache reverts to base 0).
	u.RemoveBuff(warmth)
	step(w, 10)
	t.Logf("after remove + 10 ticks: u.life=%.0f (want unchanged 80)", u.Life())
	if u.Life() != 80 {
		t.Fatalf("regen continued after buff removal: u.life=%.0f, want 80", u.Life())
	}
}

// TestRegenDeterminismFSV: two identical regen runs produce the same state hash.
func TestRegenDeterminismFSV(t *testing.T) {
	run := func() uint64 {
		w, g := regenWorld(t, int64(3*fixed.One))
		warmth := g.BuffType("warmth")
		u := regenUnit(t, w, g, 64, fixed.FromInt(100), fixed.FromInt(1))
		u.SetLife(40)
		u.ApplyBuff(warmth)
		step(w, 25)
		return g.StateHash()
	}
	h1, h2 := run(), run()
	if h1 != h2 {
		t.Fatalf("non-deterministic regen: h1=%#x h2=%#x", h1, h2)
	}
	t.Logf("FSV #520 determinism: two regen runs identical hash=%#x", h1)
}

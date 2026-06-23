package litd

// Max-mana buff-stat FSV (#522). SoT = Unit.MaxMana()/Mana() via the public api.
// Proves a +max-mana mod raises the effective cap (all read sites fold it), lets
// the pool fill higher, clamps SetMana to the buffed cap, and — the subtle edge —
// clamps current mana back down when the mod is REMOVED (a dropped +max buff must
// not leave the pool stranded over the base cap).

import (
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
)

func maxManaWorld(t *testing.T, addFixed int64) (*sim.World, *Game) {
	t.Helper()
	w := sim.NewWorld(sim.Caps{Units: 8, RuntimeAbilityDefs: 8})
	defs := []data.BuffType{
		{ID: "bigpool", DurationTicks: 10000, Stacking: data.StackRefresh, MaxStacks: 1,
			Mods: []data.StatMod{{Stat: data.StatMaxMana, Add: addFixed, Permille: 1000}}},
	}
	if !w.BindBuffTypes(defs) {
		t.Fatal("BindBuffTypes failed")
	}
	return w, newGame(w)
}

func TestMaxManaBuffFSV(t *testing.T) {
	w, g := maxManaWorld(t, int64(50)<<32) // +50 mana points
	big := g.BuffType("bigpool")
	if big.IsZero() {
		t.Fatal("BuffType(bigpool) null")
	}
	u := manaUnit(t, w, g, 64) // base MaxMana 100, Mana 0

	// Base cap.
	t.Logf("BASE: maxMana=%.0f", u.MaxMana())
	if u.MaxMana() != 100 {
		t.Fatalf("base MaxMana=%.0f, want 100", u.MaxMana())
	}
	u.SetMana(100)
	if u.Mana() != 100 {
		t.Fatalf("fill to base: mana=%.0f, want 100", u.Mana())
	}

	// Apply +50 max-mana → cap reads 150 immediately (recompute on apply).
	u.ApplyBuff(big)
	t.Logf("AFTER +50 buff: maxMana=%.0f (want 150)", u.MaxMana())
	if u.MaxMana() != 150 {
		t.Fatalf("MAX-MANA MOD NOT READ: maxMana=%.0f, want 150 (#522 fail-open)", u.MaxMana())
	}
	// Pool can now hold up to 150; SetMana past it clamps to the buffed cap.
	u.SetMana(150)
	if u.Mana() != 150 {
		t.Fatalf("fill to buffed cap: mana=%.0f, want 150", u.Mana())
	}
	u.SetMana(200)
	t.Logf("SetMana(200) with cap 150: mana=%.0f (want clamp 150)", u.Mana())
	if u.Mana() != 150 {
		t.Fatalf("SetMana did not clamp to buffed cap: mana=%.0f, want 150", u.Mana())
	}

	// Remove the buff → cap drops to 100 AND current mana clamps down to 100
	// (the clamp-on-removal edge).
	u.RemoveBuff(big)
	t.Logf("AFTER remove: maxMana=%.0f mana=%.0f (want 100/100)", u.MaxMana(), u.Mana())
	if u.MaxMana() != 100 {
		t.Fatalf("cap did not revert: maxMana=%.0f, want 100", u.MaxMana())
	}
	if u.Mana() != 100 {
		t.Fatalf("OVER-CAP AFTER REMOVAL: mana=%.0f left above base cap 100 — clamp-on-removal failed", u.Mana())
	}
}

func TestMaxManaDeterminismFSV(t *testing.T) {
	run := func() uint64 {
		w, g := maxManaWorld(t, int64(40)<<32)
		big := g.BuffType("bigpool")
		u := manaUnit(t, w, g, 64)
		u.ApplyBuff(big)
		u.SetMana(140)
		stepMana(w, 5)
		return g.StateHash()
	}
	h1, h2 := run(), run()
	if h1 != h2 {
		t.Fatalf("non-deterministic max-mana: h1=%#x h2=%#x", h1, h2)
	}
	t.Logf("FSV #522 determinism: two max-mana runs identical hash=%#x", h1)
}

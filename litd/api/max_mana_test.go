package litd

// Max-mana buff-stat FSV (#522 stat + #523 WC3 current-rises policy). SoT =
// Unit.MaxMana()/Mana() via the public api.
//
// Current-rises model (#523): a +max-mana mod raises the cap AND current mana by
// the same amount; removing it lowers both by the bonus, floored at 0. Reads fold
// the cap; SetMana clamps to it.

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

func TestMaxManaCurrentRisesFSV(t *testing.T) {
	w, g := maxManaWorld(t, int64(50)<<32) // +50 mana points
	big := g.BuffType("bigpool")
	if big.IsZero() {
		t.Fatal("BuffType(bigpool) null")
	}
	u := manaUnit(t, w, g, 64) // base MaxMana 100, Mana 0

	// Half-full to show the rise is by the bonus, not "to full".
	u.SetMana(40)
	t.Logf("BASE: %.0f/%.0f", u.Mana(), u.MaxMana())
	if u.Mana() != 40 || u.MaxMana() != 100 {
		t.Fatalf("base = %.0f/%.0f, want 40/100", u.Mana(), u.MaxMana())
	}

	// Apply +50: cap 100→150 AND current 40→90 (rises by the bonus).
	u.ApplyBuff(big)
	t.Logf("AFTER +50: %.0f/%.0f (want 90/150)", u.Mana(), u.MaxMana())
	if u.Mana() != 90 || u.MaxMana() != 150 {
		t.Fatalf("current did not rise with cap: %.0f/%.0f, want 90/150 (#523)", u.Mana(), u.MaxMana())
	}

	// Fill to the buffed cap; SetMana clamps there.
	u.SetMana(200)
	if u.Mana() != 150 {
		t.Fatalf("SetMana did not clamp to buffed cap: %.0f, want 150", u.Mana())
	}

	// Remove: cap 150→100 AND current 150→100 (drops by the bonus).
	u.RemoveBuff(big)
	t.Logf("AFTER remove (was full): %.0f/%.0f (want 100/100)", u.Mana(), u.MaxMana())
	if u.Mana() != 100 || u.MaxMana() != 100 {
		t.Fatalf("remove did not drop current by bonus: %.0f/%.0f, want 100/100", u.Mana(), u.MaxMana())
	}
}

// TestMaxManaRemoveFloorsAtZeroFSV: dropping the buff floors current mana at 0.
func TestMaxManaRemoveFloorsAtZeroFSV(t *testing.T) {
	w, g := maxManaWorld(t, int64(50)<<32)
	big := g.BuffType("bigpool")
	u := manaUnit(t, w, g, 64)

	u.ApplyBuff(big) // 0→50 / 150 (current rose by bonus from 0)
	u.SetMana(10)    // 10/150
	u.RemoveBuff(big) // 10 - 50 → floor 0
	t.Logf("mana 10 then remove +50: %.0f/%.0f (want 0/100)", u.Mana(), u.MaxMana())
	if u.Mana() != 0 {
		t.Fatalf("mana floor: %.0f, want 0", u.Mana())
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
	t.Logf("FSV #523 determinism: two max-mana runs identical hash=%#x", h1)
}

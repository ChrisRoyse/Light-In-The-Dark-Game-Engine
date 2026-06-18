package litd

// #394 install-seam FSV. The DefineEffects/Abilities/Items/BuffTypes/Upgrades
// setup verbs are thin wrappers over the sim's Bind* methods; this proves each
// actually installs its table by reading the sim's own registries
// (AbilityDefCount / BuffTypeID / ItemTypeID / the BindTech rebind-length guard)
// AFTER the call — never by trusting the method's nil error return (doctrine §0).

import (
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
)

func hfooDefs() []data.Unit {
	return []data.Unit{{ID: "hfoo", Life: 100, MoveSpeedPerTick: 8 * fixed.One, TurnRatePerTick: 65535, CollisionSize: 16}}
}

// TestDefineNonUnitTablesInstallFSV — happy path: each seam installs its table
// into the sim, verified against the sim registry the table feeds.
func TestDefineNonUnitTablesInstallFSV(t *testing.T) {
	w := sim.NewWorld(sim.Caps{Units: 8, RuntimeAbilityDefs: 8})
	g := newGame(w)
	if err := g.DefineUnits(hfooDefs()); err != nil { // upgrades depend on units
		t.Fatalf("DefineUnits: %v", err)
	}

	// --- Effects: empty arena binds cleanly (prerequisite for items/abilities). ---
	if err := g.DefineEffects(nil); err != nil {
		t.Fatalf("DefineEffects(nil): %v", err)
	}

	// --- Abilities: SoT = w.AbilityDefCount(). ---
	if n := w.AbilityDefCount(); n != 0 {
		t.Fatalf("ability defs BEFORE: %d, want 0", n)
	}
	if err := g.DefineAbilities([]data.Ability{{ID: "AHbz", Name: "Blizzard"}, {ID: "AHwe", Name: "Web"}}); err != nil {
		t.Fatalf("DefineAbilities: %v", err)
	}
	if n := w.AbilityDefCount(); n != 2 {
		t.Fatalf("ability defs AFTER: %d, want 2", n)
	}
	t.Logf("FSV abilities: AbilityDefCount 0 -> 2 (read from sim registry)")

	// Behavioral cross-check: refs 1,2 are grantable; ref 3 is unknown.
	id, ok := w.CreateUnit(fixed.Vec2{X: fixed.FromInt(64), Y: fixed.FromInt(64)}, 0)
	if !ok {
		t.Fatal("CreateUnit failed")
	}
	u := Unit{id: id, g: g}
	if !u.AddAbility(AbilityRef(1)).Valid() || !u.AddAbility(AbilityRef(2)).Valid() {
		t.Fatal("AddAbility(1|2) must resolve against the installed defs")
	}
	if u.AddAbility(AbilityRef(3)).Valid() {
		t.Fatal("AddAbility(3) must fail — only 2 defs installed")
	}

	// --- BuffTypes: SoT = w.BuffTypeID(code). ---
	if _, ok := w.BuffTypeID("BFro"); ok {
		t.Fatal("buff BFro resolvable BEFORE define")
	}
	if err := g.DefineBuffTypes([]data.BuffType{{ID: "BFro"}}); err != nil {
		t.Fatalf("DefineBuffTypes: %v", err)
	}
	if _, ok := w.BuffTypeID("BFro"); !ok {
		t.Fatal("buff BFro NOT resolvable AFTER define")
	}
	t.Logf("FSV buffs: BuffTypeID(BFro) absent -> present")

	// --- Items: SoT = w.ItemTypeID(code) / w.ItemTypeCount(). ---
	if _, ok := w.ItemTypeID("ratf"); ok {
		t.Fatal("item ratf resolvable BEFORE define")
	}
	if err := g.DefineItems([]data.Item{{ID: "ratf"}}); err != nil {
		t.Fatalf("DefineItems: %v", err)
	}
	if _, ok := w.ItemTypeID("ratf"); !ok {
		t.Fatal("item ratf NOT resolvable AFTER define")
	}
	t.Logf("FSV items: ItemTypeID(ratf) absent -> present, count=%d", w.ItemTypeCount())

	// --- Upgrades: SoT = the BindTech rebind-length guard, which reads the
	// stored upgradeDefs slice. Install 1, then a 2-length rebind must be
	// rejected (proving len-1 is stored), and a same-length rebind accepted. ---
	if err := g.DefineUpgrades([]data.Upgrade{{ID: "Rhme"}}, nil); err != nil {
		t.Fatalf("DefineUpgrades(1): %v", err)
	}
	if err := g.DefineUpgrades([]data.Upgrade{{ID: "Rhme"}, {ID: "Rhde"}}, nil); err == nil {
		t.Fatal("DefineUpgrades(2) after (1) must fail the rebind-length guard")
	}
	if err := g.DefineUpgrades([]data.Upgrade{{ID: "Rhme"}}, nil); err != nil {
		t.Fatalf("DefineUpgrades(1) same-length rebind: %v", err)
	}
	t.Logf("FSV upgrades: stored len=1 confirmed via rebind-length guard")
}

// TestDefineEconomyEnablesResourcesFSV — #396 economy seam, and the explicit
// path for #388: SetGold no-ops until the economy is bound, then writes. SoT =
// the sim per-player resource ledger (w.Resources), read directly.
func TestDefineEconomyEnablesResourcesFSV(t *testing.T) {
	w := sim.NewWorld(sim.Caps{Units: 4})
	g := newGame(w)
	p := g.Player(0)

	// BEFORE economy: SetGold is a no-op (the #388 behaviour).
	p.SetGold(500)
	if got := p.Gold(); got != 0 {
		t.Fatalf("Gold BEFORE DefineEconomy = %d, want 0 (no-op until bound)", got)
	}
	if got := w.Resources(0, 0); got != 0 {
		t.Fatalf("sim resource[0][0] BEFORE = %d, want 0", got)
	}

	if err := g.DefineEconomy(2); err != nil {
		t.Fatalf("DefineEconomy(2): %v", err)
	}

	// AFTER: SetGold writes; verify against the sim ledger, not Gold()'s return.
	p.SetGold(500)
	if got := w.Resources(0, 0); got != 500 {
		t.Fatalf("sim resource[0][0] AFTER = %d, want 500", got)
	}
	if got := p.Gold(); got != 500 {
		t.Fatalf("Gold AFTER = %d, want 500", got)
	}
	t.Logf("FSV economy: SetGold no-op (ledger 0) before DefineEconomy; writes (ledger 500) after")

	// Edges.
	if err := (*Game)(nil).DefineEconomy(2); err == nil {
		t.Error("DefineEconomy on nil game must error")
	}
	if err := g.DefineEconomy(0); err == nil {
		t.Error("DefineEconomy(0) must error (non-positive)")
	}
	if err := g.DefineEconomy(3); err == nil {
		t.Error("DefineEconomy(3) after (2) must fail the conflicting-rebind guard")
	}
	if err := g.DefineEconomy(2); err != nil {
		t.Errorf("DefineEconomy(2) idempotent rebind: %v", err)
	}
}

// TestDefineSeamsFailClosedFSV — edge audit: every seam fails closed (errors,
// installs nothing) on invalid input, rather than silently no-opping.
func TestDefineSeamsFailClosedFSV(t *testing.T) {
	// Edge 1 — nil game.
	if err := (*Game)(nil).DefineAbilities(nil); err == nil {
		t.Error("DefineAbilities on nil game must error")
	}

	// Edge 2 — DefineUpgrades before DefineUnits: BindTech consults w.unitDefs,
	// so this proves the seam reads sim state rather than blindly succeeding.
	w := sim.NewWorld(sim.Caps{Units: 8})
	g := newGame(w)
	if err := g.DefineUpgrades([]data.Upgrade{{ID: "Rhme"}}, nil); err == nil {
		t.Error("DefineUpgrades before DefineUnits must fail (units not defined)")
	}

	// Edge 3 — empty item table is rejected.
	if err := g.DefineItems(nil); err == nil {
		t.Error("DefineItems(nil) must fail (empty table)")
	}

	// Edge 4 — item references the effect arena out of range (no effects bound).
	bad := []data.Item{{ID: "bad", Effects: data.EffectList{Off: 0, Len: 1}}}
	if err := g.DefineItems(bad); err == nil {
		t.Error("DefineItems with out-of-range effect list must fail")
	}
	if n := w.ItemTypeCount(); n != 0 {
		t.Errorf("failed DefineItems calls still installed %d items — not fail-closed", n)
	}

	// Edge 5 — effect arena with an out-of-range primitive is rejected.
	if err := g.DefineEffects([]data.CompiledEffect{{Prim: data.EffectPrimCount}}); err == nil {
		t.Error("DefineEffects with out-of-range primitive must fail")
	}
	t.Logf("FSV fail-closed: nil-game, upgrades-before-units, empty-items, bad-effect-range, bad-primitive all errored; 0 items installed")
}

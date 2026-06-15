package litd

// #234 abilities + buffs public-API FSV. SoT = the per-unit ability field
// override store (w.AbilityFields), the buff pool (w.Buffs via the world
// accessors), and the state hash. Covers the issue's flagship copy-on-write
// edge and the buff lifecycle/determinism.

import (
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/statehash"
)

// TestAbilityFieldCopyOnWriteFSV — issue edge (1): SetField on one unit's
// ability instance must not touch a second unit's same ability. The override
// store is keyed per (unit, slot), so the write is structurally isolated.
func TestAbilityFieldCopyOnWriteFSV(t *testing.T) {
	w := sim.NewWorld(sim.Caps{Units: 8, RuntimeAbilityDefs: 8})
	g := newGame(w)
	mk := func(x int32) Unit {
		id, ok := w.CreateUnit(fixed.Vec2{X: fixed.FromInt(x), Y: fixed.FromInt(64)}, 0)
		if !ok {
			t.Fatal("CreateUnit failed")
		}
		return Unit{id: id, g: g}
	}
	u1, u2 := mk(64), mk(128)
	ref := g.RegisterAbility(AbilityDef{ID: "api-cow", Name: "Twin", ManaCost: 10, Cooldown: 2.0})
	if ref == 0 {
		t.Fatal("RegisterAbility failed")
	}
	a1, a2 := u1.AddAbility(ref), u2.AddAbility(ref)
	if !a1.Valid() || !a2.Valid() {
		t.Fatal("AddAbility failed")
	}

	base1, base2 := a1.Cooldown(), a2.Cooldown()
	_, ok1 := w.AbilityFields.Get(u1.id, 0, sim.AbilityFieldCooldown)
	_, ok2 := w.AbilityFields.Get(u2.id, 0, sim.AbilityFieldCooldown)
	t.Logf("FSV BEFORE SetField: a1.cd=%v a2.cd=%v override1=%v override2=%v (want equal, no overrides)", base1, base2, ok1, ok2)
	if base1 != base2 || ok1 || ok2 {
		t.Fatalf("baseline diverged or stray override: %v/%v ov %v/%v", base1, base2, ok1, ok2)
	}

	a1.SetField(AbilityFieldCooldown, 9.0)

	after1, after2 := a1.Cooldown(), a2.Cooldown()
	raw1, has1 := w.AbilityFields.Get(u1.id, 0, sim.AbilityFieldCooldown)
	raw2, has2 := w.AbilityFields.Get(u2.id, 0, sim.AbilityFieldCooldown)
	t.Logf("FSV AFTER SetField(a1,9): a1.cd=%v a2.cd=%v u1.override=(%v,%d) u2.override=(%v,%d)",
		after1, after2, has1, int64(raw1), has2, int64(raw2))
	if after1 != 9.0 {
		t.Fatalf("a1 cooldown not written: %v", after1)
	}
	if after2 != base2 || has2 {
		t.Fatalf("copy-on-write violated: u2 cooldown=%v override=%v (want %v / false)", after2, has2, base2)
	}
	if !has1 {
		t.Fatal("u1 override missing from SoT")
	}
}

func buffAPIWorld(t *testing.T) (*sim.World, *Game) {
	t.Helper()
	w := sim.NewWorld(sim.Caps{Units: 8})
	defs := []data.BuffType{
		{ID: "slow", DurationTicks: 100, Stacking: data.StackCount, MaxStacks: 3,
			Mods: []data.StatMod{{Stat: data.StatArmor, Add: -2, Permille: 1000}}},
	}
	if !w.BindBuffTypes(defs) {
		t.Fatal("BindBuffTypes failed")
	}
	return w, newGame(w)
}

func buffAPIUnit(t *testing.T, w *sim.World, g *Game, x int32) Unit {
	t.Helper()
	id, ok := w.CreateUnit(fixed.Vec2{X: fixed.FromInt(x), Y: fixed.FromInt(64)}, 0)
	if !ok || !w.Healths.Add(w.Ents, id, fixed.FromInt(100), 0, 5, 0) {
		t.Fatal("buffAPIUnit failed")
	}
	return Unit{id: id, g: g}
}

// TestBuffLifecycleFSV — ApplyBuff writes a real pooled instance; the getters
// read it back; stacking, survival across a tick, and removal all reflect in
// the buff-pool SoT. The "survives a step with a stable handle" path is the
// metamorphosis-proxy (entity identity preserved).
func TestBuffLifecycleFSV(t *testing.T) {
	w, g := buffAPIWorld(t)
	u := buffAPIUnit(t, w, g, 64)
	typ := g.BuffType("slow")
	if typ.IsZero() {
		t.Fatal("BuffType(slow) null")
	}

	t.Logf("FSV before: HasBuff=%v BuffCount=%d (want false/0)", u.HasBuff(typ), u.BuffCount())
	if u.HasBuff(typ) || u.BuffCount() != 0 {
		t.Fatal("unit should start buff-free")
	}

	b := u.ApplyBuff(typ)
	t.Logf("FSV apply1: valid=%v present=%v stacks=%d remaining=%.2fs count=%d",
		b.Valid(), b.Present(), b.Stacks(), b.RemainingSeconds(), u.BuffCount())
	if !b.Valid() || !b.Present() || b.Stacks() != 1 || !u.HasBuff(typ) || u.BuffCount() != 1 {
		t.Fatal("apply did not write a live instance")
	}
	if b.RemainingSeconds() <= 0 {
		t.Fatal("fresh buff should have positive remaining duration")
	}

	// StackCount: reapply increments stacks.
	u.ApplyBuff(typ, WithStacks(1))
	t.Logf("FSV apply2: stacks=%d (want 2, StackCount rule)", b.Stacks())
	if b.Stacks() != 2 {
		t.Fatalf("stack rule failed: %d", b.Stacks())
	}

	// survive a tick: handle stays valid, instance still present, duration ticks down.
	remBefore := b.RemainingSeconds()
	w.Step()
	t.Logf("FSV after step: valid=%v present=%v remaining %.2f->%.2f", b.Valid(), b.Present(), remBefore, b.RemainingSeconds())
	if !b.Valid() || !b.Present() || b.RemainingSeconds() >= remBefore {
		t.Fatal("buff should survive the tick with a decremented duration")
	}

	// removal clears the pool rows.
	if !u.RemoveBuff(typ) {
		t.Fatal("RemoveBuff should report a removal")
	}
	t.Logf("FSV after remove: HasBuff=%v count=%d present=%v (want false/0/false)", u.HasBuff(typ), u.BuffCount(), b.Present())
	if u.HasBuff(typ) || u.BuffCount() != 0 || b.Present() {
		t.Fatal("removal did not clear the buff")
	}

	// edge: zero/null no-ops.
	var zero Buff
	if zero.Stacks() != 0 || zero.Present() || zero.Valid() {
		t.Fatal("zero-value Buff must be a safe no-op")
	}
	if u.ApplyBuff(BuffType{}).Valid() {
		t.Fatal("ApplyBuff(null) must return invalid handle")
	}
}

// TestBuffAPIDeterminismFSV — two identical buff scripts hash identically.
func TestBuffAPIDeterminismFSV(t *testing.T) {
	run := func() uint64 {
		w, g := buffAPIWorld(t)
		u := buffAPIUnit(t, w, g, 64)
		typ := g.BuffType("slow")
		u.ApplyBuff(typ)
		u.ApplyBuff(typ, WithStacks(2))
		w.Step()
		reg := sim.NewHashRegistry()
		var snap statehash.Snapshot
		w.HashState(reg, &snap)
		return snap.Top
	}
	a, b := run(), run()
	t.Logf("FSV buff determinism: run1=%016x run2=%016x", a, b)
	if a != b {
		t.Fatalf("buff scripts diverged: %016x vs %016x", a, b)
	}
}

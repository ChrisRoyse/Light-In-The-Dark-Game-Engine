package sim

import (
	"testing"
	"unsafe"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
)

// Per-row byte sums vs the ecs §5.1 estimates (~160 B Combat at 2
// weapon slots; the estimate is an upper bound, the real sum prints).
func TestStoreCombatRowBytes(t *testing.T) {
	c := NewCombatStore(1, 1)
	combat := int(unsafe.Sizeof(c.DmgBase[0]) + unsafe.Sizeof(c.DmgDice[0]) + unsafe.Sizeof(c.DmgSides[0]) +
		unsafe.Sizeof(c.AttackType[0]) + unsafe.Sizeof(c.Cooldown[0]) + unsafe.Sizeof(c.DamagePt[0]) +
		unsafe.Sizeof(c.Range[0]) + unsafe.Sizeof(c.ProjRef[0]) + unsafe.Sizeof(c.ReadyAt[0]) +
		unsafe.Sizeof(c.AcquisitionRange[0]) + unsafe.Sizeof(c.Target[0]) + unsafe.Sizeof(c.LastAttacker[0]) +
		unsafe.Sizeof(c.LastDamagedTick[0]) + unsafe.Sizeof(c.Entity[0]))
	a := NewAbilityStore(1, 1)
	ability := int(unsafe.Sizeof(a.AbilityID[0]) + unsafe.Sizeof(a.Level[0]) + unsafe.Sizeof(a.ReadyAt[0]) +
		unsafe.Sizeof(a.ManaCostRef[0]) + unsafe.Sizeof(a.CastState[0]) + unsafe.Sizeof(a.Entity[0]))
	inv := NewInventoryStore(1, 1)
	invB := int(unsafe.Sizeof(inv.Slots[0]) + unsafe.Sizeof(inv.Entity[0]))
	o := NewOrderStore(1, 1)
	orderB := int(unsafe.Sizeof(o.Kind[0]) + unsafe.Sizeof(o.Target[0]) + unsafe.Sizeof(o.Point[0]) +
		unsafe.Sizeof(o.QueueHead[0]) + unsafe.Sizeof(o.Entity[0]))

	t.Logf("Combat row bytes (2 weapon slots): %d (ecs §5.1 estimate ~160, upper bound)", combat)
	t.Logf("Ability row bytes (%d slots): %d", AbilitySlots, ability)
	t.Logf("Inventory row bytes (%d slots): %d", InventorySlots, invB)
	t.Logf("Order-head row bytes: %d", orderB)
	if combat > 160 {
		t.Fatalf("Combat row %d B exceeds the §5.1 budget of ~160", combat)
	}
}

// Edge 1: weapon slot 2 unused — the zero-valued slot is legal and
// reads back as "no weapon" while slot 1 works.
func TestStoreCombatUnusedWeaponSlot(t *testing.T) {
	w := NewWorld(Caps{Units: 8})
	id, _ := w.CreateUnit(fixed.Vec2{}, 0)
	w.Combats.Add(w.Ents, id)
	r := w.Combats.Row(id)
	w.Combats.DmgBase[r][0] = 12
	w.Combats.DmgDice[r][0] = 1
	w.Combats.DmgSides[r][0] = 6
	w.Combats.Cooldown[r][0] = 30
	// slot 1 untouched
	t.Logf("weapon 0: base=%d dice=%dd%d cooldown=%d | weapon 1 (unused): base=%d dice=%dd%d cooldown=%d",
		w.Combats.DmgBase[r][0], w.Combats.DmgDice[r][0], w.Combats.DmgSides[r][0], w.Combats.Cooldown[r][0],
		w.Combats.DmgBase[r][1], w.Combats.DmgDice[r][1], w.Combats.DmgSides[r][1], w.Combats.Cooldown[r][1])
	if w.Combats.Cooldown[r][1] != 0 || w.Combats.DmgDice[r][1] != 0 {
		t.Fatalf("unused slot must stay zero-valued")
	}
}

// Edge 2: inventory slot out of range rejected; occupied slot
// rejected; slots unchanged after both.
func TestStoreInventorySlotBounds(t *testing.T) {
	w := NewWorld(Caps{Units: 8})
	hero, _ := w.CreateUnit(fixed.Vec2{}, 0)
	item, _ := w.CreateUnit(fixed.Vec2{}, 0) // items are entities
	w.Invents.Add(w.Ents, hero)

	var asserts []string
	w.Invents.DebugAssert = func(msg string, id EntityID) { asserts = append(asserts, msg) }

	w.Invents.SetSlot(hero, 0, item)
	r := w.Invents.Row(hero)
	before := w.Invents.Slots[r]
	if w.Invents.SetSlot(hero, 6, item) { // slot 7 (index 6) of a 6-slot bag
		t.Fatalf("slot index 6 must be rejected")
	}
	if w.Invents.SetSlot(hero, -1, item) {
		t.Fatalf("negative slot must be rejected")
	}
	if w.Invents.SetSlot(hero, 0, item) {
		t.Fatalf("occupied slot must be rejected")
	}
	after := w.Invents.Slots[r]
	t.Logf("slots before bad ops: %v\nslots after  bad ops: %v\nasserts: %v", before, after, asserts)
	if before != after {
		t.Fatalf("rejected ops mutated the inventory")
	}
	if got, ok := w.Invents.ClearSlot(hero, 0); !ok || got != item {
		t.Fatalf("ClearSlot must hand back the item")
	}
}

// Edge 3: cooldown clocks near the u32 boundary — the signed-diff
// compare handles ready-at values that wrapped past zero.
func TestStoreCombatCooldownU32Boundary(t *testing.T) {
	tick := uint32(4294967000) // near 2^32-1 = 4294967295
	period := uint32(500)
	readyAt := tick + period // wraps: 4294967000 + 500 = 204 (mod 2^32)
	t.Logf("attack at tick %d, period %d -> ReadyAt = %d (wrapped past 2^32)", tick, period, readyAt)

	cases := []struct {
		now  uint32
		want bool
	}{
		{tick, false},         // just fired
		{tick + 499, false},   // 1 tick early (still pre-wrap)
		{readyAt - 1, false},  // post-wrap, 1 tick early
		{readyAt, true},       // exactly ready
		{readyAt + 100, true}, // past ready
	}
	for _, c := range cases {
		got := CooldownReady(c.now, readyAt)
		t.Logf("CooldownReady(now=%d, readyAt=%d) = %v (want %v)", c.now, readyAt, got, c.want)
		if got != c.want {
			t.Fatalf("wrap mishandled at now=%d", c.now)
		}
	}
}

// Edge 4: swap-remove preserves other rows' queue-head indices and
// combat targets.
func TestStoreOrderSwapPreservesHeads(t *testing.T) {
	w := NewWorld(Caps{Units: 8})
	var ids []EntityID
	for i := 0; i < 3; i++ {
		id, _ := w.CreateUnit(fixed.Vec2{}, 0)
		w.Orders.Add(w.Ents, id)
		r := w.Orders.Row(id)
		w.Orders.QueueHead[r] = int32(100 + i) // distinct pool indices
		ids = append(ids, id)
	}
	headOf := func(id EntityID) int32 { return w.Orders.QueueHead[w.Orders.Row(id)] }
	t.Logf("heads before remove: e0=%d e1=%d e2=%d", headOf(ids[0]), headOf(ids[1]), headOf(ids[2]))
	w.Orders.Remove(ids[0]) // entity 2's row swaps into row 0
	t.Logf("heads after remove(e0): e1=%d e2=%d (e2 now at row %d)", headOf(ids[1]), headOf(ids[2]), w.Orders.Row(ids[2]))
	if headOf(ids[1]) != 101 || headOf(ids[2]) != 102 {
		t.Fatalf("queue heads corrupted by swap-remove: e1=%d e2=%d", headOf(ids[1]), headOf(ids[2]))
	}
}

// Zero-alloc add/remove across the four stores; DestroyUnit clears
// all nine component stores.
func TestStoreCombatGroupZeroAllocAndDestroy(t *testing.T) {
	w := NewWorld(Caps{Units: 64})
	id, _ := w.CreateUnit(fixed.Vec2{}, 0)
	allocs := testing.AllocsPerRun(200, func() {
		w.Combats.Add(w.Ents, id)
		w.Abilities.Add(w.Ents, id)
		w.Invents.Add(w.Ents, id)
		w.Orders.Add(w.Ents, id)
		w.Combats.Remove(id)
		w.Abilities.Remove(id)
		w.Invents.Remove(id)
		w.Orders.Remove(id)
	})
	t.Logf("AllocsPerRun(add+remove x4 stores) = %v", allocs)
	if allocs != 0 {
		t.Fatalf("R-GC-1 violated: %v allocs/op", allocs)
	}

	w.Combats.Add(w.Ents, id)
	w.Abilities.Add(w.Ents, id)
	w.Invents.Add(w.Ents, id)
	w.Orders.Add(w.Ents, id)
	w.DestroyUnit(id)
	if w.Combats.Count() != 0 || w.Abilities.Count() != 0 || w.Invents.Count() != 0 || w.Orders.Count() != 0 {
		t.Fatalf("DestroyUnit left rows: combat=%d ability=%d inv=%d order=%d",
			w.Combats.Count(), w.Abilities.Count(), w.Invents.Count(), w.Orders.Count())
	}
}

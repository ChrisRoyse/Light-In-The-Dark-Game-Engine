package sim

// #305 item tests. SoT = inventory slot dumps, item instance fields
// (charges), derived-stat raw values, ground-item positions across
// twin runs, and the save round-trip hashes.

import (
	"bytes"
	"fmt"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/statehash"
)

const (
	itClaws  uint16 = 0 // passive: +3 armor, +2 attack damage; not dropped on death
	itPotion uint16 = 1 // 2 charges, consumable, 10t class cooldown, damage-30 pipeline
	itStone  uint16 = 2 // passive: +1 armor; drop-on-death
	itScroll uint16 = 3 // targeted (range 200), uncharged, same pipeline
)

func itemDefs() []data.Item {
	dmg30 := data.EffectList{Off: 0, Len: 1}
	return []data.Item{
		{ID: "claws", Class: 0, DropOnDeath: false, Mods: []data.StatMod{
			{Stat: data.StatArmor, Add: 3, Permille: 1000},
			{Stat: data.StatAttackDamage, Add: 2 << 32, Permille: 1000},
		}},
		{ID: "potion", Class: 1, Charges: 2, Consumable: true,
			CooldownTicks: 10, Effects: dmg30},
		{ID: "stone", Class: 0, DropOnDeath: true, Mods: []data.StatMod{
			{Stat: data.StatArmor, Add: 1, Permille: 1000},
		}},
		{ID: "scroll", Class: 2, Targeted: true,
			UseRange: fixed.FromInt(200), Effects: dmg30},
	}
}

func itemWorld(t *testing.T) *World {
	t.Helper()
	resetEffectExecs()
	t.Cleanup(resetEffectExecs)
	RegisterCoreEffectExecs()
	w := NewWorld(Caps{Units: 64})
	if err := w.BindDamageMatrix(dmgMatrix); err != nil {
		t.Fatal(err)
	}
	arena := []data.CompiledEffect{
		{Prim: data.EPDamage, Params: [data.MaxEffectParams]int64{30, 0, 0, 0}},
	}
	if err := w.BindEffects(arena); err != nil {
		t.Fatal(err)
	}
	if !w.BindItemDefs(itemDefs()) {
		t.Fatal("bind item defs")
	}
	return w
}

// itemUnit: transform + inventory (+ orders/movement for pickup
// orders) + health so pipelines have something to hit.
func itemUnit(t *testing.T, w *World, pos fixed.Vec2) EntityID {
	t.Helper()
	id, ok := w.CreateUnit(pos, 0)
	if !ok || !w.AddInventory(id) ||
		!w.Healths.Add(w.Ents, id, 200*fixed.One, 0, 0, 0) ||
		!w.Orders.Add(w.Ents, id) ||
		!w.Movements.Add(w.Ents, w.Transforms, id, 4*fixed.One, 65535) {
		t.Fatal("itemUnit failed")
	}
	return id
}

func invDump(w *World, id EntityID) string {
	ir := w.Invents.Row(id)
	if ir == -1 {
		return "no-inventory"
	}
	s := fmt.Sprintf("t%d slots[", w.Tick())
	for i := 0; i < InventorySlots; i++ {
		item := w.Invents.Slots[ir][i]
		if item == 0 {
			s += " -"
			continue
		}
		r := w.Items.Row(item)
		s += fmt.Sprintf(" %s/c%d", w.itemDefs[w.Items.TypeID[r]].ID, w.Items.Charges[r])
	}
	return s + " ]"
}

func itemDump(w *World, item EntityID) string {
	r := w.Items.Row(item)
	if r == -1 {
		return fmt.Sprintf("item %d: no row (alive=%v)", item, w.Ents.Alive(item))
	}
	s := fmt.Sprintf("item %s charges=%d carrier=%d", w.itemDefs[w.Items.TypeID[r]].ID,
		w.Items.Charges[r], w.Items.Carrier[r])
	if tr := w.Transforms.Row(item); tr != -1 {
		s += fmt.Sprintf(" ground=(%d,%d)", w.Transforms.Pos[tr].X.Floor(), w.Transforms.Pos[tr].Y.Floor())
	}
	return s
}

// Edge 1: 6 slots fill in order; the 7th pickup refuses with ItemFull
// and the item stays grounded.
func TestItemPickupFullInventory(t *testing.T) {
	w := itemWorld(t)
	u := itemUnit(t, w, pt2(100, 100))
	var items []EntityID
	for i := 0; i < 7; i++ {
		it, ok := w.SpawnItem(itClaws, pt2(110+int32(i), 100))
		if !ok {
			t.Fatal("spawn item")
		}
		items = append(items, it)
	}
	t.Logf("before: %s", invDump(w, u))
	for i := 0; i < 6; i++ {
		if got := w.PickupItem(u, items[i]); got != ItemOK {
			t.Fatalf("pickup %d: %d", i, got)
		}
	}
	t.Logf("after 6: %s", invDump(w, u))
	got := w.PickupItem(u, items[6])
	t.Logf("7th pickup -> %d; %s", got, itemDump(w, items[6]))
	if got != ItemFull {
		t.Fatalf("full inventory: %d", got)
	}
	r := w.Items.Row(items[6])
	if w.Items.Carrier[r] != 0 || w.Transforms.Row(items[6]) == -1 {
		t.Fatal("refused pickup mutated the ground item")
	}
	// reach gate: a far item refuses ItemTooFar
	far, _ := w.SpawnItem(itClaws, pt2(5000, 5000))
	w.DropItem(u, 5) // free a slot so reach is the failing gate
	if got := w.PickupItem(u, far); got != ItemTooFar {
		t.Fatalf("far pickup: %d", got)
	}
}

// Edge 2: a 2-charge consumable: use → 1, class cooldown blocks, use
// → 0 → the item entity is destroyed and the slot empties. Uncharged
// items never deplete.
func TestItemUseChargesConsumable(t *testing.T) {
	w := itemWorld(t)
	u := itemUnit(t, w, pt2(100, 100))
	victim := itemUnit(t, w, pt2(150, 100))
	pot, _ := w.SpawnItem(itPotion, pt2(110, 100))
	scr, _ := w.SpawnItem(itScroll, pt2(112, 100))
	w.PickupItem(u, pot)
	w.PickupItem(u, scr)
	hp := func() int64 { return w.Healths.Life[w.Healths.Row(victim)].Floor() }

	t.Logf("before use 1: %s | victim hp=%d", itemDump(w, pot), hp())
	if got := w.UseItem(u, 0, victim, fixed.Vec2{}); got != ItemOK {
		t.Fatalf("use 1: %d", got)
	}
	w.Step() // damage applies at combat-phase end
	t.Logf("after use 1:  %s | victim hp=%d | %s", itemDump(w, pot), hp(), invDump(w, u))
	if r := w.Items.Row(pot); w.Items.Charges[r] != 1 {
		t.Fatalf("charges after use 1: %d", w.Items.Charges[r])
	}
	if hp() != 170 {
		t.Fatalf("victim hp %d, want 170 (30 dmg, coeff 1000, armor 0)", hp())
	}
	if got := w.UseItem(u, 0, victim, fixed.Vec2{}); got != ItemOnCooldown {
		t.Fatalf("class cooldown: %d", got)
	}
	for i := 0; i < 10; i++ {
		w.Step()
	}
	if got := w.UseItem(u, 0, victim, fixed.Vec2{}); got != ItemOK {
		t.Fatalf("use 2: %d", got)
	}
	w.Step()
	t.Logf("after use 2:  %s | victim hp=%d | %s", itemDump(w, pot), hp(), invDump(w, u))
	if w.Ents.Alive(pot) || w.Items.Row(pot) != -1 {
		t.Fatal("consumable at 0 charges not destroyed")
	}
	ir := w.Invents.Row(u)
	if w.Invents.Slots[ir][0] != 0 {
		t.Fatal("slot not cleared")
	}
	if hp() != 140 {
		t.Fatalf("victim hp %d, want 140", hp())
	}
	// targeted gates: out of range / dead target refuse; in range fires
	farv := itemUnit(t, w, pt2(2000, 2000))
	if got := w.UseItem(u, 1, farv, fixed.Vec2{}); got != ItemBadTarget {
		t.Fatalf("out of range: %d", got)
	}
	if got := w.UseItem(u, 1, victim, fixed.Vec2{}); got != ItemOK {
		t.Fatalf("scroll in range: %d", got)
	}
	if r := w.Items.Row(scr); w.Items.Charges[r] != 0 {
		t.Fatal("uncharged item grew charges")
	}
	if w.UseItem(u, 1, victim, fixed.Vec2{}) != ItemOK { // class 2 has no cooldown
		t.Fatal("uncharged reuse refused")
	}
	if !w.Ents.Alive(scr) {
		t.Fatal("uncharged item destroyed")
	}
	// passive refuses
	cl, _ := w.SpawnItem(itClaws, pt2(108, 100))
	w.PickupItem(u, cl)
	if got := w.UseItem(u, 0, victim, fixed.Vec2{}); got != ItemNotUsable {
		t.Fatalf("passive use: %d", got)
	}
}

// Edge 3: carried modifiers — +3 armor / +2 attack damage fold on
// pickup, restore exactly on drop; stacking with a second item sums.
func TestItemCarriedModifiers(t *testing.T) {
	w := itemWorld(t)
	u := itemUnit(t, w, pt2(100, 100))
	claws, _ := w.SpawnItem(itClaws, pt2(110, 100))
	stone, _ := w.SpawnItem(itStone, pt2(112, 100))
	armor := func() int { return w.BuffedArmor(u, 0) }
	dmgAdd := func() int64 { return w.buffAdd[data.StatAttackDamage][u.Index()] }
	t.Logf("bare:        armor=%d dmgAdd(raw)=%d", armor(), dmgAdd())
	if armor() != 0 || dmgAdd() != 0 {
		t.Fatal("dirty baseline")
	}
	w.PickupItem(u, claws)
	t.Logf("claws:       armor=%d dmgAdd(raw)=%d", armor(), dmgAdd())
	if armor() != 3 || dmgAdd() != 2<<32 {
		t.Fatalf("claws fold: armor=%d dmgAdd=%d", armor(), dmgAdd())
	}
	w.PickupItem(u, stone)
	t.Logf("claws+stone: armor=%d dmgAdd(raw)=%d", armor(), dmgAdd())
	if armor() != 4 {
		t.Fatalf("stacked fold: armor=%d", armor())
	}
	if got := w.DropItem(u, 0); got != ItemOK { // claws
		t.Fatalf("drop: %d", got)
	}
	t.Logf("stone only:  armor=%d dmgAdd(raw)=%d | %s", armor(), dmgAdd(), itemDump(w, claws))
	if armor() != 1 || dmgAdd() != 0 {
		t.Fatalf("fold not restored: armor=%d dmgAdd=%d", armor(), dmgAdd())
	}
	if w.Transforms.Row(claws) == -1 || w.Items.Carrier[w.Items.Row(claws)] != 0 {
		t.Fatal("dropped item not grounded")
	}
}

// Edge 4: death drops — flagged items ground at deterministic
// adjacent cells, unflagged die with the carrier; two runs identical.
func TestItemDeathDropDeterministic(t *testing.T) {
	run := func() (stonePos fixed.Vec2, stoneAlive, clawsAlive bool) {
		w := itemWorld(t)
		u := itemUnit(t, w, pt2(300, 300))
		claws, _ := w.SpawnItem(itClaws, pt2(310, 300))
		stone, _ := w.SpawnItem(itStone, pt2(312, 300))
		w.PickupItem(u, claws) // slot 0: not dropped on death
		w.PickupItem(u, stone) // slot 1: drop-on-death
		w.KillUnit(u)
		w.Step()
		if tr := w.Transforms.Row(stone); tr != -1 {
			stonePos = w.Transforms.Pos[tr]
		}
		return stonePos, w.Ents.Alive(stone), w.Ents.Alive(claws)
	}
	p1, sAlive1, cAlive1 := run()
	p2, sAlive2, cAlive2 := run()
	t.Logf("run 1: stone=(%d,%d) alive=%v claws alive=%v", p1.X.Floor(), p1.Y.Floor(), sAlive1, cAlive1)
	t.Logf("run 2: stone=(%d,%d) alive=%v claws alive=%v", p2.X.Floor(), p2.Y.Floor(), sAlive2, cAlive2)
	if !sAlive1 || !sAlive2 {
		t.Fatal("drop-on-death item did not survive")
	}
	if cAlive1 || cAlive2 {
		t.Fatal("unflagged item survived its carrier")
	}
	if p1 != p2 {
		t.Fatalf("drop position diverged: (%d,%d) vs (%d,%d)", p1.X.Floor(), p1.Y.Floor(), p2.X.Floor(), p2.Y.Floor())
	}
	// seed rotation: slot 1 starts the ring scan one direction later
	// (N of the carrier instead of E)
	if p1.X.Floor() != 300 || p1.Y.Floor() <= 300 {
		t.Fatalf("expected the slot-1 drop north of (300,300), got (%d,%d)", p1.X.Floor(), p1.Y.Floor())
	}
}

// Edge 5: cross-give between two units in one tick — no duplication,
// both inventories and carrier fields consistent.
func TestItemGiveSwapNoDuplication(t *testing.T) {
	w := itemWorld(t)
	a := itemUnit(t, w, pt2(100, 100))
	b := itemUnit(t, w, pt2(150, 100))
	for _, u := range []EntityID{a, b} {
		if !w.Owners.Add(w.Ents, u, 0, 0, 0) {
			t.Fatal("owner add")
		}
	}
	i1, _ := w.SpawnItem(itClaws, pt2(105, 100))
	i2, _ := w.SpawnItem(itStone, pt2(155, 100))
	w.PickupItem(a, i1)
	w.PickupItem(b, i2)
	t.Logf("before: A %s | B %s", invDump(w, a), invDump(w, b))
	if got := w.GiveItem(a, 0, b); got != ItemOK {
		t.Fatalf("give a->b: %d", got)
	}
	if got := w.GiveItem(b, 0, a); got != ItemOK { // b's slot 0 = i2 still
		t.Fatalf("give b->a: %d", got)
	}
	t.Logf("after:  A %s | B %s", invDump(w, a), invDump(w, b))
	// no duplication: each item exactly once, carrier consistent
	count := map[EntityID]int{}
	for _, u := range []EntityID{a, b} {
		ir := w.Invents.Row(u)
		for s := 0; s < InventorySlots; s++ {
			if it := w.Invents.Slots[ir][s]; it != 0 {
				count[it]++
				if w.Items.Carrier[w.Items.Row(it)] != u {
					t.Fatalf("item %d slotted on %d but carrier=%d", it, u, w.Items.Carrier[w.Items.Row(it)])
				}
			}
		}
	}
	if count[i1] != 1 || count[i2] != 1 {
		t.Fatalf("duplication: i1=%d i2=%d", count[i1], count[i2])
	}
	ar, br := w.Invents.Row(a), w.Invents.Row(b)
	if w.Invents.Slots[ar][0] != i2 || w.Invents.Slots[br][1] != i1 {
		t.Fatalf("slot layout wrong: A0=%d B1=%d", w.Invents.Slots[ar][0], w.Invents.Slots[br][1])
	}
	// give to a FULL inventory refuses without mutation
	for i := 0; i < 5; i++ {
		it, _ := w.SpawnItem(itClaws, pt2(150+int32(i), 102))
		if got := w.PickupItem(b, it); got != ItemOK {
			t.Fatalf("fill b: %d", got)
		}
	}
	if got := w.GiveItem(a, 0, b); got != ItemFull {
		t.Fatalf("give to full: %d", got)
	}
	if w.Items.Carrier[w.Items.Row(i2)] != a {
		t.Fatal("refused give mutated the item")
	}
	// in-inventory swap reorders slots only (i1 sits at B's slot 1)
	w.SwapSlots(b, 1, 5)
	if w.Invents.Slots[br][5] != i1 || w.Items.Carrier[w.Items.Row(i1)] != b {
		t.Fatal("swap did not move the item")
	}
}

// OrderPickup: move→take; full inventory completes the order false.
func TestItemPickupOrder(t *testing.T) {
	w := itemWorld(t)
	u := itemUnit(t, w, pt2(100, 100))
	item, _ := w.SpawnItem(itClaws, pt2(900, 100)) // out of reach: must walk
	var got []Event
	w.RegisterHandler(hA, func(_ *World, e Event) { got = append(got, e) })
	w.Subscribe(EvItemPickedUp, hA)
	if !w.IssueOrder(u, Order{Kind: OrderPickup, Target: item}, false) {
		t.Fatal("issue")
	}
	for i := 0; i < 400 && len(got) == 0; i++ {
		w.Step()
	}
	t.Logf("t%d picked up after walk: %s | %s", w.Tick(), invDump(w, u), itemDump(w, item))
	if len(got) != 1 || got[0].Dst != item {
		t.Fatalf("pickup events: %v", got)
	}
	if w.Items.Carrier[w.Items.Row(item)] != u {
		t.Fatal("not carried")
	}
	// classify: ground item → TCItem; carried item → invalid
	g, _ := w.SpawnItem(itStone, pt2(140, 100))
	if tc, ok := w.ClassifyTarget(0, g); !ok || tc != data.TCItem {
		t.Fatalf("ground classify: %d %v", tc, ok)
	}
	if _, ok := w.ClassifyTarget(0, item); ok {
		t.Fatal("carried item classified as clickable")
	}
}

// Determinism + save v6: twins agree; charges/cooldowns/ground items
// round-trip; loads with item rows but no defs refuse by name.
func TestItemDeterminismAndSave(t *testing.T) {
	resetEffectExecs()
	t.Cleanup(resetEffectExecs)
	RegisterCoreEffectExecs()
	build := func() *World {
		w := NewWorld(Caps{Units: 64})
		w.BindDamageMatrix(dmgMatrix)
		w.BindEffects([]data.CompiledEffect{
			{Prim: data.EPDamage, Params: [data.MaxEffectParams]int64{30, 0, 0, 0}},
		})
		w.BindItemDefs(itemDefs())
		u, _ := w.CreateUnit(pt2(100, 100), 0)
		w.AddInventory(u)
		w.Healths.Add(w.Ents, u, 200*fixed.One, 0, 0, 0)
		v, _ := w.CreateUnit(pt2(150, 100), 0)
		w.Healths.Add(w.Ents, v, 200*fixed.One, 0, 0, 0)
		pot, _ := w.SpawnItem(itPotion, pt2(110, 100))
		w.SpawnItem(itStone, pt2(400, 400)) // stays grounded
		w.PickupItem(u, pot)
		w.UseItem(u, 0, v, fixed.Vec2{}) // charges 2→1, class cooldown armed
		w.Step()
		return w
	}
	a, b := build(), build()
	var sa, sb statehash.Snapshot
	a.HashState(NewHashRegistry(), &sa)
	b.HashState(NewHashRegistry(), &sb)
	t.Logf("twin A=%016x B=%016x", sa.Top, sb.Top)
	if sa.Top != sb.Top {
		t.Fatal("twin divergence")
	}
	a.Items.Charges[0]++ // instance bytes are state
	var sa2 statehash.Snapshot
	a.HashState(NewHashRegistry(), &sa2)
	if sa2.Top == sa.Top {
		t.Fatal("charge mutation invisible to the hash")
	}
	a.Items.Charges[0]--

	var buf bytes.Buffer
	if err := a.SaveState(&buf, 7); err != nil {
		t.Fatal(err)
	}
	w2 := NewWorld(Caps{Units: 64})
	w2.BindDamageMatrix(dmgMatrix)
	w2.BindEffects([]data.CompiledEffect{
		{Prim: data.EPDamage, Params: [data.MaxEffectParams]int64{30, 0, 0, 0}},
	})
	w2.BindItemDefs(itemDefs())
	if err := w2.LoadState(bytes.NewReader(buf.Bytes()), 7); err != nil {
		t.Fatal(err)
	}
	var sl statehash.Snapshot
	w2.HashState(NewHashRegistry(), &sl)
	t.Logf("loaded=%016x (orig %016x)", sl.Top, sa.Top)
	if sl.Top != sa.Top {
		t.Fatal("load diverged")
	}
	// carried modifiers re-derived at load? potion has none — verify
	// the carrier's class cooldown survived instead
	u := a.Invents.Entity[0]
	if w2.Invents.ClassReady[w2.Invents.Row(u)] != a.Invents.ClassReady[a.Invents.Row(u)] {
		t.Fatal("class cooldowns lost in round-trip")
	}
	// no defs bound → refuse by name
	w3 := NewWorld(Caps{Units: 64})
	if err := w3.LoadState(bytes.NewReader(buf.Bytes()), 7); err == nil {
		t.Fatal("load without item defs accepted")
	} else {
		t.Logf("unbound items refused: %v", err)
	}
}

// R-GC-1: a pickup order in flight allocates nothing per tick.
func TestItemTickAllocs(t *testing.T) {
	w := itemWorld(t)
	u := itemUnit(t, w, pt2(100, 100))
	item, _ := w.SpawnItem(itClaws, pt2(2000, 100))
	if !w.IssueOrder(u, Order{Kind: OrderPickup, Target: item}, false) {
		t.Fatal("issue")
	}
	w.Step()
	allocs := testing.AllocsPerRun(30, func() { w.Step() })
	t.Logf("allocs/op with pickup order driving: %v", allocs)
	if allocs != 0 {
		t.Fatalf("item tick allocates: %v", allocs)
	}
}

func BenchmarkItemTick(b *testing.B) {
	resetEffectExecs()
	b.Cleanup(resetEffectExecs)
	RegisterCoreEffectExecs()
	w := NewWorld(Caps{Units: 256})
	w.BindDamageMatrix(dmgMatrix)
	w.BindEffects([]data.CompiledEffect{
		{Prim: data.EPDamage, Params: [data.MaxEffectParams]int64{30, 0, 0, 0}},
	})
	w.BindItemDefs(itemDefs())
	for i := 0; i < 16; i++ {
		u, _ := w.CreateUnit(pt2(100+int32(i*40), 100), 0)
		w.AddInventory(u)
		w.Healths.Add(w.Ents, u, 200*fixed.One, 0, 0, 0)
		w.Orders.Add(w.Ents, u)
		w.Movements.Add(w.Ents, w.Transforms, u, 4*fixed.One, 65535)
		item, _ := w.SpawnItem(itClaws, pt2(100+int32(i*40), 3000))
		w.IssueOrder(u, Order{Kind: OrderPickup, Target: item}, false)
	}
	w.Step()
	b.ReportAllocs()
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		w.Step()
	}
}

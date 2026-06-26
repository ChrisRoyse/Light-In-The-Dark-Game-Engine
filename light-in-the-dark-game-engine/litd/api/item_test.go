package litd

// #225 items + inventory public-API FSV. SoT = the sim ItemStore /
// InventoryStore rows, unit Health, and the state hash — read back after each
// public verb to prove it writes real, deterministic, hashed sim state. The
// effect-exec registry is package-global with no reset hook from this package,
// so core execs register exactly once via sync.Once.

import (
	"sync"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/statehash"
)

var itemExecsOnce sync.Once

// itemAPIDefs: a synthetic item vocabulary with one of each behavior class.
//   - claws  : passive permanent (+armor mod), no use
//   - potion : 2-charge consumable, 30 self-damage, 10-tick class cooldown
//   - stone  : passive, drop-on-death
//   - rune   : power-up — fires 30 self-damage on pickup, never stored
func itemAPIDefs() []data.Item {
	dmg := data.EffectList{Off: 0, Len: 1}
	return []data.Item{
		{ID: "claws", Class: 0, Mods: []data.StatMod{{Stat: data.StatArmor, Add: 3, Permille: 1000}}},
		{ID: "potion", Class: 1, Charges: 2, Consumable: true, CooldownTicks: 10, Effects: dmg},
		{ID: "stone", Class: 0, DropOnDeath: true, Mods: []data.StatMod{{Stat: data.StatArmor, Add: 1, Permille: 1000}}},
		{ID: "rune", Class: 2, PowerUp: true, Effects: dmg},
	}
}

func itemAPIWorld(t *testing.T) (*sim.World, *Game) {
	t.Helper()
	itemExecsOnce.Do(sim.RegisterCoreEffectExecs)
	w := sim.NewWorld(sim.Caps{Units: 64})
	if err := w.BindDamageMatrix([][]int32{{1000}}); err != nil {
		t.Fatalf("bind matrix: %v", err)
	}
	arena := []data.CompiledEffect{{Prim: data.EPDamage, Params: [data.MaxEffectParams]int64{30, 0, 0, 0}}}
	if err := w.BindEffects(arena); err != nil {
		t.Fatalf("bind effects: %v", err)
	}
	if !w.BindItemDefs(itemAPIDefs()) {
		t.Fatal("bind item defs")
	}
	return w, newGame(w)
}

// itemAPIUnit makes a carrier: transform + inventory + health (+orders/movement
// so death-drop's adjacent-cell scan has the same shape as the sim fixtures).
func itemAPIUnit(t *testing.T, w *sim.World, g *Game, x, y int32) Unit {
	t.Helper()
	id, ok := w.CreateUnit(fixed.Vec2{X: fixed.FromInt(x), Y: fixed.FromInt(y)}, 0)
	if !ok || !w.AddInventory(id) ||
		!w.Healths.Add(w.Ents, id, fixed.FromInt(200), 0, 0, 0) ||
		!w.Orders.Add(w.Ents, id) ||
		!w.Movements.Add(w.Ents, w.Transforms, id, fixed.FromInt(4), 65535) {
		t.Fatal("itemAPIUnit failed")
	}
	return Unit{id: id, g: g}
}

func unitLife(w *sim.World, u Unit) int64 {
	return w.Healths.Life[w.Healths.Row(u.id)].Floor()
}

// TestItemCreateAndReadFSV — CreateItem writes a real ground item; the public
// getters read the sim SoT back exactly.
func TestItemCreateAndReadFSV(t *testing.T) {
	w, g := itemAPIWorld(t)
	typ := g.ItemType("potion")
	if typ.IsZero() {
		t.Fatal("ItemType(potion) is null")
	}
	it := g.CreateItem(typ, Vec2{X: 110, Y: 100})
	if !it.Valid() {
		t.Fatal("CreateItem returned invalid handle")
	}
	tid, _ := w.ItemTypeOf(it.id)
	ch, _ := w.ItemCharges(it.id)
	t.Logf("FSV after CreateItem: simType=%d simCharges=%d carrier=%d (want type=1 charges=2 carrier=0)",
		tid, ch, w.ItemCarrier(it.id))
	if it.Charges() != 2 || it.Carried() || it.Type().ref != typ.ref {
		t.Fatalf("getters wrong: charges=%d carried=%v type=%d", it.Charges(), it.Carried(), it.Type().ref)
	}
	if p := it.Position(); p.X != 110 || p.Y != 100 {
		t.Fatalf("ground position wrong: %+v", p)
	}
	// edge: unknown code → null type, CreateItem rejects → zero handle.
	if !g.ItemType("nonesuch").IsZero() {
		t.Fatal("unknown code should be null type")
	}
	if g.CreateItem(ItemType{}, Vec2{}).Valid() {
		t.Fatal("CreateItem(null) must return invalid handle")
	}
	// edge: zero-value Item verbs are safe no-ops.
	var zero Item
	zero.SetCharges(5)
	if zero.Charges() != 0 || zero.Carried() || zero.Valid() {
		t.Fatal("zero-value Item must be a safe no-op")
	}
}

// TestUnitAddItemFullInventoryFSV — issue edge (1): the 7th add to a full
// 6-slot inventory fails and the item stays grounded.
func TestUnitAddItemFullInventoryFSV(t *testing.T) {
	w, g := itemAPIWorld(t)
	u := itemAPIUnit(t, w, g, 100, 100)
	claws := g.ItemType("claws")

	for s := 0; s < 6; s++ {
		if it := u.AddItemByType(claws); !it.Valid() {
			t.Fatalf("AddItemByType slot %d failed", s)
		}
	}
	t.Logf("FSV after 6 adds: ItemCount=%d InventorySize=%d (want 6/6)", u.ItemCount(), u.InventorySize())
	if u.ItemCount() != 6 {
		t.Fatalf("expected full inventory, got %d", u.ItemCount())
	}

	// 7th: spawn a ground item, AddItem must refuse and leave it grounded.
	spare := g.CreateItem(claws, Vec2{X: 500, Y: 500})
	ok := u.AddItem(spare)
	t.Logf("FSV 7th AddItem: ok=%v ItemCount=%d spareCarried=%v sparePos=%+v (want false/6/false/(500,500))",
		ok, u.ItemCount(), spare.Carried(), spare.Position())
	if ok {
		t.Fatal("AddItem to full inventory must return false")
	}
	if u.ItemCount() != 6 {
		t.Fatalf("inventory count changed to %d", u.ItemCount())
	}
	if spare.Carried() || w.ItemCarrier(spare.id) != 0 {
		t.Fatal("refused item must stay on the ground")
	}
	if p := spare.Position(); p.X != 500 || p.Y != 500 {
		t.Fatalf("refused item moved: %+v", p)
	}
}

// TestPowerUpAutoConsumeFSV — issue edge (2): a power-up picked up is not
// stored; its effect fires on the taker and the handle goes invalid.
func TestPowerUpAutoConsumeFSV(t *testing.T) {
	w, g := itemAPIWorld(t)
	u := itemAPIUnit(t, w, g, 100, 100)
	rune := g.CreateItem(g.ItemType("rune"), Vec2{X: 105, Y: 100})

	lifeBefore := unitLife(w, u)
	countBefore := u.ItemCount()
	t.Logf("FSV before pickup: life=%d ItemCount=%d runeValid=%v", lifeBefore, countBefore, rune.Valid())

	if !u.AddItem(rune) {
		t.Fatal("power-up pickup should report success")
	}
	w.Step() // EPDamage applies at combat-phase end

	lifeAfter := unitLife(w, u)
	t.Logf("FSV after pickup+step: life=%d ItemCount=%d runeValid=%v (want life=%d count=0 valid=false)",
		lifeAfter, u.ItemCount(), rune.Valid(), lifeBefore-30)
	if u.ItemCount() != 0 {
		t.Fatalf("power-up must not be stored: ItemCount=%d", u.ItemCount())
	}
	if rune.Valid() {
		t.Fatal("power-up handle must be invalid after auto-consume")
	}
	if lifeAfter != lifeBefore-30 {
		t.Fatalf("power-up effect not applied: life %d -> %d, want -30", lifeBefore, lifeAfter)
	}
}

// TestUnitDeathDropsItemsFSV — issue edge (3): a dying carrier grounds its
// drop-on-death items at its death position and destroys the rest.
func TestUnitDeathDropsItemsFSV(t *testing.T) {
	w, g := itemAPIWorld(t)
	u := itemAPIUnit(t, w, g, 300, 300)
	claws := u.AddItemByType(g.ItemType("claws")) // slot 0: not dropped
	stone := u.AddItemByType(g.ItemType("stone")) // slot 1: drop-on-death
	if !claws.Valid() || !stone.Valid() {
		t.Fatal("setup adds failed")
	}
	t.Logf("FSV before death: ItemCount=%d clawsCarried=%v stoneCarried=%v", u.ItemCount(), claws.Carried(), stone.Carried())

	u.Kill()
	w.Step() // death resolves; inventory releases

	t.Logf("FSV after death: stoneValid=%v stoneCarried=%v stonePos=%+v clawsValid=%v",
		stone.Valid(), stone.Carried(), stone.Position(), claws.Valid())
	if !stone.Valid() || stone.Carried() {
		t.Fatal("drop-on-death item must survive on the ground")
	}
	if p := stone.Position(); p.X == 0 && p.Y == 0 {
		t.Fatal("dropped item should have a real ground position")
	}
	if claws.Valid() {
		t.Fatal("non-drop item must die with its carrier")
	}
}

// TestUnitUseItemEdgesFSV — issue edge (4) + happy use path: a consumable
// spends charges, is destroyed at 0, and a use against the now-empty slot is a
// no-op; a passive item refuses use.
func TestUnitUseItemEdgesFSV(t *testing.T) {
	w, g := itemAPIWorld(t)
	u := itemAPIUnit(t, w, g, 100, 100)
	victim := itemAPIUnit(t, w, g, 150, 100)
	potion := u.AddItemByType(g.ItemType("potion"))
	if potion.Charges() != 2 {
		t.Fatalf("potion should start at 2 charges, got %d", potion.Charges())
	}

	// use 1: charge 2 -> 1, victim takes 30.
	lifeBefore := unitLife(w, victim)
	if !u.UseItem(0, UseOn(victim)) {
		t.Fatal("use 1 should succeed")
	}
	w.Step()
	t.Logf("FSV use1: charges=%d victimLife %d->%d", potion.Charges(), lifeBefore, unitLife(w, victim))
	if potion.Charges() != 1 || unitLife(w, victim) != lifeBefore-30 {
		t.Fatalf("use 1 wrong: charges=%d life=%d", potion.Charges(), unitLife(w, victim))
	}
	// edge: re-use while the class cooldown is armed → no-op.
	if u.UseItem(0, UseOn(victim)) {
		t.Fatal("use during class cooldown must be a no-op")
	}
	for i := 0; i < 10; i++ {
		w.Step()
	}
	// use 2: charge 1 -> 0, consumable destroyed, slot cleared.
	if !u.UseItem(0, UseOn(victim)) {
		t.Fatal("use 2 should succeed")
	}
	w.Step()
	t.Logf("FSV use2: potionValid=%v ItemCount=%d slot0=%v", potion.Valid(), u.ItemCount(), u.ItemInSlot(0).Valid())
	if potion.Valid() || u.ItemCount() != 0 || u.ItemInSlot(0).Valid() {
		t.Fatal("consumable at 0 charges must be destroyed and its slot cleared")
	}
	// edge (4): UseItem against the now-empty slot is a no-op (false).
	countBefore := u.ItemCount()
	if u.UseItem(0, UseOn(victim)) {
		t.Fatal("use of empty slot must be a no-op false")
	}
	if u.ItemCount() != countBefore {
		t.Fatal("no-op use changed inventory")
	}
	// edge: passive item refuses use.
	claws := u.AddItemByType(g.ItemType("claws"))
	if u.UseItem(0, UseOn(victim)) {
		t.Fatal("passive item use must return false")
	}
	if claws.Charges() != 0 {
		t.Fatalf("passive use mutated charges: %d", claws.Charges())
	}
}

// TestItemAPIDeterminismFSV — two identical public-API item scripts produce
// the identical state hash.
func TestItemAPIDeterminismFSV(t *testing.T) {
	run := func() uint64 {
		w, g := itemAPIWorld(t)
		u := itemAPIUnit(t, w, g, 200, 200)
		u.AddItemByType(g.ItemType("claws"))
		p := u.AddItemByType(g.ItemType("potion"))
		victim := itemAPIUnit(t, w, g, 240, 200)
		u.UseItem(1, UseOn(victim))
		w.Step()
		u.DropItem(0)
		_ = p
		reg := sim.NewHashRegistry()
		var snap statehash.Snapshot
		w.HashState(reg, &snap)
		return snap.Top
	}
	a, b := run(), run()
	t.Logf("FSV item-script determinism: run1=%016x run2=%016x", a, b)
	if a != b {
		t.Fatalf("identical item scripts diverged: %016x vs %016x", a, b)
	}
}

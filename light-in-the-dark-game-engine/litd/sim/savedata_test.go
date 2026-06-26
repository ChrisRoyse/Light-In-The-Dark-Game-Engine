package sim

// #208 campaign carry-over tests (D-15). SoT = the extracted record
// dump, the map-A vs map-B field tables, and the named refusal
// errors.

import (
	"fmt"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
)

const sdFingerprint = 0xA17E5C0DE0B5E55E

func sdItemDefs() []data.Item {
	return []data.Item{
		{ID: "sd-claws", Class: 0, Mods: []data.StatMod{{Stat: data.StatArmor, Add: 3, Permille: 1000}}},
		{ID: "sd-wand", Class: 1, Charges: 3, Consumable: true},
	}
}

// sdWorld is one "map": the hero fixture plus items, an armor buff
// type, and the table fingerprint.
func sdWorld(t *testing.T) *World {
	t.Helper()
	w := heroWorld(t)
	if !w.BindItemDefs(sdItemDefs()) {
		t.Fatal("bind item defs")
	}
	if !w.BindBuffTypes([]data.BuffType{{ID: "stone-skin", DurationTicks: 600, MaxStacks: 1,
		Mods: []data.StatMod{{Stat: data.StatArmor, Add: 5, Permille: 1000}}}}) {
		t.Fatal("bind buffs")
	}
	w.SetDataFingerprint(sdFingerprint)
	return w
}

// sdFields is the comparison table: every record-carried field plus
// the destination-derived ones.
func sdFields(w *World, hero EntityID) string {
	hr := w.Heroes.Row(hero)
	h := w.Heroes
	s := fmt.Sprintf("lvl=%d xp=%d str=%d agi=%d int=%d pts=%d skills=%v",
		h.Level[hr], h.XP[hr], int64(h.Str[hr]), int64(h.Agi[hr]), int64(h.Int[hr]),
		h.SkillPoints[hr], h.SkillLevel[hr])
	ar := w.Abilities.Row(hero)
	s += fmt.Sprintf(" ability0=%d/L%d", w.Abilities.AbilityID[ar][0], w.Abilities.Level[ar][0])
	if ir := w.Invents.Row(hero); ir != -1 {
		s += " items["
		for sl := 0; sl < InventorySlots; sl++ {
			item := w.Invents.Slots[ir][sl]
			if item == 0 {
				s += " -"
				continue
			}
			r := w.Items.Row(item)
			s += fmt.Sprintf(" %s/c%d", w.itemDefs[w.Items.TypeID[r]].ID, w.Items.Charges[r])
		}
		s += " ]"
	}
	return s
}

// The D-15 fixture: end of map A → record → wire bytes → map B.
// Level, attributes, learned skills, and item charges round-trip
// bit-identically; slot layout survives.
func TestHeroCarryOver(t *testing.T) {
	mapA := sdWorld(t)
	hero, _ := mapA.SpawnHero(hPaladin, 0, 0, pt2(100, 100))
	mapA.AddXP(hero, 250) // level 2
	if mapA.LearnSkill(hero, 0) != SkillOK {
		t.Fatal("learn")
	}
	if !mapA.AddInventory(hero) {
		t.Fatal("inventory")
	}
	claws, _ := mapA.SpawnItem(0, pt2(110, 100))
	wand, _ := mapA.SpawnItem(1, pt2(112, 100))
	mapA.PickupItem(hero, claws)
	mapA.PickupItem(hero, wand)
	mapA.SwapSlots(hero, 1, 3)                   // sparse layout: wand at slot 3
	mapA.Items.Charges[mapA.Items.Row(wand)] = 2 // edge 1: 2 of 3 charges left

	sd, err := mapA.ExtractSaveData(hero)
	if err != nil {
		t.Fatal(err)
	}
	t.Logf("record: fp=%016x hero=%+v", sd.Fingerprint, sd.Hero)
	t.Logf("record items: %+v", sd.Items)
	if !sd.Items[0].Used || sd.Items[0].TypeID != 0 ||
		!sd.Items[3].Used || sd.Items[3].TypeID != 1 || sd.Items[3].Charges != 2 {
		t.Fatalf("record items wrong: %+v", sd.Items)
	}

	wire := EncodeSaveData(nil, &sd)
	dec, rest, err := DecodeSaveData(wire)
	if err != nil || len(rest) != 0 {
		t.Fatalf("decode: %v rest=%d", err, len(rest))
	}
	if dec != sd {
		t.Fatalf("wire round-trip diff:\n%+v\n%+v", sd, dec)
	}

	mapB := sdWorld(t)
	hero2, err := mapB.InstantiateSaveData(&dec, 3, 1, pt2(700, 700))
	if err != nil {
		t.Fatal(err)
	}
	a, b := sdFields(mapA, hero), sdFields(mapB, hero2)
	t.Logf("map A: %s", a)
	t.Logf("map B: %s", b)
	if a != b {
		t.Fatal("carry-over field tables differ")
	}
	// derived stats re-derived, not carried: agi fold + claws +3 on
	// both maps — the adds must match exactly
	wantAdd := mapA.buffAdd[data.StatArmor][hero.Index()]
	gotAdd := mapB.buffAdd[data.StatArmor][hero2.Index()]
	t.Logf("derived armor add: map A=%d map B=%d (agi fold + item +3)", wantAdd, gotAdd)
	if gotAdd != wantAdd || wantAdd < 3+3 {
		t.Fatalf("item fold not re-derived on map B: %d vs %d", gotAdd, wantAdd)
	}
	// re-extract on B must equal the original record
	sd2, err := mapB.ExtractSaveData(hero2)
	if err != nil || sd2 != sd {
		t.Fatalf("re-extract diff (%v):\n%+v\n%+v", err, sd, sd2)
	}
}

// Edge 2: a +armor buff at extraction time is NOT carried — the
// record holds base attributes; map B re-derives without the buff.
func TestCarryOverDerivedNotCarried(t *testing.T) {
	mapA := sdWorld(t)
	hero, _ := mapA.SpawnHero(hPaladin, 0, 0, pt2(100, 100))
	base := mapA.BuffedArmor(hero, 0)
	if !mapA.ApplyBuff(hero, hero, 0, 1) { // stone-skin +5
		t.Fatal("buff")
	}
	buffed := mapA.BuffedArmor(hero, 0)
	t.Logf("map A armor: base-derived=%d buffed-at-extraction=%d", base, buffed)
	if buffed != base+5 {
		t.Fatalf("buff fold wrong: %d vs %d", buffed, base)
	}
	sd, err := mapA.ExtractSaveData(hero)
	if err != nil {
		t.Fatal(err)
	}
	mapB := sdWorld(t)
	hero2, err := mapB.InstantiateSaveData(&sd, 0, 0, pt2(100, 100))
	if err != nil {
		t.Fatal(err)
	}
	got := mapB.BuffedArmor(hero2, 0)
	t.Logf("map B armor: %d (want the unbuffed base %d)", got, base)
	if got != base {
		t.Fatalf("map B armor %d carried the buff (base %d)", got, base)
	}
}

// Edge 3: a record taken under table hash X refuses to load under Y.
func TestCarryOverFingerprintMismatch(t *testing.T) {
	mapA := sdWorld(t)
	hero, _ := mapA.SpawnHero(hPaladin, 0, 0, pt2(100, 100))
	sd, err := mapA.ExtractSaveData(hero)
	if err != nil {
		t.Fatal(err)
	}
	mapB := sdWorld(t)
	mapB.SetDataFingerprint(0xDEADBEEF) // different table content
	if _, err := mapB.InstantiateSaveData(&sd, 0, 0, pt2(100, 100)); err == nil {
		t.Fatal("cross-version replay accepted")
	} else {
		t.Logf("refused: %v", err)
	}
	// unversioned worlds refuse both directions
	bare := heroWorld(t)
	if _, err := bare.ExtractSaveData(hero); err == nil {
		t.Fatal("extraction without a fingerprint accepted")
	} else {
		t.Logf("refused: %v", err)
	}
}

// Edge 4: minimal record — fresh level-1 hero, empty inventory.
func TestCarryOverMinimal(t *testing.T) {
	mapA := sdWorld(t)
	hero, _ := mapA.SpawnHero(hPaladin, 0, 0, pt2(100, 100))
	mapA.AddInventory(hero) // empty — instantiation always attaches one
	sd, err := mapA.ExtractSaveData(hero)
	if err != nil {
		t.Fatal(err)
	}
	t.Logf("minimal record: %+v items=%+v", sd.Hero, sd.Items)
	wire := EncodeSaveData(nil, &sd)
	dec, _, err := DecodeSaveData(wire)
	if err != nil || dec != sd {
		t.Fatalf("minimal round-trip: %v", err)
	}
	mapB := sdWorld(t)
	hero2, err := mapB.InstantiateSaveData(&dec, 0, 0, pt2(100, 100))
	if err != nil {
		t.Fatal(err)
	}
	t.Logf("map A: %s", sdFields(mapA, hero))
	t.Logf("map B: %s", sdFields(mapB, hero2))
	if sdFields(mapA, hero) != sdFields(mapB, hero2) {
		t.Fatal("minimal carry-over differs")
	}
}

// Decode is fail-closed: truncation, alien version, flag garbage,
// and a dead consumable in the record all refuse by name.
func TestSaveDataFailClosed(t *testing.T) {
	mapA := sdWorld(t)
	hero, _ := mapA.SpawnHero(hPaladin, 0, 0, pt2(100, 100))
	sd, _ := mapA.ExtractSaveData(hero)
	wire := EncodeSaveData(nil, &sd)

	for _, c := range []struct {
		name string
		b    []byte
	}{
		{"empty", nil},
		{"bad version", append([]byte{99}, wire[1:]...)},
		{"truncated hero", wire[:12]},
		{"truncated items", wire[:len(wire)-3]},
	} {
		if _, _, err := DecodeSaveData(c.b); err == nil {
			t.Errorf("%s: accepted", c.name)
		} else {
			t.Logf("%s: %v", c.name, err)
		}
	}
	bad := wire[:len(wire):len(wire)]
	bad = append(bad[:len(bad)-6:len(bad)-6], 7) // slot flag garbage
	if _, _, err := DecodeSaveData(bad); err == nil {
		t.Error("flag garbage accepted")
	}
	// dead consumable: type 1 (sd-wand) with 0 charges
	sd.Items[2] = SavedItem{Used: true, TypeID: 1, Charges: 0}
	mapB := sdWorld(t)
	if _, err := mapB.InstantiateSaveData(&sd, 0, 0, pt2(100, 100)); err == nil {
		t.Error("dead consumable accepted")
	} else {
		t.Logf("dead consumable: %v", err)
	}
	// unknown item type
	sd.Items[2] = SavedItem{Used: true, TypeID: 9, Charges: 1}
	if _, err := mapB.InstantiateSaveData(&sd, 0, 0, pt2(100, 100)); err == nil {
		t.Error("unknown item type accepted")
	} else {
		t.Logf("unknown type: %v", err)
	}
}

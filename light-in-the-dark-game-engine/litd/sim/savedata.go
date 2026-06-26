package sim

// Campaign persistence store (#208, D-15; combat-and-orders.md §5.3,
// determinism.md §6): a SaveData record carries a hero across maps
// with game-cache semantics. The record is SELF-CONTAINED — type IDs
// and instance fields only, never EntityIDs (those are match-local).
// Extraction is a column copy of the hero row + inventory slots, not
// a graph walk; the destination map allocates fresh entities and
// replays the record into them. Derived stats (buffs, auras, item
// folds) are NOT carried — they re-derive from the destination's
// tables + the record.
//
// The record is versioned with the data-table content hash
// (Tables.Fingerprint): extraction stamps the source world's
// fingerprint, instantiation refuses a mismatch by name. The same
// format serves mid-game saves and the game-cache API analogues.

import (
	"encoding/binary"
	"fmt"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
)

// SaveDataVersion bumps on record layout change.
const SaveDataVersion uint8 = 1

// SavedItem is one carried inventory slot (D-15 instance fields).
type SavedItem struct {
	Used    bool
	TypeID  uint16
	Charges uint16
}

// SaveData is the D-15 carry-over record.
type SaveData struct {
	Fingerprint uint64 // data-table content hash at extraction
	Hero        HeroRecord
	Items       [InventorySlots]SavedItem // slot layout preserved
}

// SetDataFingerprint installs the loaded table content hash
// (Tables.Fingerprint). Engine init calls this once after binding the
// tables; SaveData extraction and instantiation are refused without
// it (fail closed — an unversioned record cannot be validated).
func (w *World) SetDataFingerprint(fp uint64) { w.dataFingerprint = fp }

// ExtractSaveData snapshots a live hero (+ inventory) as a
// self-contained record. Column copy: hero row fields and the slot
// array's type/charge columns.
func (w *World) ExtractSaveData(hero EntityID) (SaveData, error) {
	if w.dataFingerprint == 0 {
		return SaveData{}, fmt.Errorf("sim: savedata: no data fingerprint set — SetDataFingerprint before extraction")
	}
	rec, ok := w.ExtractHeroRecord(hero)
	if !ok {
		return SaveData{}, fmt.Errorf("sim: savedata: entity %d has no hero row", hero)
	}
	sd := SaveData{Fingerprint: w.dataFingerprint, Hero: rec}
	if ir := w.Invents.Row(hero); ir != -1 {
		for s := 0; s < InventorySlots; s++ {
			item := w.Invents.Slots[ir][s]
			if item == 0 {
				continue
			}
			r := w.Items.Row(item)
			if r == -1 {
				return SaveData{}, fmt.Errorf("sim: savedata: slot %d holds entity %d with no item row", s, item)
			}
			sd.Items[s] = SavedItem{Used: true, TypeID: w.Items.TypeID[r], Charges: w.Items.Charges[r]}
		}
	}
	return sd, nil
}

// InstantiateSaveData replays a record onto THIS world: fingerprint
// check, fresh hero entity, fresh item entities slotted at their
// recorded positions, derived stats re-derived from the bound tables.
func (w *World) InstantiateSaveData(sd *SaveData, player, team uint8, pos fixed.Vec2) (EntityID, error) {
	if w.dataFingerprint == 0 {
		return 0, fmt.Errorf("sim: savedata: no data fingerprint set — SetDataFingerprint before instantiation")
	}
	if sd.Fingerprint != w.dataFingerprint {
		return 0, fmt.Errorf("sim: savedata: record taken under table hash %016x, this world runs %016x — refusing a cross-version replay",
			sd.Fingerprint, w.dataFingerprint)
	}
	for s := range sd.Items {
		it := &sd.Items[s]
		if !it.Used {
			continue
		}
		if w.itemDefs == nil || int(it.TypeID) >= len(w.itemDefs) {
			return 0, fmt.Errorf("sim: savedata: slot %d item type %d outside the bound table", s, it.TypeID)
		}
		if w.itemDefs[it.TypeID].Consumable && it.Charges == 0 {
			return 0, fmt.Errorf("sim: savedata: slot %d consumable %q with 0 charges (would be a dead item)", s, w.itemDefs[it.TypeID].ID)
		}
	}
	hero, ok := w.InstantiateHero(&sd.Hero, player, team, pos)
	if !ok {
		return 0, fmt.Errorf("sim: savedata: hero instantiation refused (tables unbound, bad record bounds, or caps)")
	}
	if w.Invents.Row(hero) == -1 && !w.AddInventory(hero) {
		w.DestroyUnit(hero)
		return 0, fmt.Errorf("sim: savedata: inventory attach failed")
	}
	for s := range sd.Items {
		it := &sd.Items[s]
		if !it.Used {
			continue
		}
		id, ok2 := w.Ents.Create()
		if !ok2 || !w.Items.add(w.Ents, id, it.TypeID, it.Charges) {
			if ok2 {
				w.Ents.Destroy(id)
			}
			w.DestroyUnit(hero)
			return 0, fmt.Errorf("sim: savedata: item entity allocation failed at slot %d", s)
		}
		w.unitCount++
		w.Items.Carrier[w.Items.Row(id)] = hero
		w.Invents.SetSlot(hero, s, id)
	}
	w.recomputeBuffStats(hero) // re-derive: tables + record, never carried
	return hero, nil
}

// ---- wire format (mid-game saves and game-cache analogues) ----

// EncodeSaveData appends the record to dst, little-endian.
func EncodeSaveData(dst []byte, sd *SaveData) []byte {
	dst = append(dst, SaveDataVersion)
	dst = binary.LittleEndian.AppendUint64(dst, sd.Fingerprint)
	h := &sd.Hero
	dst = binary.LittleEndian.AppendUint16(dst, h.HeroType)
	dst = binary.LittleEndian.AppendUint64(dst, uint64(h.XP))
	dst = append(dst, h.Level)
	dst = binary.LittleEndian.AppendUint64(dst, uint64(h.Str))
	dst = binary.LittleEndian.AppendUint64(dst, uint64(h.Agi))
	dst = binary.LittleEndian.AppendUint64(dst, uint64(h.Int))
	dst = append(dst, h.SkillPoints)
	for s := 0; s < data.MaxHeroSkills; s++ {
		dst = append(dst, h.SkillLevel[s])
	}
	for s := range sd.Items {
		it := &sd.Items[s]
		if !it.Used {
			dst = append(dst, 0)
			continue
		}
		dst = append(dst, 1)
		dst = binary.LittleEndian.AppendUint16(dst, it.TypeID)
		dst = binary.LittleEndian.AppendUint16(dst, it.Charges)
	}
	return dst
}

// DecodeSaveData parses one record, returning the remaining bytes.
// Fail-closed on truncation, version, or flag garbage.
func DecodeSaveData(b []byte) (SaveData, []byte, error) {
	var sd SaveData
	fail := func(what string) (SaveData, []byte, error) {
		return SaveData{}, nil, fmt.Errorf("sim: savedata: decode: %s", what)
	}
	if len(b) < 1 {
		return fail("empty input")
	}
	if b[0] != SaveDataVersion {
		return SaveData{}, nil, fmt.Errorf("sim: savedata: decode: version %d, this engine reads %d", b[0], SaveDataVersion)
	}
	b = b[1:]
	need := func(n int) bool { return len(b) >= n }
	if !need(8) {
		return fail("truncated at fingerprint")
	}
	sd.Fingerprint = binary.LittleEndian.Uint64(b)
	b = b[8:]
	heroFixed := 2 + 8 + 1 + 8 + 8 + 8 + 1 + data.MaxHeroSkills
	if !need(heroFixed) {
		return fail("truncated in hero block")
	}
	h := &sd.Hero
	h.Used = true
	h.HeroType = binary.LittleEndian.Uint16(b)
	h.XP = int64(binary.LittleEndian.Uint64(b[2:]))
	h.Level = b[10]
	h.Str = fixed.F64(binary.LittleEndian.Uint64(b[11:]))
	h.Agi = fixed.F64(binary.LittleEndian.Uint64(b[19:]))
	h.Int = fixed.F64(binary.LittleEndian.Uint64(b[27:]))
	h.SkillPoints = b[35]
	for s := 0; s < data.MaxHeroSkills; s++ {
		h.SkillLevel[s] = b[36+s]
	}
	b = b[heroFixed:]
	for s := range sd.Items {
		if !need(1) {
			return fail("truncated at item flags")
		}
		flag := b[0]
		b = b[1:]
		switch flag {
		case 0:
			continue
		case 1:
			if !need(4) {
				return fail("truncated in item block")
			}
			sd.Items[s] = SavedItem{Used: true,
				TypeID:  binary.LittleEndian.Uint16(b),
				Charges: binary.LittleEndian.Uint16(b[2:])}
			b = b[4:]
		default:
			return fail(fmt.Sprintf("item slot flag %d not 0/1", flag))
		}
	}
	return sd, b, nil
}

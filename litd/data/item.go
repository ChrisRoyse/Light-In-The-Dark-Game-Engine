package data

// Item tables (#305, ecs-architecture.md §5: items are entities;
// items.md jass-mapping): items/*.toml|json define the item-type
// vocabulary — class, shop costs, charges, the use-effect pipeline
// (compiled into the shared ADR #294 arena), carried stat modifiers
// (the #162 vocabulary), and behavior flags. Everything fails closed
// at load; the converted rows fold into the determinism fingerprint.

import (
	"fmt"
	"io/fs"
	"sort"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/statehash"
)

// ItemClassCount sizes per-class state arrays in the sim (use
// cooldowns are shared per class, WC3-style).
const ItemClassCount = 7

// ItemClasses is the closed class vocabulary (WC3 item classes).
// Index = Item.Class. Appending is safe, reordering is a sim-version
// change.
var ItemClasses = [ItemClassCount]string{
	"permanent", "charged", "power-up", "artifact",
	"purchasable", "campaign", "miscellaneous",
}

// Item is one converted item row. Sim units throughout.
type Item struct {
	ID            string
	Name          string
	Class         uint8      // index into ItemClasses
	Costs         []int64    // per resource index (nil = not purchasable)
	Charges       uint16     // initial charges; 0 = uncharged
	CooldownTicks uint16     // per-class use cooldown
	Targeted      bool       // use requires an entity target
	UseRange      fixed.F64  // max use distance (targeted only)
	Effects       EffectList // use pipeline; zero-length = passive item
	Mods          []StatMod  // carried modifiers (#162 fold)
	Consumable    bool       // removed when charges reach 0
	DropOnDeath   bool       // carrier death grounds the item
	PowerUp       bool       // consumed instantly on pickup (effect fires, item destroyed)
}

type rawItemFile struct {
	Item []rawItem `toml:"item" json:"item"`
}

type rawItem struct {
	ID          string           `toml:"id" json:"id"`
	Name        string           `toml:"name" json:"name"`
	Class       string           `toml:"class" json:"class"`
	Costs       map[string]int64 `toml:"costs" json:"costs"`
	Charges     int64            `toml:"charges" json:"charges"`
	Cooldown    float64          `toml:"cooldown" json:"cooldown"` // seconds
	Targeted    bool             `toml:"targeted" json:"targeted"`
	UseRange    float64          `toml:"use-range" json:"use-range"` // world units
	Effects     []map[string]any `toml:"effects" json:"effects"`
	Mods        []rawStatMod     `toml:"mod" json:"mod"`
	Consumable  bool             `toml:"consumable" json:"consumable"`
	DropOnDeath bool             `toml:"drop-on-death" json:"drop-on-death"`
	PowerUp     bool             `toml:"power-up" json:"power-up"`
}

// loadItems reads items/*.toml|json (optional directory). The caller
// owns the shared effect compiler; item use pipelines land in the
// same arena as buffs and abilities.
func (t *Tables) loadItems(fsys fs.FS, comp *effectCompiler) error {
	files, err := listTables(fsys, "items")
	if err != nil || len(files) == 0 {
		return err
	}
	type pendingItem struct {
		file string
		raw  rawItem
	}
	var pending []pendingItem
	for _, f := range files {
		blob, err := fs.ReadFile(fsys, f)
		if err != nil {
			return fmt.Errorf("data: %s: %w", f, err)
		}
		var raw rawItemFile
		if err := decodeStrict(f, blob, &raw, "item.effects"); err != nil {
			return err
		}
		for i := range raw.Item {
			if raw.Item[i].ID == "" {
				return fmt.Errorf("data: %s: item with empty id", f)
			}
			pending = append(pending, pendingItem{file: f, raw: raw.Item[i]})
		}
	}
	sort.Slice(pending, func(i, j int) bool { return pending[i].raw.ID < pending[j].raw.ID })
	for i := 1; i < len(pending); i++ {
		if pending[i].raw.ID == pending[i-1].raw.ID {
			return fmt.Errorf("data: duplicate item id %q (%s, %s)",
				pending[i].raw.ID, pending[i-1].file, pending[i].file)
		}
	}
	for _, p := range pending {
		it, err := t.convertItem(p.file, &p.raw)
		if err != nil {
			return err
		}
		if p.raw.Effects != nil {
			comp.file = p.file
			where := fmt.Sprintf("item %q effects", p.raw.ID)
			lst, inv, err := comp.compile(where, p.raw.Effects, 1)
			if err != nil {
				return err
			}
			if inv > MaxEffectInvocations {
				return fmt.Errorf("data: %s: %s: worst-case invocation count %d exceeds ceiling %d",
					p.file, where, inv, MaxEffectInvocations)
			}
			it.Effects = lst
		}
		// usable-surface coherence: passive items declare none of the
		// use knobs; targeted use needs a positive range
		if it.Effects.Len == 0 {
			if it.Targeted || it.UseRange != 0 || it.CooldownTicks != 0 {
				return fmt.Errorf("data: %s: item %q: targeted/use-range/cooldown require an effects list (passive items have no use)", p.file, p.raw.ID)
			}
			if it.Consumable {
				return fmt.Errorf("data: %s: item %q: consumable requires an effects list", p.file, p.raw.ID)
			}
		}
		if it.Targeted && it.UseRange <= 0 {
			return fmt.Errorf("data: %s: item %q: targeted use requires use-range > 0", p.file, p.raw.ID)
		}
		if it.Consumable && it.Charges == 0 {
			return fmt.Errorf("data: %s: item %q: consumable requires charges >= 1", p.file, p.raw.ID)
		}
		if it.PowerUp {
			if it.Effects.Len == 0 {
				return fmt.Errorf("data: %s: item %q: power-up requires an effects list (it fires on pickup)", p.file, p.raw.ID)
			}
			if it.Consumable || it.Targeted {
				return fmt.Errorf("data: %s: item %q: power-up is instant + self-applied (not consumable/targeted)", p.file, p.raw.ID)
			}
		}
		t.Items = append(t.Items, it)
	}
	return nil
}

func (t *Tables) convertItem(file string, r *rawItem) (Item, error) {
	fail := func(field string, err error) (Item, error) {
		return Item{}, fmt.Errorf("data: %s: item %q: %s: %w", file, r.ID, field, err)
	}
	it := Item{ID: r.ID, Name: r.Name, Targeted: r.Targeted,
		Consumable: r.Consumable, DropOnDeath: r.DropOnDeath, PowerUp: r.PowerUp}
	ci := indexOf(ItemClasses[:], r.Class)
	if ci < 0 {
		return fail("class", fmt.Errorf("%q is not one of %v", r.Class, ItemClasses))
	}
	it.Class = uint8(ci)
	costs, err := t.costMap(file+": item "+r.ID+" costs", r.Costs)
	if err != nil {
		return Item{}, err
	}
	it.Costs = costs
	if r.Charges < 0 || r.Charges > 65535 {
		return fail("charges", fmt.Errorf("value %d out of range [0, 65535]", r.Charges))
	}
	it.Charges = uint16(r.Charges)
	cd, err := SecondsToTicks(r.Cooldown)
	if err != nil {
		return fail("cooldown", err)
	}
	it.CooldownTicks = cd
	ur, err := worldUnits(r.UseRange)
	if err != nil {
		return fail("use-range", err)
	}
	it.UseRange = ur
	for i := range r.Mods {
		m := &r.Mods[i]
		mi := indexOf(buffStatNames[:], m.Stat)
		if mi < 0 {
			return fail("mod.stat", fmt.Errorf("%q is not move-speed|armor|attack-cooldown|attack-damage", m.Stat))
		}
		sm := StatMod{Stat: uint8(mi), Permille: 1000}
		if m.Permille != 0 {
			if m.Permille < 0 || m.Permille > 10000 {
				return fail("mod.permille", fmt.Errorf("value %d out of range [0, 10000]", m.Permille))
			}
			sm.Permille = int32(m.Permille)
		}
		add, err := convertStatAdd(sm.Stat, m.Add)
		if err != nil {
			return fail("mod.add", err)
		}
		sm.Add = add
		it.Mods = append(it.Mods, sm)
	}
	return it, nil
}

// hashItems folds the converted item rows into the fingerprint.
func (t *Tables) hashItems(h *statehash.Hasher) {
	h.WriteU32(uint32(len(t.Items)))
	for i := range t.Items {
		it := &t.Items[i]
		writeString(h, it.ID)
		writeString(h, it.Name)
		h.WriteU8(it.Class)
		h.WriteU32(uint32(len(it.Costs)))
		for _, c := range it.Costs {
			h.WriteI64(c)
		}
		h.WriteU16(it.Charges)
		h.WriteU16(it.CooldownTicks)
		h.WriteBool(it.Targeted)
		h.WriteI64(int64(it.UseRange))
		h.WriteU16(it.Effects.Off)
		h.WriteU16(it.Effects.Len)
		h.WriteU32(uint32(len(it.Mods)))
		for j := range it.Mods {
			m := &it.Mods[j]
			h.WriteU8(m.Stat)
			h.WriteI64(m.Add)
			h.WriteU32(uint32(m.Permille))
		}
		h.WriteBool(it.Consumable)
		h.WriteBool(it.DropOnDeath)
		h.WriteBool(it.PowerUp)
	}
}

// itemIndex finds an item row by ID (sorted order). -1 when absent.
func (t *Tables) itemIndex(id string) int {
	i := sort.Search(len(t.Items), func(k int) bool { return t.Items[k].ID >= id })
	if i == len(t.Items) || t.Items[i].ID != id {
		return -1
	}
	return i
}

package data

// #305 item-table tests. SoT = converted Item rows, the shared
// effect arena, and the named refusal errors.

import (
	"strings"
	"testing"
	"testing/fstest"
)

const itemBuff = `
[[buff]]
id = "haste"
name = "Haste"
duration = 5.0
stacking = "refresh"
[[buff.mod]]
stat = "move-speed"
add = 60.0
`

const itemPotionTOML = `
[[item]]
id = "claws"
name = "Claws of Attack"
class = "permanent"
drop-on-death = true
[item.costs]
gold = 100
[[item.mod]]
stat = "attack-damage"
add = 2

[[item]]
id = "potion"
name = "Healing Potion"
class = "charged"
charges = 2
consumable = true
cooldown = 1.0
[[item.effects]]
prim = "heal"
amount = 50

[[item]]
id = "wand"
name = "Wand of Haste"
class = "charged"
charges = 3
targeted = true
use-range = 500.0
[[item.effects]]
prim = "apply-buff"
buff = "haste"
`

func loadItemTables(t *testing.T, items string) (*Tables, error) {
	t.Helper()
	fs := econFS(econBase, econUnit)
	fs["buffs/core.toml"] = &fstest.MapFile{Data: []byte(itemBuff)}
	if items != "" {
		fs["items/core.toml"] = &fstest.MapFile{Data: []byte(items)}
	}
	return Load(fs)
}

func TestItemTablesHappyPath(t *testing.T) {
	tb, err := loadItemTables(t, itemPotionTOML)
	if err != nil {
		t.Fatal(err)
	}
	if len(tb.Items) != 3 {
		t.Fatalf("items: %d", len(tb.Items))
	}
	claws := &tb.Items[tb.itemIndex("claws")]
	if claws.Class != 0 || !claws.DropOnDeath || claws.Consumable ||
		claws.Costs[0] != 100 || claws.Effects.Len != 0 {
		t.Fatalf("claws: %+v", claws)
	}
	if len(claws.Mods) != 1 || claws.Mods[0].Stat != StatAttackDamage ||
		claws.Mods[0].Add != 2<<32 || claws.Mods[0].Permille != 1000 {
		t.Fatalf("claws mods: %+v", claws.Mods)
	}
	pot := &tb.Items[tb.itemIndex("potion")]
	// class "charged"=1; 1.0s at 20 t/s = 20 ticks
	if pot.Class != 1 || pot.Charges != 2 || !pot.Consumable ||
		pot.CooldownTicks != 20 || pot.Targeted || pot.Effects.Len != 1 {
		t.Fatalf("potion: %+v", pot)
	}
	if pe := tb.Effects[pot.Effects.Off]; pe.Prim != EPHeal || pe.Params[0] != 50 {
		t.Fatalf("potion effect: %+v", pe)
	}
	wand := &tb.Items[tb.itemIndex("wand")]
	if !wand.Targeted || wand.UseRange != 500<<32 || wand.Charges != 3 || wand.Consumable {
		t.Fatalf("wand: %+v", wand)
	}
	we := tb.Effects[wand.Effects.Off]
	if we.Prim != EPApplyBuff || int(we.Params[0]) != 0 { // "haste" = buff index 0
		t.Fatalf("wand effect: %+v", we)
	}
	t.Logf("claws: %+v", claws)
	t.Logf("potion: %+v effect=%+v", pot, tb.Effects[pot.Effects.Off])
	t.Logf("wand: %+v effect=%+v", wand, we)
}

func TestItemTablesFailClosed(t *testing.T) {
	cases := []struct {
		name, items, wantErr string
	}{
		{"unknown class", strings.Replace(itemPotionTOML, `class = "permanent"`, `class = "legendary"`, 1), "is not one of"},
		{"unknown cost resource", strings.Replace(itemPotionTOML, "[item.costs]\ngold = 100", "[item.costs]\noil = 100", 1), `resource "oil" is not in resource-types`},
		{"unknown mod stat", strings.Replace(itemPotionTOML, `stat = "attack-damage"`, `stat = "luck"`, 1), "is not move-speed|armor|attack-cooldown|attack-damage"},
		{"charges out of range", strings.Replace(itemPotionTOML, "charges = 2", "charges = 70000", 1), "out of range [0, 65535]"},
		{"consumable without effects", strings.Replace(strings.Replace(itemPotionTOML,
			"[[item.effects]]\nprim = \"heal\"\namount = 50\n", "", 1),
			"cooldown = 1.0\n", "", 1), "consumable requires an effects list"},
		{"targeted without range", strings.Replace(itemPotionTOML, "use-range = 500.0\n", "", 1), "targeted use requires use-range > 0"},
		{"passive with cooldown", strings.Replace(itemPotionTOML, `class = "permanent"`, "class = \"permanent\"\ncooldown = 2.0", 1), "require an effects list"},
		{"unknown effect prim", strings.Replace(itemPotionTOML, `prim = "heal"`, `prim = "bless"`, 1), "is not a registered effect primitive"},
		{"unknown buff ref", strings.Replace(itemPotionTOML, `buff = "haste"`, `buff = "rage"`, 1), "is not a defined buff"},
		{"duplicate id", itemPotionTOML + "\n[[item]]\nid = \"claws\"\nclass = \"permanent\"\n", "duplicate item id"},
		{"empty id", strings.Replace(itemPotionTOML, `id = "claws"`, `id = ""`, 1), "item with empty id"},
		{"unknown key", itemPotionTOML + "\nbogus = 1\n", "bogus"},
	}
	for _, c := range cases {
		_, err := loadItemTables(t, c.items)
		if err == nil {
			t.Errorf("%s: accepted", c.name)
			continue
		}
		if !strings.Contains(err.Error(), c.wantErr) {
			t.Errorf("%s: error %q does not name %q", c.name, err, c.wantErr)
		}
		t.Logf("%s: %v", c.name, err)
	}
}

func TestItemFingerprintSensitivity(t *testing.T) {
	base, err := loadItemTables(t, itemPotionTOML)
	if err != nil {
		t.Fatal(err)
	}
	for name, mutated := range map[string]string{
		"mod":     strings.Replace(itemPotionTOML, "add = 2", "add = 3", 1),
		"charges": strings.Replace(itemPotionTOML, "charges = 2", "charges = 4", 1),
		"effect":  strings.Replace(itemPotionTOML, "amount = 50", "amount = 51", 1),
		"flag":    strings.Replace(itemPotionTOML, "drop-on-death = true", "drop-on-death = false", 1),
	} {
		changed, err := loadItemTables(t, mutated)
		if err != nil {
			t.Fatalf("%s: %v", name, err)
		}
		if base.Fingerprint == changed.Fingerprint {
			t.Errorf("%s change did not move the fingerprint", name)
		}
		t.Logf("%s: base=%016x changed=%016x", name, base.Fingerprint, changed.Fingerprint)
	}
}

package data

// #303 tech-table tests. SoT = converted Upgrade/Require rows, unit
// Researches lists, and the named refusal errors.

import (
	"strings"
	"testing"
	"testing/fstest"
)

const techUnits = prodUnit + `
[[unit]]
id = "barracks"
life = 1200
armor = 3
armor-type = "none"
move-speed = 0
turn-rate = 0
collision-size = 48
pathing = "ground"
acquisition-range = 0
model = "units/barracks.glb"
researches = ["iron-blades"]
`

const techTOML = `
[[upgrade]]
id = "iron-blades"
applies-to = ["worker"]

[[upgrade.level]]
research-seconds = 20.0
[upgrade.level.costs]
gold = 100

[[upgrade.level]]
research-seconds = 30.0
[upgrade.level.costs]
gold = 150
lumber = 50

[[upgrade.mod]]
stat = "attack-damage"
add = 2

[[upgrade.mod]]
stat = "armor"
add = 1
permille = 1000

[[require]]
unit = "worker"
alive = ["barracks"]
[require.upgrades]
iron-blades = 1

[[require]]
upgrade = "iron-blades"
alive = ["hall"]
`

func loadTechTables(t *testing.T, units, tech string) (*Tables, error) {
	t.Helper()
	fs := econFS(econBase, units)
	if tech != "" {
		fs["tech/upgrades.toml"] = &fstest.MapFile{Data: []byte(tech)}
	}
	return Load(fs)
}

func TestTechHappyPath(t *testing.T) {
	tb, err := loadTechTables(t, techUnits, techTOML)
	if err != nil {
		t.Fatal(err)
	}
	if len(tb.Upgrades) != 1 {
		t.Fatalf("upgrades: %d", len(tb.Upgrades))
	}
	u := &tb.Upgrades[0]
	// sorted units: barracks(0) hall(1) worker(2)
	if u.ID != "iron-blades" || len(u.Levels) != 2 {
		t.Fatalf("upgrade row: %+v", u)
	}
	if u.Levels[0].ResearchTicks != 400 || u.Levels[0].Costs[0] != 100 || u.Levels[0].Costs[1] != 0 {
		t.Fatalf("level 1: %+v", u.Levels[0])
	}
	if u.Levels[1].ResearchTicks != 600 || u.Levels[1].Costs[0] != 150 || u.Levels[1].Costs[1] != 50 {
		t.Fatalf("level 2: %+v", u.Levels[1])
	}
	if len(u.Mods) != 2 || u.Mods[0].Stat != StatAttackDamage || u.Mods[0].Add != 2<<32 ||
		u.Mods[1].Stat != StatArmor || u.Mods[1].Add != 1 || u.Mods[1].Permille != 1000 {
		t.Fatalf("mods: %+v", u.Mods)
	}
	if len(u.AppliesTo) != 1 || tb.Units[u.AppliesTo[0]].ID != "worker" {
		t.Fatalf("applies-to: %v", u.AppliesTo)
	}
	if len(tb.Requires) != 2 {
		t.Fatalf("requires: %d", len(tb.Requires))
	}
	ru := tb.Requires[0] // unit rows sort before upgrade rows
	if ru.IsUpgrade || tb.Units[ru.Target].ID != "worker" ||
		len(ru.Upgrades) != 1 || ru.Upgrades[0] != (ReqTerm{Upgrade: 0, Level: 1}) ||
		len(ru.Alive) != 1 || tb.Units[ru.Alive[0]].ID != "barracks" {
		t.Fatalf("unit require: %+v", ru)
	}
	rg := tb.Requires[1]
	if !rg.IsUpgrade || rg.Target != 0 || len(rg.Alive) != 1 || tb.Units[rg.Alive[0]].ID != "hall" {
		t.Fatalf("upgrade require: %+v", rg)
	}
	var barracks *Unit
	for i := range tb.Units {
		if tb.Units[i].ID == "barracks" {
			barracks = &tb.Units[i]
		}
	}
	if len(barracks.Researches) != 1 || barracks.Researches[0] != 0 {
		t.Fatalf("barracks researches: %v", barracks.Researches)
	}
	t.Logf("upgrade: %+v", *u)
	t.Logf("requires: %+v", tb.Requires)
	t.Logf("barracks.Researches: %v", barracks.Researches)
}

func TestTechFailClosed(t *testing.T) {
	cases := []struct {
		name, units, tech, wantErr string
	}{
		{"no levels", techUnits, strings.Replace(strings.Replace(techTOML, "[[upgrade.level]]\nresearch-seconds = 20.0\n[upgrade.level.costs]\ngold = 100\n", "", 1), "[[upgrade.level]]\nresearch-seconds = 30.0\n[upgrade.level.costs]\ngold = 150\nlumber = 50\n", "", 1), "levels out of range"},
		{"unknown cost resource", techUnits, strings.Replace(techTOML, "gold = 100", "oil = 100", 1), `resource "oil" is not in resource-types`},
		{"unknown mod stat", techUnits, strings.Replace(techTOML, `stat = "attack-damage"`, `stat = "mana"`, 1), "mod.stat"},
		{"unknown applies-to", techUnits, strings.Replace(techTOML, `applies-to = ["worker"]`, `applies-to = ["ghost"]`, 1), `unit "ghost" is not defined`},
		{"require both targets", techUnits, strings.Replace(techTOML, `unit = "worker"`, "unit = \"worker\"\nupgrade = \"iron-blades\"", 1), "exactly one target"},
		{"require no terms", techUnits, techTOML + "\n[[require]]\nunit = \"hall\"\n", "no terms"},
		{"require level over max", techUnits, strings.Replace(techTOML, "iron-blades = 1", "iron-blades = 3", 1), "out of range [1, 2]"},
		{"require unknown alive", techUnits, strings.Replace(techTOML, `alive = ["barracks"]`, `alive = ["keep"]`, 1), `alive unit "keep" is not defined`},
		{"researches without tech table", techUnits, "", "researches but no tech"},
		{"researches unknown upgrade", strings.Replace(techUnits, `researches = ["iron-blades"]`, `researches = ["masonry"]`, 1), techTOML, `researches reference to undefined upgrade "masonry"`},
		{"zero research seconds", techUnits, strings.Replace(techTOML, "research-seconds = 20.0", "research-seconds = -1.0", 1), "research-seconds"},
		{"duplicate require target", techUnits, techTOML + "\n[[require]]\nunit = \"worker\"\nalive = [\"hall\"]\n", "duplicate requirement target"},
	}
	for _, c := range cases {
		_, err := loadTechTables(t, c.units, c.tech)
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

func TestTechFingerprintSensitivity(t *testing.T) {
	base, err := loadTechTables(t, techUnits, techTOML)
	if err != nil {
		t.Fatal(err)
	}
	for name, mutated := range map[string]string{
		"cost":     strings.Replace(techTOML, "gold = 100", "gold = 101", 1),
		"mod add":  strings.Replace(techTOML, "add = 2", "add = 3", 1),
		"req term": strings.Replace(techTOML, "iron-blades = 1", "iron-blades = 2", 1),
	} {
		changed, err := loadTechTables(t, techUnits, mutated)
		if err != nil {
			t.Fatalf("%s: %v", name, err)
		}
		if base.Fingerprint == changed.Fingerprint {
			t.Errorf("%s change did not move the fingerprint", name)
		}
		t.Logf("%s: base=%016x changed=%016x", name, base.Fingerprint, changed.Fingerprint)
	}
}

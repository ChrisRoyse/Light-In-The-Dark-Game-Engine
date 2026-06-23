package data

// #155/#156 grimoire-track tests. SoT = converted Grimoire rows (slot,
// researched-at building, ordered tiers, resolved grant indices) and the named
// refusal errors.

import (
	"strings"
	"testing"
	"testing/fstest"
)

const grimUnits = `
[[unit]]
id = "worker"
life = 100
armor = 0
armor-type = "none"
move-speed = 200
turn-rate = 1.0
collision-size = 16
pathing = "ground"
acquisition-range = 0
model = "u.glb"

[[unit]]
id = "lodge"
life = 800
armor = 2
armor-type = "none"
move-speed = 0
turn-rate = 0
collision-size = 48
pathing = "ground"
acquisition-range = 0
model = "b.glb"
footprint = 3
build-seconds = 20.0
researches = ["focus"]
`

const grimTech = `
[[upgrade]]
id = "focus"
[[upgrade.level]]
research-seconds = 10.0
[[upgrade.mod]]
stat = "armor"
add = 1
`

const grimAbilities = `
[[ability]]
id = "blink"
name = "Blink"
`

const grimTOML = `
[[grimoire]]
id = "test_tome"
name = "Test Tome"
slot = "doctrine"
researched-at = "lodge"
[[grimoire.tier]]
research-seconds = 15.0
costs = { gold = 50 }
grants-upgrades = ["focus"]
[[grimoire.tier]]
research-seconds = 25.0
grants-units = ["worker"]
grants-abilities = ["blink"]
`

func loadGrimoireTables(t *testing.T, grim string) (*Tables, error) {
	t.Helper()
	fs := econFS(econBase, grimUnits)
	fs["tech/upgrades.toml"] = &fstest.MapFile{Data: []byte(grimTech)}
	fs["abilities/core.toml"] = &fstest.MapFile{Data: []byte(grimAbilities)}
	if grim != "" {
		fs["grimoires/test.toml"] = &fstest.MapFile{Data: []byte(grim)}
	}
	return Load(fs)
}

func TestGrimoireHappyPath(t *testing.T) {
	tb, err := loadGrimoireTables(t, grimTOML)
	if err != nil {
		t.Fatal(err)
	}
	if len(tb.Grimoires) != 1 {
		t.Fatalf("grimoires: %d", len(tb.Grimoires))
	}
	g := &tb.Grimoires[0]
	if g.ID != "test_tome" || g.Slot != "doctrine" {
		t.Fatalf("grimoire identity: %+v", g)
	}
	// sorted units: lodge(0) worker(1)
	if tb.Units[g.ResearchedAt].ID != "lodge" {
		t.Fatalf("researched-at did not resolve to lodge: %d", g.ResearchedAt)
	}
	if len(g.Tiers) != 2 {
		t.Fatalf("tiers: %d", len(g.Tiers))
	}
	// tier 1: 15s -> 300 ticks, grants upgrade focus
	if g.Tiers[0].ResearchTicks != 300 || g.Tiers[0].Costs[0] != 50 {
		t.Fatalf("tier 0: %+v", g.Tiers[0])
	}
	if len(g.Tiers[0].Upgrades) != 1 || tb.Upgrades[g.Tiers[0].Upgrades[0]].ID != "focus" {
		t.Fatalf("tier 0 upgrade grant: %+v", g.Tiers[0])
	}
	// tier 2: 25s -> 500 ticks, grants worker unit + blink ability
	if g.Tiers[1].ResearchTicks != 500 {
		t.Fatalf("tier 1 ticks: %d", g.Tiers[1].ResearchTicks)
	}
	if len(g.Tiers[1].Units) != 1 || tb.Units[g.Tiers[1].Units[0]].ID != "worker" {
		t.Fatalf("tier 1 unit grant: %+v", g.Tiers[1])
	}
	if len(g.Tiers[1].Abilities) != 1 || tb.Abilities[g.Tiers[1].Abilities[0]].ID != "blink" {
		t.Fatalf("tier 1 ability grant: %+v", g.Tiers[1])
	}
}

func TestGrimoireFailClosed(t *testing.T) {
	cases := map[string]string{
		"undefined upgrade": strings.Replace(grimTOML, `grants-upgrades = ["focus"]`, `grants-upgrades = ["ghost"]`, 1),
		"undefined unit":    strings.Replace(grimTOML, `grants-units = ["worker"]`, `grants-units = ["ghost"]`, 1),
		"empty tier":        strings.Replace(grimTOML, `grants-upgrades = ["focus"]`, ``, 1),
		"not a building":    strings.Replace(grimTOML, `researched-at = "lodge"`, `researched-at = "worker"`, 1),
		"undefined builder": strings.Replace(grimTOML, `researched-at = "lodge"`, `researched-at = "ghost"`, 1),
		"empty slot":        strings.Replace(grimTOML, `slot = "doctrine"`, `slot = ""`, 1),
	}
	for name, bad := range cases {
		if _, err := loadGrimoireTables(t, bad); err == nil {
			t.Errorf("%s: want load error, got nil", name)
		} else {
			t.Logf("%s: %v", name, err)
		}
	}
}

func TestGrimoireFingerprintSensitivity(t *testing.T) {
	base, err := loadGrimoireTables(t, grimTOML)
	if err != nil {
		t.Fatal(err)
	}
	for name, mutated := range map[string]string{
		"cost": strings.Replace(grimTOML, "gold = 50", "gold = 51", 1),
		"time": strings.Replace(grimTOML, "research-seconds = 15.0", "research-seconds = 20.0", 1),
		"slot": strings.Replace(grimTOML, `slot = "doctrine"`, `slot = "other"`, 1),
	} {
		changed, err := loadGrimoireTables(t, mutated)
		if err != nil {
			t.Fatalf("%s: %v", name, err)
		}
		if base.Fingerprint == changed.Fingerprint {
			t.Errorf("%s change did not move the fingerprint", name)
		}
	}
}

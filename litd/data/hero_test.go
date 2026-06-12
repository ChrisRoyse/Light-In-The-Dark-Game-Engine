package data

// #304 hero-table tests. SoT = converted HeroTables rows and the
// named refusal errors.

import (
	"strings"
	"testing"
	"testing/fstest"
)

const heroUnits = techUnits + `
[[unit]]
id = "paladin"
life = 100
armor = 0
armor-type = "none"
move-speed = 60
turn-rate = 3
collision-size = 16
pathing = "ground"
acquisition-range = 0
model = "units/paladin.glb"
food-cost = 5

[[unit]]
id = "altar"
life = 900
armor = 5
armor-type = "none"
move-speed = 0
turn-rate = 0
collision-size = 48
pathing = "ground"
acquisition-range = 0
model = "units/altar.glb"
revives-heroes = true
`

const heroAbility = `
[[ability]]
id = "holy-light"
name = "Holy Light"
`

const heroTOML = `
xp-curve = [0, 200, 500, 900]

[xp]
share-radius = 600.0
split = "equal"
death-penalty-permille = 100
start-skill-points = 1

[[bounty]]
unit = "worker"
xp = 25

[[hero]]
unit = "paladin"
str = 22.0
agi = 13.0
int = 17.0
str-per-level = 2.5
agi-per-level = 1.5
int-per-level = 1.75

[[hero.skill]]
ability = "holy-light"
min-hero-level = [1, 3]

[attributes]
str-hp = 25.0
str-regen = 0.05
agi-armor = 0.3
agi-cooldown-permille = 990
int-mana = 15.0
int-mana-regen = 0.04

[revive]
base-seconds = 10.0
seconds-per-level = 5.0
[revive.costs-base]
gold = 100
[revive.costs-per-level]
gold = 50
`

func loadHeroTables(t *testing.T, units, abilities, hero string) (*Tables, error) {
	t.Helper()
	fs := econFS(econBase, units)
	fs["tech/upgrades.toml"] = &fstest.MapFile{Data: []byte(techTOML)}
	if abilities != "" {
		fs["abilities/core.toml"] = &fstest.MapFile{Data: []byte(abilities)}
	}
	if hero != "" {
		fs["heroes/heroes.toml"] = &fstest.MapFile{Data: []byte(hero)}
	}
	return Load(fs)
}

func TestHeroTablesHappyPath(t *testing.T) {
	tb, err := loadHeroTables(t, heroUnits, heroAbility, heroTOML)
	if err != nil {
		t.Fatal(err)
	}
	h := tb.Hero
	if h == nil {
		t.Fatal("hero tables nil")
	}
	if len(h.Curve) != 4 || h.Curve[3] != 900 || h.Split != SplitEqual || h.DeathPenalty != 100 || h.StartSkillPts != 1 {
		t.Fatalf("rules: %+v", h)
	}
	wi := tb.unitIndex("worker")
	if h.Bounty[wi] != 25 || h.Bounty[tb.unitIndex("hall")] != 0 {
		t.Fatalf("bounty: %v", h.Bounty)
	}
	if len(h.Heroes) != 1 {
		t.Fatalf("heroes: %d", len(h.Heroes))
	}
	hd := &h.Heroes[0]
	if int(hd.Unit) != tb.unitIndex("paladin") {
		t.Fatalf("hero unit: %d", hd.Unit)
	}
	// 22.0 → fixed 22<<32; growth 2.5 → 2.5×2^32
	if hd.Str != 22<<32 || hd.StrG != 5<<31 {
		t.Fatalf("str/strG raw: %d %d", hd.Str, hd.StrG)
	}
	if len(hd.Skills) != 1 || int(hd.Skills[0].Ability) != tb.abilityIndex("holy-light") ||
		len(hd.Skills[0].MinHeroLevel) != 2 || hd.Skills[0].MinHeroLevel[1] != 3 {
		t.Fatalf("skills: %+v", hd.Skills)
	}
	if h.Attr.StrHP != 25<<32 || h.Attr.AgiCooldownPermille != 990 {
		t.Fatalf("attr: %+v", h.Attr)
	}
	// str-regen 0.05/s at 20 t/s = 0.0025/tick
	if int64(h.Attr.StrRegen) != (25<<32)/10000 {
		t.Fatalf("str-regen raw = %d, want %d (0.0025 fixed)", int64(h.Attr.StrRegen), (int64(25)<<32)/10000)
	}
	if h.Revive.BaseTicks != 200 || h.Revive.TicksPerLevel != 100 ||
		h.Revive.CostsBase[0] != 100 || h.Revive.CostsPerLevel[0] != 50 {
		t.Fatalf("revive: %+v", h.Revive)
	}
	var altar *Unit
	for i := range tb.Units {
		if tb.Units[i].ID == "altar" {
			altar = &tb.Units[i]
		}
	}
	if !altar.RevivesHeroes {
		t.Fatal("altar revives-heroes flag lost")
	}
	t.Logf("curve=%v bounty[worker]=%d", h.Curve, h.Bounty[wi])
	t.Logf("paladin: str=%d agi=%d int=%d (raw fixed) skills=%+v", hd.Str, hd.Agi, hd.Int, hd.Skills)
	t.Logf("revive: %+v  attr: %+v", h.Revive, h.Attr)
}

func TestHeroTablesFailClosed(t *testing.T) {
	cases := []struct {
		name, hero, wantErr string
	}{
		{"curve not increasing", strings.Replace(heroTOML, "xp-curve = [0, 200, 500, 900]", "xp-curve = [0, 200, 200, 900]", 1), "not strictly increasing"},
		{"curve not zero-based", strings.Replace(heroTOML, "xp-curve = [0, 200, 500, 900]", "xp-curve = [10, 200, 500, 900]", 1), "level 1 must cost 0"},
		{"unknown split", strings.Replace(heroTOML, `split = "equal"`, `split = "weighted"`, 1), "is not equal|full"},
		{"unknown bounty unit", strings.Replace(heroTOML, `unit = "worker"`, `unit = "ghost"`, 1), `unit "ghost" is not defined`},
		{"unknown hero unit", strings.Replace(heroTOML, `unit = "paladin"`, `unit = "ghost"`, 1), `unit "ghost" is not defined`},
		{"unknown skill ability", strings.Replace(heroTOML, `ability = "holy-light"`, `ability = "storm-bolt"`, 1), "is not a defined ability"},
		{"tier levels decreasing", strings.Replace(heroTOML, "min-hero-level = [1, 3]", "min-hero-level = [3, 1]", 1), "non-decreasing"},
		{"penalty out of range", strings.Replace(heroTOML, "death-penalty-permille = 100", "death-penalty-permille = 1001", 1), "out of range [0, 1000]"},
		{"hero without revive table", heroTOML[:strings.Index(heroTOML, "[revive]")], "hero rows without a [revive] table"},
		{"unknown revive resource", strings.Replace(heroTOML, "[revive.costs-base]\ngold = 100", "[revive.costs-base]\noil = 100", 1), `resource "oil" is not in resource-types`},
	}
	for _, c := range cases {
		_, err := loadHeroTables(t, heroUnits, heroAbility, c.hero)
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

func TestHeroFingerprintSensitivity(t *testing.T) {
	base, err := loadHeroTables(t, heroUnits, heroAbility, heroTOML)
	if err != nil {
		t.Fatal(err)
	}
	for name, mutated := range map[string]string{
		"curve":  strings.Replace(heroTOML, "xp-curve = [0, 200, 500, 900]", "xp-curve = [0, 200, 500, 901]", 1),
		"growth": strings.Replace(heroTOML, "str-per-level = 2.5", "str-per-level = 2.6", 1),
		"coeff":  strings.Replace(heroTOML, "str-hp = 25.0", "str-hp = 26.0", 1),
		"bounty": strings.Replace(heroTOML, "xp = 25", "xp = 26", 1),
	} {
		changed, err := loadHeroTables(t, heroUnits, heroAbility, mutated)
		if err != nil {
			t.Fatalf("%s: %v", name, err)
		}
		if base.Fingerprint == changed.Fingerprint {
			t.Errorf("%s change did not move the fingerprint", name)
		}
		t.Logf("%s: base=%016x changed=%016x", name, base.Fingerprint, changed.Fingerprint)
	}
}

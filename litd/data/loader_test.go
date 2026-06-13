package data

import (
	"os"
	"reflect"
	"strings"
	"testing"
	"testing/fstest"
)

const testDamageTable = `
attack-types = ["normal", "piercing"]
armor-types = ["light", "heavy"]
[coefficients]
normal = [1000, 700]
piercing = [2000, 350]
`

const testAbilities = `
[[ability]]
id = "defend"
name = "Defend"
`

func mapFS(units string) fstest.MapFS {
	return fstest.MapFS{
		"combat/damage-table.toml": &fstest.MapFile{Data: []byte(testDamageTable)},
		"abilities/core.toml":      &fstest.MapFile{Data: []byte(testAbilities)},
		"units/test.toml":          &fstest.MapFile{Data: []byte(units)},
	}
}

const testUnitTOML = `
[[unit]]
id = "grunt"
life = 420
regen = 0.25
armor = 2
armor-type = "heavy"
move-speed = 270
turn-rate = 0.6
collision-size = 16
pathing = "ground"
acquisition-range = 600
model = "units/grunt.glb"
abilities = ["defend"]

[[unit.attack]]
type = "normal"
range = 90
damage-base = 11
dice = 1
sides = 2
cooldown = 1.35
damage-point = 0.5
backswing = 0.5
delivery = "instant"
targets-allowed = ["ground", "structure"]
`

// Happy path: the shipped starter tables load, and every converted
// value matches the hand-computed expectation (the §6 worked example:
// cooldown 1.35 s → 27 ticks, damagePoint 0.5 s → 10 ticks, …).
func TestLoaderStarterTables(t *testing.T) {
	tables, err := Load(os.DirFS("../../data"))
	if err != nil {
		t.Fatalf("starter tables must load: %v", err)
	}
	t.Logf("fingerprint=%016x  attackTypes=%v armorTypes=%v",
		tables.Fingerprint, tables.AttackTypes, tables.ArmorTypes)
	for i := range tables.Units {
		t.Logf("%s", tables.Units[i].String())
	}

	var foot *Unit
	for i := range tables.Units {
		if tables.Units[i].ID == "footman" {
			foot = &tables.Units[i]
		}
	}
	if foot == nil {
		t.Fatal("footman row missing")
	}
	// hand-computed (X+X=Y discipline; One = 1<<32, 20 ticks/s):
	checks := []struct {
		name      string
		got, want int64
	}{
		{"life", int64(foot.Life), 420},
		{"regen/tick = round(0.25·2^32/20)", int64(foot.RegenPerTick), 53687091},
		{"armorType heavy → index", int64(foot.ArmorType), 3},
		{"speed/tick = 13.5·2^32", int64(foot.MoveSpeedPerTick), 57982058496},
		{"turn/tick = round(0.6·65536/20)", int64(uint16(foot.TurnRatePerTick)), 1966},
		{"collisionClass (16 → 1 ring)", int64(foot.CollisionClass), 1},
		{"acqRange = 600·2^32", int64(foot.AcquisitionRange), 2576980377600},
		{"sightDay = 1800·2^32", int64(foot.SightDay), 7730941132800},
		{"sightNight = 800·2^32", int64(foot.SightNight), 3435973836800},
		{"cooldown 1.35s → ticks", int64(foot.Attacks[0].CooldownTicks), 27},
		{"damagePoint 0.5s → ticks", int64(foot.Attacks[0].DamagePointTicks), 10},
		{"backswing 0.5s → ticks", int64(foot.Attacks[0].BackswingTicks), 10},
		{"targets ground|structure", int64(foot.Attacks[0].TargetsAllowed), 0b101},
		{"attackType normal → index", int64(foot.Attacks[0].AttackType), 0},
	}
	for _, c := range checks {
		t.Logf("%-36s got=%d want=%d", c.name, c.got, c.want)
		if c.got != c.want {
			t.Fatalf("%s: got %d want %d", c.name, c.got, c.want)
		}
	}

	var arch *Unit
	for i := range tables.Units {
		if tables.Units[i].ID == "archer" {
			arch = &tables.Units[i]
		}
	}
	if arch == nil {
		t.Fatal("archer row missing")
	}
	a := arch.Attacks[0]
	if a.Delivery != DeliveryProjectile || int64(a.ProjectileSpeedPerTick) != 45*(1<<32) {
		t.Fatalf("archer projectile: delivery=%d speed=%d want %d", a.Delivery,
			int64(a.ProjectileSpeedPerTick), int64(45*(1<<32)))
	}
	if a.CooldownTicks != 30 || a.DamagePointTicks != 6 {
		t.Fatalf("archer ticks: cd=%d dp=%d want 30/6", a.CooldownTicks, a.DamagePointTicks)
	}
	t.Logf("archer projectile speed/tick = %d (= 45·2^32, 900 u/s ÷ 20) ✓cd=30 dp=6", int64(a.ProjectileSpeedPerTick))

	// load again: fingerprint stable
	tables2, err := Load(os.DirFS("../../data"))
	if err != nil {
		t.Fatal(err)
	}
	t.Logf("second load fingerprint=%016x (must equal first)", tables2.Fingerprint)
	if tables.Fingerprint != tables2.Fingerprint {
		t.Fatalf("fingerprint not stable: %016x vs %016x", tables.Fingerprint, tables2.Fingerprint)
	}
}

// Canonicalization: reordering TOML keys must not change the
// fingerprint; changing one value must.
func TestLoaderFingerprintCanonical(t *testing.T) {
	base, err := Load(mapFS(testUnitTOML))
	if err != nil {
		t.Fatal(err)
	}
	// same content, keys scrambled
	reordered := `
[[unit]]
model = "units/grunt.glb"
acquisition-range = 600
id = "grunt"
armor-type = "heavy"
pathing = "ground"
life = 420
collision-size = 16
turn-rate = 0.6
move-speed = 270
armor = 2
regen = 0.25
abilities = ["defend"]

[[unit.attack]]
targets-allowed = ["ground", "structure"]
delivery = "instant"
backswing = 0.5
damage-point = 0.5
cooldown = 1.35
sides = 2
dice = 1
damage-base = 11
range = 90
type = "normal"
`
	reord, err := Load(mapFS(reordered))
	if err != nil {
		t.Fatal(err)
	}
	changed, err := Load(mapFS(strings.Replace(testUnitTOML, "life = 420", "life = 421", 1)))
	if err != nil {
		t.Fatal(err)
	}
	t.Logf("base      = %016x", base.Fingerprint)
	t.Logf("reordered = %016x (same keys, scrambled order — must equal base)", reord.Fingerprint)
	t.Logf("changed   = %016x (life 420→421 — must differ)", changed.Fingerprint)
	if base.Fingerprint != reord.Fingerprint {
		t.Fatalf("key order leaked into the fingerprint")
	}
	if base.Fingerprint == changed.Fingerprint {
		t.Fatalf("value change did not change the fingerprint")
	}
}

// Format equivalence: the SAME table as TOML and as generated JSON
// must produce identical in-memory rows and fingerprints.
func TestLoaderJSONEquivalence(t *testing.T) {
	const unitJSON = `{
  "unit": [{
    "id": "grunt",
    "life": 420,
    "regen": 0.25,
    "armor": 2,
    "armor-type": "heavy",
    "move-speed": 270,
    "turn-rate": 0.6,
    "collision-size": 16,
    "pathing": "ground",
    "acquisition-range": 600,
    "model": "units/grunt.glb",
    "abilities": ["defend"],
    "attack": [{
      "type": "normal",
      "range": 90,
      "damage-base": 11,
      "dice": 1,
      "sides": 2,
      "cooldown": 1.35,
      "damage-point": 0.5,
      "backswing": 0.5,
      "delivery": "instant",
      "targets-allowed": ["ground", "structure"]
    }]
  }]
}`
	fromTOML, err := Load(mapFS(testUnitTOML))
	if err != nil {
		t.Fatal(err)
	}
	jsonFS := fstest.MapFS{
		"combat/damage-table.toml": &fstest.MapFile{Data: []byte(testDamageTable)},
		"abilities/core.toml":      &fstest.MapFile{Data: []byte(testAbilities)},
		"units/test.json":          &fstest.MapFile{Data: []byte(unitJSON)},
	}
	fromJSON, err := Load(jsonFS)
	if err != nil {
		t.Fatal(err)
	}
	t.Logf("TOML row: %s", fromTOML.Units[0].String())
	t.Logf("JSON row: %s", fromJSON.Units[0].String())
	t.Logf("fingerprints: toml=%016x json=%016x", fromTOML.Fingerprint, fromJSON.Fingerprint)
	if !reflect.DeepEqual(fromTOML.Units, fromJSON.Units) {
		t.Fatalf("TOML and JSON rows differ")
	}
	if fromTOML.Fingerprint != fromJSON.Fingerprint {
		t.Fatalf("format leaked into the fingerprint")
	}
}

// Edge 1: an unknown field is a load error naming field and file —
// never a silent skip.
func TestLoaderUnknownField(t *testing.T) {
	bad := strings.Replace(testUnitTOML, "armor = 2", "armour = 2", 1)
	_, err := Load(mapFS(bad))
	if err == nil {
		t.Fatal("unknown field must fail the load")
	}
	t.Logf("loader error: %v", err)
	if !strings.Contains(err.Error(), "armour") || !strings.Contains(err.Error(), "units/test.toml") {
		t.Fatalf("error must name the field and the file: %v", err)
	}
}

// Edge 2: sub-tick durations quantize UP to one tick (never 0 —
// authored timings are never faster than written).
func TestLoaderSubTickQuantize(t *testing.T) {
	cases := []struct {
		s    float64
		want uint16
	}{
		{0.049, 1}, {0.001, 1}, {0.05, 1}, {0.051, 2}, {1.35, 27}, {0.5, 10}, {0, 0},
	}
	for _, c := range cases {
		got, err := SecondsToTicks(c.s)
		if err != nil {
			t.Fatal(err)
		}
		t.Logf("SecondsToTicks(%v) = %d (want %d)", c.s, got, c.want)
		if got != c.want {
			t.Fatalf("SecondsToTicks(%v) = %d, want %d", c.s, got, c.want)
		}
	}
	bad := strings.Replace(testUnitTOML, "cooldown = 1.35", "cooldown = 0.049", 1)
	tables, err := Load(mapFS(bad))
	if err != nil {
		t.Fatal(err)
	}
	if tables.Units[0].Attacks[0].CooldownTicks != 1 {
		t.Fatalf("0.049 s cooldown loaded as %d ticks, want 1", tables.Units[0].Attacks[0].CooldownTicks)
	}
	t.Logf("full load: cooldown 0.049 s → %d tick", tables.Units[0].Attacks[0].CooldownTicks)
}

// Edge 3: a reference to an undefined ability is a load error.
func TestLoaderDanglingAbilityRef(t *testing.T) {
	bad := strings.Replace(testUnitTOML, `abilities = ["defend"]`, `abilities = ["warstomp"]`, 1)
	_, err := Load(mapFS(bad))
	if err == nil {
		t.Fatal("dangling ability ref must fail the load")
	}
	t.Logf("loader error: %v", err)
	if !strings.Contains(err.Error(), "warstomp") {
		t.Fatalf("error must name the missing ability: %v", err)
	}
}

// Edge 4: out-of-range and malformed rows fail closed.
func TestLoaderRejections(t *testing.T) {
	cases := []struct {
		name, find, replace, wantSub string
	}{
		{"negative life", "life = 420", "life = -5", "life"},
		{"unknown armor type", `armor-type = "heavy"`, `armor-type = "adamantine"`, "adamantine"},
		{"unknown attack type", `type = "normal"`, `type = "holy"`, "holy"},
		{"bad delivery", `delivery = "instant"`, `delivery = "ballistic"`, "ballistic"},
		{"bad target class", `targets-allowed = ["ground", "structure"]`, `targets-allowed = ["naval"]`, "naval"},
		{"armor out of range", "armor = 2", "armor = 400", "armor"},
	}
	for _, c := range cases {
		_, err := Load(mapFS(strings.Replace(testUnitTOML, c.find, c.replace, 1)))
		if err == nil {
			t.Fatalf("%s: must fail the load", c.name)
		}
		t.Logf("%-22s → %v", c.name, err)
		if !strings.Contains(err.Error(), c.wantSub) {
			t.Fatalf("%s: error must mention %q: %v", c.name, c.wantSub, err)
		}
	}
	// damage-table shape errors
	badDT := strings.Replace(testDamageTable, "normal = [1000, 700]", "normal = [1000]", 1)
	_, err := Load(fstest.MapFS{
		"combat/damage-table.toml": &fstest.MapFile{Data: []byte(badDT)},
		"abilities/core.toml":      &fstest.MapFile{Data: []byte(testAbilities)},
		"units/test.toml":          &fstest.MapFile{Data: []byte(testUnitTOML)},
	})
	if err == nil {
		t.Fatal("short coefficient row must fail")
	}
	t.Logf("short matrix row → %v", err)
}

// TestLoaderPointValue: the optional point-value field decodes onto Unit.PointValue.
// SoT = the parsed Unit struct field (the byte the decoder lands), with the
// X+X=Y discipline (point-value 30 in TOML -> PointValue==30), an omitted-field
// default of 0, and a fail-closed rejection of a negative value.
func TestLoaderPointValue(t *testing.T) {
	const withPV = testUnitTOML + `
[[unit]]
id = "scout"
life = 100
regen = 0
armor = 0
armor-type = "light"
move-speed = 200
turn-rate = 0.6
collision-size = 8
pathing = "ground"
acquisition-range = 500
point-value = 30
`
	tables, err := Load(mapFS(withPV))
	if err != nil {
		t.Fatalf("load: %v", err)
	}
	var scout, grunt *Unit
	for i := range tables.Units {
		switch tables.Units[i].ID {
		case "scout":
			scout = &tables.Units[i]
		case "grunt":
			grunt = &tables.Units[i]
		}
	}
	if scout == nil || grunt == nil {
		t.Fatal("scout/grunt rows missing")
	}
	t.Logf("scout.PointValue=%d (want 30)  grunt.PointValue=%d (want 0, omitted)", scout.PointValue, grunt.PointValue)
	if scout.PointValue != 30 {
		t.Errorf("scout.PointValue=%d, want 30", scout.PointValue)
	}
	if grunt.PointValue != 0 { // testUnitTOML omits point-value -> default 0
		t.Errorf("grunt.PointValue=%d, want 0 (omitted default)", grunt.PointValue)
	}

	// EDGE: negative point-value must be rejected (fail-closed), not clamped.
	const negPV = testUnitTOML + `
[[unit]]
id = "bad"
life = 100
regen = 0
armor = 0
armor-type = "light"
move-speed = 200
turn-rate = 0.6
collision-size = 8
pathing = "ground"
acquisition-range = 500
point-value = -1
`
	if _, err := Load(mapFS(negPV)); err == nil {
		t.Fatal("negative point-value must fail to load")
	} else {
		t.Logf("negative point-value rejected -> %v", err)
	}
}

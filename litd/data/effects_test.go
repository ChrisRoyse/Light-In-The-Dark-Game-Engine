package data

import (
	"strconv"
	"strings"
	"testing"
	"testing/fstest"
)

// effFS builds a data layout whose abilities file is the given TOML.
func effFS(abilities string) fstest.MapFS {
	return fstest.MapFS{
		"combat/damage-table.toml": &fstest.MapFile{Data: []byte(testDamageTable)},
		"abilities/core.toml":      &fstest.MapFile{Data: []byte(abilities)},
		"units/test.toml":          &fstest.MapFile{Data: []byte(strings.ReplaceAll(testUnitTOML, `abilities = ["defend"]`, `abilities = []`))},
	}
}

const fireboltTOML = `
[[ability]]
id = "firebolt"
name = "Firebolt"

[[ability.effects]]
prim = "damage"
amount = 50
attack-type = "piercing"

[[ability.effects]]
prim = "area"
radius = 300.0
max-targets = 6

[[ability.effects.effects]]
prim = "damage"
amount = 25
attack-type = "normal"

[[ability.effects.effects]]
prim = "heal"
amount = 10
`

// Happy path: the composition compiles to the exact flat arena —
// level slots contiguous, children after, params in schema order,
// fixed-point conversion applied.
func TestEffectCompileHappy(t *testing.T) {
	tb, err := Load(effFS(fireboltTOML))
	if err != nil {
		t.Fatalf("load: %v", err)
	}
	var ab *Ability
	for i := range tb.Abilities {
		if tb.Abilities[i].ID == "firebolt" {
			ab = &tb.Abilities[i]
		}
	}
	if ab == nil {
		t.Fatal("firebolt row missing")
	}
	if ab.Effects != (EffectList{Off: 0, Len: 2}) {
		t.Fatalf("firebolt list = %+v, want {0 2}", ab.Effects)
	}
	if len(tb.Effects) != 4 {
		t.Fatalf("arena len = %d, want 4", len(tb.Effects))
	}
	for i, e := range tb.Effects {
		t.Logf("arena[%d] = prim=%d(%s) params=%v child=[%d,+%d)",
			i, e.Prim, EffectSchemas[e.Prim].Name, e.Params, e.ChildOff, e.ChildLen)
	}
	want := []CompiledEffect{
		{Prim: EPDamage, Params: [MaxEffectParams]int64{50, 0, 0, 1}},                          // attack-type "piercing" = row 1
		{Prim: EPArea, Params: [MaxEffectParams]int64{300 << 32, 6}, ChildOff: 2, ChildLen: 2}, // radius 300 wu → fixed bits
		{Prim: EPDamage, Params: [MaxEffectParams]int64{25, 0, 0, 0}},                          // "normal" = row 0
		{Prim: EPHeal, Params: [MaxEffectParams]int64{10}},
	}
	for i := range want {
		if tb.Effects[i] != want[i] {
			t.Errorf("arena[%d] = %+v, want %+v", i, tb.Effects[i], want[i])
		}
	}
}

// Defaults fill for absent optional params (chain falloff-permille
// 1000, fork is exercised by the budget tests).
func TestEffectOptionalDefaults(t *testing.T) {
	tb, err := Load(effFS(`
[[ability]]
id = "zap"
name = "Zap"

[[ability.effects]]
prim = "chain"
hops = 3
range = 500.0

[[ability.effects.effects]]
prim = "damage"
amount = 40
attack-type = "normal"
`))
	if err != nil {
		t.Fatalf("load: %v", err)
	}
	e := tb.Effects[0]
	if e.Prim != EPChain || e.Params[0] != 3 || e.Params[1] != 1000 || e.Params[2] != 500<<32 {
		t.Fatalf("chain compiled %+v, want hops=3 falloff=1000(default) range=%d", e, int64(500)<<32)
	}
}

// Abilities without an effects key stay zero-length — absence is a
// visible empty list, not an error and not a fabricated default.
func TestEffectAbsentIsEmpty(t *testing.T) {
	tb, err := Load(effFS(testAbilities))
	if err != nil {
		t.Fatalf("load: %v", err)
	}
	if tb.Abilities[0].Effects != (EffectList{}) || len(tb.Effects) != 0 {
		t.Fatalf("stub ability got effects %+v, arena %d", tb.Abilities[0].Effects, len(tb.Effects))
	}
}

// Invocation budget boundary: worst case exactly 256 loads; 257+ is
// a load error naming the count and ceiling.
func TestEffectInvocationBudget(t *testing.T) {
	mk := func(outer int) string {
		return `
[[ability]]
id = "storm"
name = "Storm"

[[ability.effects]]
prim = "fork"
count = ` + strconv.Itoa(outer) + `

[[ability.effects.effects]]
prim = "fork"
count = 16

[[ability.effects.effects.effects]]
prim = "damage"
amount = 1
attack-type = "normal"
`
	}
	// 1 + 15*(1 + 16*1) = 256 — at the ceiling, loads.
	if _, err := Load(effFS(mk(15))); err != nil {
		t.Fatalf("budget=256 must load: %v", err)
	}
	// 1 + 16*(1 + 16*1) = 273 — over, rejected.
	_, err := Load(effFS(mk(16)))
	if err == nil || !strings.Contains(err.Error(), "invocation count 273 exceeds ceiling 256") {
		t.Fatalf("budget=273 err = %v", err)
	}
}

// Every malformed composition is a LOAD error with a precise message.
func TestEffectRejections(t *testing.T) {
	cases := []struct {
		name, toml, wantErr string
	}{
		{"unknown primitive", `
[[ability]]
id = "x"
name = "X"
[[ability.effects]]
prim = "frobnicate"
`, `"frobnicate" is not a registered effect primitive`},
		{"missing prim key", `
[[ability]]
id = "x"
name = "X"
[[ability.effects]]
amount = 5
`, `missing "prim"`},
		{"missing required param", `
[[ability]]
id = "x"
name = "X"
[[ability.effects]]
prim = "damage"
attack-type = "normal"
`, `missing required param "amount"`},
		{"unknown param", `
[[ability]]
id = "x"
name = "X"
[[ability.effects]]
prim = "heal"
amount = 5
radius = 99
`, `unknown param "radius"`},
		{"out of range", `
[[ability]]
id = "x"
name = "X"
[[ability.effects]]
prim = "damage"
amount = 2000000
attack-type = "normal"
`, `out of range`},
		{"unknown attack type", `
[[ability]]
id = "x"
name = "X"
[[ability.effects]]
prim = "damage"
amount = 5
attack-type = "chaos"
`, `"chaos" is not a damage-matrix attack type`},
		{"combinator without effects", `
[[ability]]
id = "x"
name = "X"
[[ability.effects]]
prim = "fork"
count = 2
`, `combinator "fork" requires an "effects" list`},
		{"empty effects list", `
[[ability]]
id = "x"
name = "X"
effects = []
`, `effects list must be non-empty`},
		{"depth five", `
[[ability]]
id = "x"
name = "X"
[[ability.effects]]
prim = "fork"
count = 2
[[ability.effects.effects]]
prim = "fork"
count = 2
[[ability.effects.effects.effects]]
prim = "fork"
count = 2
[[ability.effects.effects.effects.effects]]
prim = "fork"
count = 2
[[ability.effects.effects.effects.effects.effects]]
prim = "heal"
amount = 1
`, `exceeds max depth 4`},
		{"wrong param type", `
[[ability]]
id = "x"
name = "X"
[[ability.effects]]
prim = "heal"
amount = "lots"
`, `must be an integer`},
	}
	for _, c := range cases {
		t.Run(c.name, func(t *testing.T) {
			_, err := Load(effFS(c.toml))
			if err == nil {
				t.Fatalf("%s: loaded, want error containing %q", c.name, c.wantErr)
			}
			t.Logf("rejection: %v", err)
			if !strings.Contains(err.Error(), c.wantErr) {
				t.Fatalf("err = %v, want substring %q", err, c.wantErr)
			}
		})
	}
}

// Duplicate ability IDs across files are rejected naming both files.
func TestEffectDuplicateAbilityID(t *testing.T) {
	fsys := effFS(testAbilities)
	fsys["abilities/more.toml"] = &fstest.MapFile{Data: []byte(testAbilities)}
	_, err := Load(fsys)
	if err == nil || !strings.Contains(err.Error(), `duplicate ability id "defend"`) {
		t.Fatalf("err = %v, want duplicate ability id", err)
	}
}

// The arena folds into the fingerprint: same composition = same
// fingerprint, any param change = different fingerprint.
func TestEffectFingerprintFold(t *testing.T) {
	a1, err := Load(effFS(fireboltTOML))
	if err != nil {
		t.Fatal(err)
	}
	a2, err := Load(effFS(fireboltTOML))
	if err != nil {
		t.Fatal(err)
	}
	if a1.Fingerprint != a2.Fingerprint {
		t.Fatalf("identical content, fingerprints differ: %016x vs %016x", a1.Fingerprint, a2.Fingerprint)
	}
	b, err := Load(effFS(strings.Replace(fireboltTOML, "amount = 50", "amount = 51", 1)))
	if err != nil {
		t.Fatal(err)
	}
	if b.Fingerprint == a1.Fingerprint {
		t.Fatalf("param changed but fingerprint held at %016x", b.Fingerprint)
	}
	t.Logf("fingerprint base=%016x amount51=%016x", a1.Fingerprint, b.Fingerprint)
}

// JSON and TOML forms of the same composition compile identically
// (the []any vs []map[string]any decoder split in asEffectSlice).
func TestEffectJSONEquivalence(t *testing.T) {
	jsonAbilities := `{"ability": [{"id": "firebolt", "name": "Firebolt", "effects": [
		{"prim": "damage", "amount": 50, "attack-type": "piercing"},
		{"prim": "area", "radius": 300.0, "max-targets": 6, "effects": [
			{"prim": "damage", "amount": 25, "attack-type": "normal"},
			{"prim": "heal", "amount": 10}
		]}
	]}]}`
	fsys := effFS("")
	delete(fsys, "abilities/core.toml")
	fsys["abilities/core.json"] = &fstest.MapFile{Data: []byte(jsonAbilities)}
	jt, err := Load(fsys)
	if err != nil {
		t.Fatalf("json load: %v", err)
	}
	tt, err := Load(effFS(fireboltTOML))
	if err != nil {
		t.Fatalf("toml load: %v", err)
	}
	if jt.Fingerprint != tt.Fingerprint {
		t.Fatalf("json fp %016x != toml fp %016x", jt.Fingerprint, tt.Fingerprint)
	}
	for i := range tt.Effects {
		if jt.Effects[i] != tt.Effects[i] {
			t.Errorf("arena[%d]: json %+v != toml %+v", i, jt.Effects[i], tt.Effects[i])
		}
	}
}

// Schema registry invariants: names nonempty and unique; combinator
// FanOut names an existing int param; param counts fit the block.
func TestEffectSchemaInvariants(t *testing.T) {
	seen := map[string]bool{}
	for id := EffectPrimID(0); id < EffectPrimCount; id++ {
		s := &EffectSchemas[id]
		if s.Name == "" {
			t.Fatalf("prim %d has empty name", id)
		}
		if seen[s.Name] {
			t.Fatalf("prim %d name %q duplicated", id, s.Name)
		}
		seen[s.Name] = true
		if len(s.Params) > MaxEffectParams {
			t.Fatalf("%s declares %d params, block holds %d", s.Name, len(s.Params), MaxEffectParams)
		}
		if s.FanOut != "" {
			if !s.Combinator {
				t.Fatalf("%s has FanOut but is not a combinator", s.Name)
			}
			found := false
			for _, p := range s.Params {
				if p.Name == s.FanOut {
					found = true
					if p.Kind != EPKInt {
						t.Fatalf("%s fan-out param %q is not an int", s.Name, p.Name)
					}
				}
			}
			if !found {
				t.Fatalf("%s FanOut %q names no param", s.Name, s.FanOut)
			}
		}
	}
}

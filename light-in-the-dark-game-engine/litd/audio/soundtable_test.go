package audio

// FSV for the #428 sound classification table. SoT = the parsed SoundTable
// (domain/priority/ogg per cue) read back from synthetic TOML with known inputs
// and known expected outputs. Happy path + the fail-closed edges: every malformed
// or unclassified entry must be rejected with a precise message, never defaulted.

import (
	"strings"
	"testing"
	"testing/fstest"
)

func loadSynth(t *testing.T, body string) (*SoundTable, error) {
	t.Helper()
	fsys := fstest.MapFS{"audio/sounds.toml": {Data: []byte(body)}}
	return LoadSoundTable(fsys, "audio/sounds.toml")
}

func TestSoundTableHappyPathFSV(t *testing.T) {
	const body = `
[[sound]]
cue = "footman_attack"
domain = "world"
priority = "attackimpact"
ogg = "sfx/footman_attack.ogg"

[[sound]]
cue = "under_attack_stinger"
domain = "ui"
priority = "alert"
ogg = "ui/under_attack.ogg"
`
	tbl, err := loadSynth(t, body)
	if err != nil {
		t.Fatalf("classified table rejected: %v", err)
	}
	// SoT read-back: exact expected classification per cue.
	if tbl.Len() != 2 {
		t.Fatalf("Len()=%d, want 2", tbl.Len())
	}
	e, ok := tbl.Lookup("footman_attack")
	if !ok || e.Domain != DomainWorld || e.Priority != PrioAttackImpact || e.Ogg != "sfx/footman_attack.ogg" {
		t.Fatalf("footman_attack = %+v ok=%v; want {world, attackimpact, sfx/footman_attack.ogg}", e, ok)
	}
	// The crux of #428: a UI sound is classified UI by its TABLE entry — independent
	// of any channel or position it is later played on.
	e, ok = tbl.Lookup("under_attack_stinger")
	if !ok || e.Domain != DomainUI || e.Priority != PrioAlert {
		t.Fatalf("under_attack_stinger = %+v ok=%v; want {ui, alert}", e, ok)
	}
	if _, ok := tbl.Lookup("nonexistent"); ok {
		t.Fatal("Lookup of an absent cue returned ok=true")
	}
	t.Logf("FSV #428: 2 cues classified — footman_attack{world,attackimpact}, under_attack_stinger{ui,alert}")
}

func TestSoundTableEmptyIsValidFSV(t *testing.T) {
	tbl, err := loadSynth(t, "# no sounds shipped yet\n")
	if err != nil {
		t.Fatalf("empty table rejected: %v", err)
	}
	if tbl.Len() != 0 {
		t.Fatalf("Len()=%d, want 0", tbl.Len())
	}
}

func TestSoundTableFailClosedEdges(t *testing.T) {
	cases := []struct {
		name, body, wantSubstr string
	}{
		{
			"missing-domain",
			"[[sound]]\ncue=\"x\"\npriority=\"death\"\nogg=\"a.ogg\"\n",
			`sound "x" has missing/invalid domain "" (want world|ui)`,
		},
		{
			"invalid-domain",
			"[[sound]]\ncue=\"x\"\ndomain=\"hud\"\npriority=\"death\"\nogg=\"a.ogg\"\n",
			`missing/invalid domain "hud"`,
		},
		{
			"missing-priority",
			"[[sound]]\ncue=\"x\"\ndomain=\"world\"\nogg=\"a.ogg\"\n",
			`missing/invalid priority "" (want ambient|attackimpact|death|abilitycast|alert)`,
		},
		{
			"invalid-priority",
			"[[sound]]\ncue=\"x\"\ndomain=\"world\"\npriority=\"loud\"\nogg=\"a.ogg\"\n",
			`missing/invalid priority "loud"`,
		},
		{
			"empty-cue",
			"[[sound]]\ncue=\"\"\ndomain=\"world\"\npriority=\"death\"\nogg=\"a.ogg\"\n",
			"entry 0 has an empty cue",
		},
		{
			"missing-ogg",
			"[[sound]]\ncue=\"x\"\ndomain=\"world\"\npriority=\"death\"\n",
			`sound "x" has no ogg path`,
		},
		{
			"non-ogg",
			"[[sound]]\ncue=\"x\"\ndomain=\"world\"\npriority=\"death\"\nogg=\"a.wav\"\n",
			`ogg "a.wav" is not a .ogg`,
		},
		{
			"duplicate-cue",
			"[[sound]]\ncue=\"x\"\ndomain=\"world\"\npriority=\"death\"\nogg=\"a.ogg\"\n" +
				"[[sound]]\ncue=\"x\"\ndomain=\"ui\"\npriority=\"alert\"\nogg=\"b.ogg\"\n",
			`duplicate cue "x"`,
		},
	}
	for _, c := range cases {
		t.Run(c.name, func(t *testing.T) {
			tbl, err := loadSynth(t, c.body)
			if err == nil {
				t.Fatalf("malformed table accepted (tbl=%+v) — must fail closed", tbl)
			}
			if !strings.Contains(err.Error(), c.wantSubstr) {
				t.Fatalf("error %q does not contain %q", err.Error(), c.wantSubstr)
			}
			t.Logf("FSV #428 edge %s: rejected — %v", c.name, err)
		})
	}
}

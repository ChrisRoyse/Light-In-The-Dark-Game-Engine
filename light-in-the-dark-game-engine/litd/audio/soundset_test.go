package audio

// FSV for the #313 sound-set table loader. SoT = the in-memory SoundSetTable
// (byType map / per-category cues) after Load, and the ERROR returned for each
// malformed input. X+X=Y: a known TOML in → known parsed cues / known rejection.
// Every cue ref is cross-checked against a synthetic #428 classification table;
// the loader is fail-closed, so each edge must reject with NO partial table.

import (
	"strings"
	"testing"
	"testing/fstest"
)

// classifyFixture is a classification table (#428) covering the six cues a full
// footman sound set names, plus a couple of spares — the universe of "classified"
// cues the sound-set loader validates against.
const classifyFixture = `
[[sound]]
cue = "footman_attack"
domain = "world"
priority = "attackimpact"
ogg = "sfx/foot_atk.ogg"
[[sound]]
cue = "footman_death"
domain = "world"
priority = "death"
ogg = "sfx/foot_die.ogg"
[[sound]]
cue = "footman_ready"
domain = "ui"
priority = "ambient"
ogg = "sfx/foot_ready.ogg"
[[sound]]
cue = "footman_ack"
domain = "ui"
priority = "ambient"
ogg = "sfx/foot_ack.ogg"
[[sound]]
cue = "footman_order"
domain = "ui"
priority = "ambient"
ogg = "sfx/foot_order.ogg"
[[sound]]
cue = "footman_warn"
domain = "world"
priority = "alert"
ogg = "sfx/foot_warn.ogg"
`

func loadClassify(t *testing.T) *SoundTable {
	t.Helper()
	st, err := LoadSoundTable(fstest.MapFS{"audio/sounds.toml": {Data: []byte(classifyFixture)}}, "audio/sounds.toml")
	if err != nil {
		t.Fatalf("classification fixture failed to load: %v", err)
	}
	return st
}

// loadSet packs body as the sound-set table and loads it against the fixture.
func loadSet(t *testing.T, body string) (*SoundSetTable, error) {
	t.Helper()
	return LoadSoundSetTable(
		fstest.MapFS{"data/sounds/sets.toml": {Data: []byte(body)}},
		"data/sounds/sets.toml", loadClassify(t))
}

const fullFootmanSet = `
[[unit]]
type = "hfoo"
attack = "footman_attack"
death = "footman_death"
ready = "footman_ready"
ack = "footman_ack"
order_ack = "footman_order"
under_attack = "footman_warn"
`

// Happy path: a complete, fully-classified set loads and Lookup returns the exact
// cue authored for each category.
func TestSoundSetLoadHappyFSV(t *testing.T) {
	tbl, err := loadSet(t, fullFootmanSet)
	if err != nil {
		t.Fatalf("valid table rejected: %v", err)
	}
	if tbl.Len() != 1 {
		t.Fatalf("Len=%d, want 1", tbl.Len())
	}
	set, ok := tbl.Lookup("hfoo")
	if !ok {
		t.Fatal("hfoo not found after load")
	}
	want := map[SoundCategory]string{
		CatAttack:      "footman_attack",
		CatDeath:       "footman_death",
		CatReady:       "footman_ready",
		CatAck:         "footman_ack",
		CatOrderAck:    "footman_order",
		CatUnderAttack: "footman_warn",
	}
	for c, w := range want {
		if got := set.Cue(c); got != w {
			t.Fatalf("hfoo %s cue = %q, want %q", c, got, w)
		}
	}
	// A unit type with no row is absent (not a silent empty set).
	if _, ok := tbl.Lookup("hpea"); ok {
		t.Fatal("undefined unit type hpea must not resolve")
	}
	t.Logf("FSV #313 happy: hfoo set loaded, all 6 categories map to their authored classified cues")
}

// Edge 1 — a category with no cue (the canonical "no death sound" case from the
// issue): reject, naming the unit and the missing category.
func TestSoundSetMissingCategoryRejectedFSV(t *testing.T) {
	body := `
[[unit]]
type = "hfoo"
attack = "footman_attack"
ready = "footman_ready"
ack = "footman_ack"
order_ack = "footman_order"
under_attack = "footman_warn"
`
	tbl, err := loadSet(t, body) // death omitted
	if err == nil {
		t.Fatal("a set missing its death cue must be rejected")
	}
	if tbl != nil {
		t.Fatal("partial table returned alongside error (must be nil)")
	}
	if !strings.Contains(err.Error(), "hfoo") || !strings.Contains(err.Error(), "death") {
		t.Fatalf("error must name the unit and the missing category: %v", err)
	}
	t.Logf("FSV #313 edge: omitted death cue rejected → %v", err)
}

// Edge 2 — a cue that is not in the classification table (dangling ref): reject as
// "missing sound ref", naming the cue.
func TestSoundSetUnclassifiedCueRejectedFSV(t *testing.T) {
	body := strings.Replace(fullFootmanSet, "footman_death", "footman_ghost", 1)
	tbl, err := loadSet(t, body)
	if err == nil {
		t.Fatal("a cue absent from the classification table must be rejected")
	}
	if tbl != nil {
		t.Fatal("partial table returned alongside error")
	}
	if !strings.Contains(err.Error(), "footman_ghost") || !strings.Contains(err.Error(), "not classified") {
		t.Fatalf("error must name the dangling cue as unclassified: %v", err)
	}
	t.Logf("FSV #313 edge: dangling cue ref rejected → %v", err)
}

// Edge 3 — duplicate unit-type rows: reject (last-wins would silently shadow).
func TestSoundSetDuplicateTypeRejectedFSV(t *testing.T) {
	tbl, err := loadSet(t, fullFootmanSet+fullFootmanSet)
	if err == nil {
		t.Fatal("duplicate unit-type rows must be rejected")
	}
	if tbl != nil {
		t.Fatal("partial table returned alongside error")
	}
	if !strings.Contains(err.Error(), "duplicate") || !strings.Contains(err.Error(), "hfoo") {
		t.Fatalf("error must report the duplicate type: %v", err)
	}
	t.Logf("FSV #313 edge: duplicate hfoo row rejected → %v", err)
}

// Edge 4 — empty type field: reject, naming the row.
func TestSoundSetEmptyTypeRejectedFSV(t *testing.T) {
	body := strings.Replace(fullFootmanSet, `type = "hfoo"`, `type = ""`, 1)
	tbl, err := loadSet(t, body)
	if err == nil {
		t.Fatal("an empty unit type must be rejected")
	}
	if tbl != nil {
		t.Fatal("partial table returned alongside error")
	}
	if !strings.Contains(err.Error(), "empty type") {
		t.Fatalf("error must report the empty type: %v", err)
	}
	t.Logf("FSV #313 edge: empty type rejected → %v", err)
}

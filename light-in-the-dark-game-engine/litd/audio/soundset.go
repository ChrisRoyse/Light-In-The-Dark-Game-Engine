package audio

// Unit-type sound SETS (#313): the data-driven table that maps a unit type to the
// cue it plays for each gameplay moment — attack impact, death, train-ready,
// selection ack, order ack, under-attack stinger. Distinct from the sound
// CLASSIFICATION table (soundtable.go, #428), which maps a cue to its domain /
// priority / .ogg: a sound-set row only NAMES cues, and every named cue must
// already be classified. The render-side trigger (litd/render) consumes this to
// turn sim events into AudioEvents.
//
// Fail-closed (R-FMT-2, doctrine §2.4): Load rejects — with NO partial table — a
// row missing a type, a duplicate type, an empty category, or a cue that is not
// present in the classification table ("missing sound ref = load-time error, not
// runtime silence"). A sound set is all-or-nothing: a unit type either has a
// complete, fully-classified set or the build fails naming the offending row.

import (
	"fmt"
	"io/fs"
	"sort"
	"strings"

	"github.com/BurntSushi/toml"
)

// SoundCategory enumerates the gameplay moments a unit-type sound set covers.
type SoundCategory int

const (
	CatAttack      SoundCategory = iota // attack impact — world, 3D
	CatDeath                            // unit death — world, 3D
	CatReady                            // train-ready — UI, 2D
	CatAck                              // selection acknowledgement — UI, 2D
	CatOrderAck                         // order acknowledgement — UI, 2D
	CatUnderAttack                      // under-attack stinger — throttled
	numCategories
)

// String names the category for error messages and the table token.
func (c SoundCategory) String() string {
	switch c {
	case CatAttack:
		return "attack"
	case CatDeath:
		return "death"
	case CatReady:
		return "ready"
	case CatAck:
		return "ack"
	case CatOrderAck:
		return "order_ack"
	case CatUnderAttack:
		return "under_attack"
	}
	return "invalid"
}

// SoundSet is one unit type's complete cue set, indexed by SoundCategory. Every
// slot is a non-empty, classified cue (Load guarantees it).
type SoundSet struct {
	cues [numCategories]string
}

// Cue returns the cue for category c (always non-empty for a loaded set).
func (s SoundSet) Cue(c SoundCategory) string {
	if c < 0 || c >= numCategories {
		return ""
	}
	return s.cues[c]
}

// SoundSetTable is the parsed, fail-closed-validated unit-type → SoundSet map.
type SoundSetTable struct {
	byType map[string]SoundSet
	order  []string // sorted type codes — deterministic iteration
}

// Lookup returns the sound set for a unit-type code and whether it has one.
func (t *SoundSetTable) Lookup(unitType string) (SoundSet, bool) {
	s, ok := t.byType[unitType]
	return s, ok
}

// Len reports the number of unit types with a sound set.
func (t *SoundSetTable) Len() int { return len(t.order) }

// Types returns the unit-type codes in deterministic (sorted) order.
func (t *SoundSetTable) Types() []string {
	out := make([]string, len(t.order))
	copy(out, t.order)
	return out
}

type rawSoundSetTable struct {
	Unit []rawSoundSet `toml:"unit"`
}

type rawSoundSet struct {
	Type        string `toml:"type"`
	Attack      string `toml:"attack"`
	Death       string `toml:"death"`
	Ready       string `toml:"ready"`
	Ack         string `toml:"ack"`
	OrderAck    string `toml:"order_ack"`
	UnderAttack string `toml:"under_attack"`
}

// LoadSoundSetTable parses and fail-closed validates the unit-type sound-set table
// at p within fsys, cross-checking every referenced cue against classify (the
// #428 classification table). Returns the FIRST structural error rather than a
// partial table — a missing/unclassified cue is a build error, never a default.
func LoadSoundSetTable(fsys fs.FS, p string, classify *SoundTable) (*SoundSetTable, error) {
	if classify == nil {
		return nil, fmt.Errorf("sound-set table %s: nil classification table (cannot validate cue refs)", p)
	}
	body, err := fs.ReadFile(fsys, p)
	if err != nil {
		return nil, fmt.Errorf("sound-set table %s: %w", p, err)
	}
	var raw rawSoundSetTable
	if err := toml.Unmarshal(body, &raw); err != nil {
		return nil, fmt.Errorf("sound-set table %s: %w", p, err)
	}

	t := &SoundSetTable{byType: make(map[string]SoundSet, len(raw.Unit))}
	for i, r := range raw.Unit {
		code := strings.TrimSpace(r.Type)
		if code == "" {
			return nil, fmt.Errorf("sound-set table %s: unit row %d has empty type", p, i)
		}
		if _, dup := t.byType[code]; dup {
			return nil, fmt.Errorf("sound-set table %s: duplicate unit type %q", p, code)
		}
		// A sound set is all-or-nothing: every category must name a cue, and every
		// named cue must already be classified (#428). Either gap fails the build.
		fields := [numCategories]string{
			CatAttack:      strings.TrimSpace(r.Attack),
			CatDeath:       strings.TrimSpace(r.Death),
			CatReady:       strings.TrimSpace(r.Ready),
			CatAck:         strings.TrimSpace(r.Ack),
			CatOrderAck:    strings.TrimSpace(r.OrderAck),
			CatUnderAttack: strings.TrimSpace(r.UnderAttack),
		}
		var set SoundSet
		for c := SoundCategory(0); c < numCategories; c++ {
			cue := fields[c]
			if cue == "" {
				return nil, fmt.Errorf("sound-set table %s: unit %q missing %s sound cue", p, code, c)
			}
			if _, ok := classify.Lookup(cue); !ok {
				return nil, fmt.Errorf("sound-set table %s: unit %q %s cue %q is not classified (missing sound ref)", p, code, c, cue)
			}
			set.cues[c] = cue
		}
		t.byType[code] = set
		t.order = append(t.order, code)
	}
	sort.Strings(t.order)
	return t, nil
}

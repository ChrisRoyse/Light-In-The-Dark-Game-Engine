package audio

// soundtable.go — the audio classification data table (#428): the authoritative
// per-cue source of a sound's playback DOMAIN (world|ui) and eviction PRIORITY,
// replacing the channel-inference interim in domains.go (DomainOf/GroupOf). A UI
// sound played "at" a unit must still classify as flat UI by its table entry, not
// by the channel or position it happens to be played on — so classification lives
// in data, per asset, not in the mixer plumbing.
//
// Fail-closed (doctrine §2.4, R-AUD-1): every entry must declare a non-empty cue
// and .ogg path, a valid domain, and a valid priority; cues are unique. A missing
// or malformed field is a hard error — an unclassified sound never silently
// defaults. The table is the SoT consumed by the Manager (domain/group routing)
// and the voice allocator (priority), and validated by `assetcheck` on every
// build. Content (the actual cue→ogg rows) is populated with the data-driven
// sound sets (#313); this defines the format and its validation.

import (
	"fmt"
	"io/fs"
	"sort"
	"strings"

	"github.com/BurntSushi/toml"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
)

// SoundEntry is one classified sound cue.
type SoundEntry struct {
	Cue      string
	Domain   Domain
	Priority Priority
	Ogg      string
}

// SoundTable is the parsed, validated cue→classification map. It is indexed both
// by cue string and by the cue's runtime id (api.CueID) so the Manager can recover
// a playing sound's classification from an AudioEvent.Cue.
type SoundTable struct {
	byCue map[string]SoundEntry
	byID  map[uint32]SoundEntry
	order []string // sorted cues — deterministic iteration
}

// Lookup returns the entry for cue and whether it is classified.
func (t *SoundTable) Lookup(cue string) (SoundEntry, bool) {
	e, ok := t.byCue[cue]
	return e, ok
}

// LookupByID returns the entry for a cue's runtime id (api.CueID(cue), i.e. the
// AudioEvent.Cue the Manager sees at play time) and whether it is classified.
func (t *SoundTable) LookupByID(id uint32) (SoundEntry, bool) {
	e, ok := t.byID[id]
	return e, ok
}

// Len reports the number of classified cues.
func (t *SoundTable) Len() int { return len(t.order) }

// Cues returns the classified cues in deterministic (sorted) order.
func (t *SoundTable) Cues() []string {
	out := make([]string, len(t.order))
	copy(out, t.order)
	return out
}

type rawSoundTable struct {
	Sound []rawSound `toml:"sound"`
}

type rawSound struct {
	Cue      string `toml:"cue"`
	Domain   string `toml:"domain"`
	Priority string `toml:"priority"`
	Ogg      string `toml:"ogg"`
}

// ParseDomain maps the table token to a Domain. The exported form lets the
// assetcheck gate and the Manager share one spelling of the allowed set.
func ParseDomain(s string) (Domain, bool) {
	switch strings.ToLower(strings.TrimSpace(s)) {
	case "world":
		return DomainWorld, true
	case "ui":
		return DomainUI, true
	}
	return 0, false
}

// ParsePriority maps the table token to a Priority.
func ParsePriority(s string) (Priority, bool) {
	switch strings.ToLower(strings.TrimSpace(s)) {
	case "ambient":
		return PrioAmbient, true
	case "attackimpact":
		return PrioAttackImpact, true
	case "death":
		return PrioDeath, true
	case "abilitycast":
		return PrioAbilityCast, true
	case "alert":
		return PrioAlert, true
	}
	return 0, false
}

// LoadSoundTable parses and fail-closed validates the sound classification table
// at p within fsys. Returns the first structural error rather than a partial
// table (an unclassified or malformed sound is a build error, never a default).
func LoadSoundTable(fsys fs.FS, p string) (*SoundTable, error) {
	body, err := fs.ReadFile(fsys, p)
	if err != nil {
		return nil, fmt.Errorf("sound table %s: %w", p, err)
	}
	var raw rawSoundTable
	if err := toml.Unmarshal(body, &raw); err != nil {
		return nil, fmt.Errorf("sound table %s: %w", p, err)
	}
	t := &SoundTable{
		byCue: make(map[string]SoundEntry, len(raw.Sound)),
		byID:  make(map[uint32]SoundEntry, len(raw.Sound)),
	}
	for i, r := range raw.Sound {
		cue := strings.TrimSpace(r.Cue)
		if cue == "" {
			return nil, fmt.Errorf("sound table %s: entry %d has an empty cue", p, i)
		}
		if _, dup := t.byCue[cue]; dup {
			return nil, fmt.Errorf("sound table %s: duplicate cue %q", p, cue)
		}
		id := api.CueID(cue)
		if prev, clash := t.byID[id]; clash {
			return nil, fmt.Errorf("sound table %s: cue %q and %q collide on id %d (FNV-32a) — rename one", p, cue, prev.Cue, id)
		}
		dom, ok := ParseDomain(r.Domain)
		if !ok {
			return nil, fmt.Errorf("sound table %s: sound %q has missing/invalid domain %q (want world|ui)", p, cue, r.Domain)
		}
		pri, ok := ParsePriority(r.Priority)
		if !ok {
			return nil, fmt.Errorf("sound table %s: sound %q has missing/invalid priority %q (want ambient|attackimpact|death|abilitycast|alert)", p, cue, r.Priority)
		}
		ogg := strings.TrimSpace(r.Ogg)
		if ogg == "" {
			return nil, fmt.Errorf("sound table %s: sound %q has no ogg path", p, cue)
		}
		if !strings.HasSuffix(strings.ToLower(ogg), ".ogg") {
			return nil, fmt.Errorf("sound table %s: sound %q ogg %q is not a .ogg (R-AUD-1)", p, cue, ogg)
		}
		e := SoundEntry{Cue: cue, Domain: dom, Priority: pri, Ogg: ogg}
		t.byCue[cue] = e
		t.byID[id] = e
		t.order = append(t.order, cue)
	}
	sort.Strings(t.order)
	return t, nil
}

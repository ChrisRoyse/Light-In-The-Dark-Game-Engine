package main

// overrides.go is where human judgment enters the pipeline (tooling.md §2.2
// step 4). overrides.toml is reviewed in git; the tool applies it over the
// heuristic classes. An override always wins and flips classifiedBy to
// "override". Tombstones can ONLY come from overrides (G1.4) — the classifier
// never tombstones on its own. Every override carries a mandatory reason.

import (
	"errors"
	"fmt"
	"io/fs"
	"os"
	"sort"

	"github.com/BurntSushi/toml"
)

// Override is one reviewed classification/mapping decision.
type Override struct {
	Name        string `toml:"name"`
	Class       string `toml:"class"`       // D1-D5 (optional when tombstoning)
	GoMapping   string `toml:"goMapping"`   // canonical symbol, optional
	GoSignature string `toml:"goSignature"` // Go signature text e.g. "() bool", optional
	Package     string `toml:"package"`     // litd/api | litd/api/helpers | litd/ai
	Tombstone   string `toml:"tombstone"`   // tombstone reason enum value, optional
	Reason      string `toml:"reason"`      // mandatory
}

type overridesFile struct {
	Override []Override `toml:"override"`
}

// tombstoneReasons is the closed enum (deduplication-policy.md §7;
// deferred-v2 retained per D-2026-06-11-6/5).
var tombstoneReasons = map[string]bool{
	"deprecated":          true,
	"gameplay-irrelevant": true,
	"superseded":          true,
	"deferred-v2":         true,
}

// validPackages bounds the goMapping package field.
var validPackages = map[string]bool{
	"litd/api":         true,
	"litd/api/helpers": true,
	"litd/ai":          true,
	"":                 true, // allowed when no goMapping (e.g. tombstone)
}

// LoadOverrides parses overrides.toml. A missing file is not an error (no
// overrides applied); a malformed file is.
func LoadOverrides(path string) ([]Override, error) {
	if _, statErr := os.Stat(path); errors.Is(statErr, fs.ErrNotExist) {
		return nil, nil // absence of the optional reviewed file = no overrides
	}
	var f overridesFile
	md, err := toml.DecodeFile(path, &f)
	if err != nil {
		return nil, fmt.Errorf("overrides: parse %s: %w", path, err)
	}
	if undec := md.Undecoded(); len(undec) > 0 {
		return nil, fmt.Errorf("overrides: %s has unknown keys: %v", path, undec)
	}
	return f.Override, nil
}

// ApplyOverrides validates overrides against the classified universe and
// applies them. It fails closed: an unknown name, missing reason, bad
// tombstone reason, bad package, or duplicate name returns an error and applies
// nothing partial beyond what was validated — callers should treat any error as
// fatal (nonzero exit).
func ApplyOverrides(cs []Classification, ovs []Override) ([]Classification, error) {
	index := map[string]int{}
	for i, c := range cs {
		index[c.Name] = i
	}

	seen := map[string]bool{}
	for _, o := range ovs {
		if _, ok := index[o.Name]; !ok {
			return nil, fmt.Errorf("override for unknown function %q", o.Name)
		}
		if seen[o.Name] {
			return nil, fmt.Errorf("duplicate override for %q", o.Name)
		}
		seen[o.Name] = true
		if o.Reason == "" {
			return nil, fmt.Errorf("override for %q missing required reason", o.Name)
		}
		if o.Tombstone != "" && !tombstoneReasons[o.Tombstone] {
			return nil, fmt.Errorf("override for %q has tombstone reason %q outside enum [deprecated gameplay-irrelevant superseded deferred-v2]", o.Name, o.Tombstone)
		}
		if !validPackages[o.Package] {
			return nil, fmt.Errorf("override for %q has package %q outside [litd/api litd/api/helpers litd/ai]", o.Name, o.Package)
		}
		if o.Class != "" && !validClass(o.Class) {
			return nil, fmt.Errorf("override for %q has class %q outside D1-D5", o.Name, o.Class)
		}
	}

	// All overrides valid — apply (work on a copy so callers can diff).
	out := append([]Classification{}, cs...)
	for _, o := range ovs {
		i := index[o.Name]
		c := &out[i]
		c.ClassifiedBy = "override"
		if o.Class != "" {
			c.Class = Class(o.Class)
		}
		if o.Tombstone != "" {
			c.Tombstone = o.Tombstone
			c.Evidence = "tombstone: " + o.Reason
		} else {
			c.Evidence = "override: " + o.Reason
		}
		c.GoMapping = o.GoMapping
		c.GoSignature = o.GoSignature
		c.Package = o.Package
	}
	return out, nil
}

func validClass(s string) bool {
	switch Class(s) {
	case ClassD1, ClassD2, ClassD3, ClassD4, ClassD5:
		return true
	}
	return false
}

// CountOverridden returns how many classifications were sourced from overrides.
func CountOverridden(cs []Classification) int {
	n := 0
	for _, c := range cs {
		if c.ClassifiedBy == "override" {
			n++
		}
	}
	return n
}

// SortedOverrideNames returns override names in stable order (for deterministic dumps).
func SortedOverrideNames(ovs []Override) []string {
	names := make([]string, len(ovs))
	for i, o := range ovs {
		names[i] = o.Name
	}
	sort.Strings(names)
	return names
}

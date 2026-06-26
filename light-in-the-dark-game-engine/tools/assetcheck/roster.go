package main

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"

	"github.com/BurntSushi/toml"
)

// checkRosterTables (#517) is the unit/building/upgrade roster cross-ref pass.
// litd/data.Load already does the structural/numeric leg (strict decode, range
// checks, tick quantization); this adds the cross-reference leg the P9 content
// cluster names as its FSV source of truth:
//
//	(a) every `model`/`icon` resolves to an assets/MANIFEST asset id,
//	(b) every `abilities`/`trained-at`/`requires`/`upgrades-used` resolves to a
//	    defined roster/ability/upgrade entry,
//	(c) ids are unique across the roster.
//
// Buildings share the [[unit]] schema (they live in *_buildings.toml). It reads
// only text (TOML rows + the tracked MANIFEST), so it needs no asset binaries;
// GLB clip coherence (animation="Attack") is left to the binary-gated GLB pass.
func checkRosterTables(dir string, files []string, add func(path, rule, msg string)) {
	type rosterUnit struct {
		ID           string   `toml:"id"`
		Model        string   `toml:"model"`
		Icon         string   `toml:"icon"`
		Abilities    []string `toml:"abilities"`
		TrainedAt    string   `toml:"trained-at"`
		Requires     []string `toml:"requires"`
		UpgradesUsed []string `toml:"upgrades-used"`
	}
	type unitFile struct {
		Unit []rosterUnit `toml:"unit"`
	}
	type upgradeRow struct {
		ID       string   `toml:"id"`
		Requires []string `toml:"requires"`
	}
	type upgradeFile struct {
		Upgrade []upgradeRow `toml:"upgrade"`
	}
	type abilityRow struct {
		ID string `toml:"id"`
	}
	type abilityFile struct {
		Ability []abilityRow `toml:"ability"`
	}

	isUnit := func(rel string) bool { return underDir(rel, "units") }
	isUpgrade := func(rel string) bool { return underDir(rel, "upgrades") }
	isAbility := func(rel string) bool { return underDir(rel, "abilities") }

	// First pass: gather every defined id, flagging duplicates as we go.
	unitIDs := map[string]bool{}
	upgradeIDs := map[string]bool{}
	abilityIDs := map[string]bool{}
	type unitRef struct {
		rel string
		u   rosterUnit
	}
	var units []unitRef

	register := func(set map[string]bool, id, rel, kind string) {
		if id == "" {
			return
		}
		if set[id] {
			add(rel, "ROSTER-DUP-ID", fmt.Sprintf("%s id %q is already defined", kind, id))
			return
		}
		set[id] = true
	}

	for _, rel := range files {
		if strings.ToLower(filepath.Ext(rel)) != ".toml" {
			continue
		}
		full := filepath.Join(dir, rel)
		switch {
		case isUnit(rel):
			var f unitFile
			if _, err := toml.DecodeFile(full, &f); err != nil {
				add(rel, "ROSTER-PARSE", err.Error())
				continue
			}
			for _, u := range f.Unit {
				register(unitIDs, u.ID, rel, "unit")
				units = append(units, unitRef{rel, u})
			}
		case isUpgrade(rel):
			var f upgradeFile
			if _, err := toml.DecodeFile(full, &f); err != nil {
				add(rel, "ROSTER-PARSE", err.Error())
				continue
			}
			for _, up := range f.Upgrade {
				register(upgradeIDs, up.ID, rel, "upgrade")
			}
		case isAbility(rel):
			var f abilityFile
			if _, err := toml.DecodeFile(full, &f); err != nil {
				add(rel, "ROSTER-PARSE", err.Error())
				continue
			}
			for _, ab := range f.Ability {
				register(abilityIDs, ab.ID, rel, "ability")
			}
		}
	}

	if len(units) == 0 {
		return // no roster present in this tree: nothing to cross-ref
	}

	manifest := loadManifestPaths(dir)

	// Second pass: cross-ref each unit's references.
	for _, ref := range units {
		u := ref.u
		for _, ab := range u.Abilities {
			if !abilityIDs[ab] {
				add(ref.rel, "ROSTER-ABILITY-REF", fmt.Sprintf("unit %q references undefined ability %q", u.ID, ab))
			}
		}
		if u.TrainedAt != "" && !unitIDs[u.TrainedAt] {
			add(ref.rel, "ROSTER-TRAINEDAT-REF", fmt.Sprintf("unit %q trained-at %q is not a defined unit/building", u.ID, u.TrainedAt))
		}
		for _, req := range u.Requires {
			if !unitIDs[req] && !upgradeIDs[req] {
				add(ref.rel, "ROSTER-REQUIRES-REF", fmt.Sprintf("unit %q requires %q which is not a defined unit/building/upgrade", u.ID, req))
			}
		}
		for _, up := range u.UpgradesUsed {
			if !upgradeIDs[up] {
				add(ref.rel, "ROSTER-UPGRADE-REF", fmt.Sprintf("unit %q upgrades-used %q is not a defined upgrade", u.ID, up))
			}
		}
		// Asset cross-ref: model/icon must resolve to a MANIFEST entry. Skipped
		// when the MANIFEST is absent (can't validate) rather than failing open.
		if manifest != nil {
			if u.Model != "" && !manifest[u.Model] {
				add(ref.rel, "ROSTER-MODEL", fmt.Sprintf("unit %q model %q resolves to no assets/MANIFEST asset", u.ID, u.Model))
			}
			if u.Icon != "" && !manifest[u.Icon] {
				add(ref.rel, "ROSTER-ICON", fmt.Sprintf("unit %q icon %q resolves to no assets/MANIFEST asset", u.ID, u.Icon))
			}
		}
	}
}

// underDir reports whether rel sits in a top-level <name>/ directory of the
// validated tree (forward-slash normalized).
func underDir(rel, name string) bool {
	return strings.HasPrefix(filepath.ToSlash(rel), name+"/")
}

// loadManifestPaths reads assets/MANIFEST (sibling of the validated data dir)
// into a set of asset paths for the model/icon cross-ref. Returns nil when the
// MANIFEST cannot be found or parsed, so the caller skips the asset check
// rather than reporting spurious dangling refs.
func loadManifestPaths(dataDir string) map[string]bool {
	candidates := []string{
		filepath.Join(filepath.Dir(filepath.Clean(dataDir)), "assets", "MANIFEST"),
		filepath.Join(dataDir, "MANIFEST"),
	}
	var path string
	for _, c := range candidates {
		if _, err := os.Stat(c); err == nil {
			path = c
			break
		}
	}
	if path == "" {
		return nil
	}
	var doc struct {
		Asset []struct {
			Path string `toml:"path"`
		} `toml:"asset"`
	}
	if _, err := toml.DecodeFile(path, &doc); err != nil {
		return nil
	}
	set := make(map[string]bool, len(doc.Asset))
	for _, a := range doc.Asset {
		if a.Path != "" {
			set[a.Path] = true
		}
	}
	return set
}

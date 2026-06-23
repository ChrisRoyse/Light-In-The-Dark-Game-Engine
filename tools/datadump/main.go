// Command datadump loads a game-data directory through the real strict loader
// (litd/data.Load) and prints the interned, post-conversion tables as JSON, plus
// the canonical content Fingerprint. It is the headless "loader dump" Source of
// Truth the roster/building/upgrade content issues (#119-124, #153-157) name in
// their FSV sections: author a TOML table, load it here, and hand-check that the
// authored seconds/units-per-second rows converted to the expected ticks/fixed-
// point integers — the loader fails closed on unknown fields, out-of-range
// values, and non-tick-divisible durations, so a clean dump IS the validation.
//
// It deliberately does NOT decide anything — like tools/fsv it produces fast,
// structured evidence an agent reads to render the verdict (prompts/fsv.md).
//
// Usage:
//
//	go run ./tools/datadump -dir data                 # fingerprint + table sizes
//	go run ./tools/datadump -dir data -table units    # full units table as JSON
//	go run ./tools/datadump -dir data -table all       # every table as JSON
package main

import (
	"encoding/json"
	"flag"
	"fmt"
	"os"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
)

func unitName(t *data.Tables, idx uint16) string {
	if int(idx) < len(t.Units) {
		return t.Units[idx].ID
	}
	return fmt.Sprintf("#%d", idx)
}

func main() {
	dir := flag.String("dir", "data", "game-data directory to load")
	table := flag.String("table", "", "table to dump: units|upgrades|abilities|buffs|items|nodes|all (default: summary only)")
	flag.Parse()

	tables, err := data.Load(os.DirFS(*dir))
	if err != nil {
		// Fail-closed: a load error is the validator speaking. Surface it
		// verbatim (it names the file + field + reason) and exit non-zero.
		fmt.Fprintf(os.Stderr, "datadump: load %s: %v\n", *dir, err)
		os.Exit(1)
	}

	out := map[string]any{
		"dir":           *dir,
		"fingerprint":   fmt.Sprintf("0x%016x", tables.Fingerprint),
		"resourceTypes": tables.ResourceTypes,
		"counts": map[string]int{
			"units":     len(tables.Units),
			"upgrades":  len(tables.Upgrades),
			"abilities": len(tables.Abilities),
			"buffs":     len(tables.BuffTypes),
			"items":     len(tables.Items),
			"nodes":     len(tables.Nodes),
			"requires":  len(tables.Requires),
		},
	}

	switch *table {
	case "":
		// summary only
	case "units":
		out["units"] = tables.Units
	case "upgrades":
		out["upgrades"] = tables.Upgrades
	case "abilities":
		out["abilities"] = tables.Abilities
	case "buffs":
		out["buffs"] = tables.BuffTypes
	case "items":
		// Resolve recipe component indices + acquisition/stat enums to names.
		acq := []string{"unspecified", "found", "bought", "crafted"}
		stat := []string{"move-speed", "armor", "attack-cooldown", "attack-damage"}
		rows := make([]map[string]any, 0, len(tables.Items))
		for i := range tables.Items {
			it := &tables.Items[i]
			recipe := make([]string, 0, len(it.Recipe))
			for _, c := range it.Recipe {
				if int(c) < len(tables.Items) {
					recipe = append(recipe, tables.Items[c].ID)
				}
			}
			mods := make([]map[string]any, 0, len(it.Mods))
			for j := range it.Mods {
				mods = append(mods, map[string]any{"stat": stat[it.Mods[j].Stat], "add": it.Mods[j].Add})
			}
			a := "unspecified"
			if int(it.Acquisition) < len(acq) {
				a = acq[it.Acquisition]
			}
			rows = append(rows, map[string]any{
				"id": it.ID, "tier": it.Tier, "acquisition": a, "costs": it.Costs,
				"recipe": recipe, "mods": mods,
			})
		}
		out["items"] = rows
	case "nodes":
		out["nodes"] = tables.Nodes
	case "heroes":
		// Resolve hero unit + skill ability indices back to names for readability.
		if tables.Hero == nil {
			out["heroes"] = nil
			break
		}
		hs := make([]map[string]any, 0, len(tables.Hero.Heroes))
		for i := range tables.Hero.Heroes {
			hd := &tables.Hero.Heroes[i]
			skills := make([]map[string]any, 0, len(hd.Skills))
			for si := range hd.Skills {
				sk := &hd.Skills[si]
				ab := "#?"
				if int(sk.Ability) < len(tables.Abilities) {
					ab = tables.Abilities[sk.Ability].ID
				}
				skills = append(skills, map[string]any{"ability": ab, "minHeroLevel": sk.MinHeroLevel})
			}
			hs = append(hs, map[string]any{
				"unit": unitName(tables, hd.Unit), "str": int64(hd.Str), "agi": int64(hd.Agi),
				"int": int64(hd.Int), "strG": int64(hd.StrG), "agiG": int64(hd.AgiG), "intG": int64(hd.IntG),
				"skills": skills,
			})
		}
		out["heroSystem"] = map[string]any{
			"xpCurve": tables.Hero.Curve, "startSkillPts": tables.Hero.StartSkillPts,
			"shareRadius": int64(tables.Hero.ShareRadius), "deathPenalty": tables.Hero.DeathPenalty,
			"reviveBaseTicks": tables.Hero.Revive.BaseTicks, "reviveTicksPerLevel": tables.Hero.Revive.TicksPerLevel,
			"heroes": hs,
		}
	case "tech", "requires":
		// Resolve requirement rows back to names so the tech tree is readable:
		// each row = which target is gated on which alive prerequisites.
		rows := make([]map[string]any, 0, len(tables.Requires))
		for _, r := range tables.Requires {
			alive := make([]string, 0, len(r.Alive))
			for _, idx := range r.Alive {
				alive = append(alive, unitName(tables, idx))
			}
			target := unitName(tables, r.Target)
			if r.IsUpgrade && int(r.Target) < len(tables.Upgrades) {
				target = tables.Upgrades[r.Target].ID
			}
			rows = append(rows, map[string]any{"target": target, "isUpgrade": r.IsUpgrade, "alive": alive})
		}
		out["requires"] = rows
		out["resourceTypes"] = tables.ResourceTypes
	case "all":
		out["units"] = tables.Units
		out["upgrades"] = tables.Upgrades
		out["abilities"] = tables.Abilities
		out["buffs"] = tables.BuffTypes
		out["items"] = tables.Items
		out["nodes"] = tables.Nodes
	default:
		fmt.Fprintf(os.Stderr, "datadump: unknown -table %q\n", *table)
		os.Exit(2)
	}

	enc := json.NewEncoder(os.Stdout)
	enc.SetIndent("", "  ")
	if err := enc.Encode(out); err != nil {
		fmt.Fprintf(os.Stderr, "datadump: encode: %v\n", err)
		os.Exit(1)
	}
}

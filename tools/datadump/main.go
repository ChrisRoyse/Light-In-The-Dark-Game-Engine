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
		"dir":         *dir,
		"fingerprint": fmt.Sprintf("0x%016x", tables.Fingerprint),
		"counts": map[string]int{
			"units":     len(tables.Units),
			"upgrades":  len(tables.Upgrades),
			"abilities": len(tables.Abilities),
			"buffs":     len(tables.BuffTypes),
			"items":     len(tables.Items),
			"nodes":     len(tables.Nodes),
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
		out["items"] = tables.Items
	case "nodes":
		out["nodes"] = tables.Nodes
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

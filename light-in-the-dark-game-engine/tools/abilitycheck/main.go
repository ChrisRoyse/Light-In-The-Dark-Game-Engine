// abilitycheck is the build-time gate over composable-ability spec files
// (PRD2 06, custom-ability-authoring.md §4, #598). It mirrors assetcheck /
// apilint: walk a directory of *.toml ability specs and, for each:
//
//   - Reference resolution + range/type + precision: the same fail-closed
//     compile the runtime runs at world load (litd/ability.Compile), against a
//     resolver built from the spec's own declarations.
//   - Reference declaration: every effect-list name an op references must be
//     declared in the file's [ability.effects]; every mover kind must be known.
//   - Zero-alloc budget: the spec's worst-case primitive fan-out (movers and
//     projectile entities, multiplied through times/loop/for_each) must fit the
//     engine caps — a 5000-bolt nova warns it could exhaust Caps.Movers
//     (perf-budget §7).
//
// Usage:
//
//	go run ./tools/abilitycheck [--json] [--strict] [--group-max N] <dir>...
//
// Exit: 0 = clean (warnings allowed), 1 = errors (or warnings under --strict),
// 2 = usage/IO error. There are no bypass flags for the compile/reference rules.
package main

import (
	"encoding/json"
	"flag"
	"fmt"
	"io/fs"
	"os"
	"path/filepath"
	"sort"
	"strings"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/ability"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
)

type finding struct {
	File     string `json:"file"`
	Ability  string `json:"ability,omitempty"`
	Severity string `json:"severity"` // "error" | "warning"
	Rule     string `json:"rule"`
	Message  string `json:"message"`
}

func main() {
	jsonOut := flag.Bool("json", false, "emit findings as JSON")
	strict := flag.Bool("strict", false, "treat warnings as failures")
	groupMax := flag.Int("group-max", 64, "assumed worst-case group size for for_each fan-out budgeting")
	flag.Parse()
	dirs := flag.Args()
	if len(dirs) == 0 {
		fmt.Fprintln(os.Stderr, "usage: abilitycheck [--json] [--strict] [--group-max N] <dir>...")
		os.Exit(2)
	}

	files, err := collectTOML(dirs)
	if err != nil {
		fmt.Fprintf(os.Stderr, "abilitycheck: %v\n", err)
		os.Exit(2)
	}

	var findings []finding
	checked := 0
	for _, f := range files {
		findings = append(findings, checkFile(f, *groupMax)...)
		checked++
	}

	errs, warns := 0, 0
	for _, fi := range findings {
		if fi.Severity == "error" {
			errs++
		} else {
			warns++
		}
	}

	if *jsonOut {
		_ = json.NewEncoder(os.Stdout).Encode(map[string]any{
			"checked": checked, "errors": errs, "warnings": warns, "findings": findings,
		})
	} else {
		for _, fi := range findings {
			tag := strings.ToUpper(fi.Severity)
			if fi.Ability != "" {
				fmt.Printf("%s: %s [%s] %s: %s\n", tag, fi.File, fi.Ability, fi.Rule, fi.Message)
			} else {
				fmt.Printf("%s: %s %s: %s\n", tag, fi.File, fi.Rule, fi.Message)
			}
		}
		fmt.Printf("abilitycheck: %d file(s), %d error(s), %d warning(s)\n", checked, errs, warns)
	}

	if errs > 0 || (*strict && warns > 0) {
		os.Exit(1)
	}
}

// collectTOML gathers *.toml files from the given dirs (recursively). A missing
// dir is not an error (the gate is a no-op when a map ships no abilities).
func collectTOML(dirs []string) ([]string, error) {
	var out []string
	for _, d := range dirs {
		info, err := os.Stat(d)
		if err != nil {
			if os.IsNotExist(err) {
				continue
			}
			return nil, err
		}
		if !info.IsDir() {
			if strings.HasSuffix(d, ".toml") {
				out = append(out, d)
			}
			continue
		}
		err = filepath.WalkDir(d, func(p string, e fs.DirEntry, err error) error {
			if err != nil {
				return err
			}
			if !e.IsDir() && strings.HasSuffix(p, ".toml") {
				out = append(out, p)
			}
			return nil
		})
		if err != nil {
			return nil, err
		}
	}
	sort.Strings(out)
	return out, nil
}

func checkFile(path string, groupMax int) []finding {
	blob, err := os.ReadFile(path)
	if err != nil {
		return []finding{{File: path, Severity: "error", Rule: "io", Message: err.Error()}}
	}
	tpl, err := ability.LoadTOML(blob)
	if err != nil {
		return []finding{{File: path, Severity: "error", Rule: "load", Message: err.Error()}}
	}
	var out []finding
	// Reference declaration (effect lists declared, mover kinds known).
	for _, e := range ability.CheckTemplateRefs(tpl) {
		out = append(out, finding{File: path, Ability: tpl.Source.ID, Severity: "error", Rule: "reference", Message: e.Error()})
	}
	// Compile (reference resolution + range/type + precision).
	if err := ability.Validate(tpl); err != nil {
		out = append(out, finding{File: path, Ability: tpl.Source.ID, Severity: "error", Rule: "compile", Message: err.Error()})
	}
	// Budget lint (worst-case primitive fan-out vs engine caps).
	movers, ents := budget(tpl.Source.OnCast, 1, groupMax)
	if movers > sim.EngineCaps.Movers {
		out = append(out, finding{File: path, Ability: tpl.Source.ID, Severity: "warning", Rule: "budget",
			Message: fmt.Sprintf("worst-case %d movers could exhaust Caps.Movers (%d) — bound the fan-out (times/loop count, group size)", movers, sim.EngineCaps.Movers)})
	}
	if ents > sim.EngineCaps.Units {
		out = append(out, finding{File: path, Ability: tpl.Source.ID, Severity: "warning", Rule: "budget",
			Message: fmt.Sprintf("worst-case %d projectile entities could exhaust Caps.Units (%d)", ents, sim.EngineCaps.Units)})
	}
	return out
}

// budget estimates the worst-case live movers and spawned projectile entities a
// cast could instantiate, multiplying through the fan-out ops. for_each_in_group
// is assumed to iterate up to groupMax members; times/loop multiply by their
// count. An unbounded count (<=0) is treated as 1 (the compiler bounds them).
func budget(ops []data.OpSource, fanout, groupMax int) (movers, ents int) {
	for i := range ops {
		op := &ops[i]
		switch op.Op {
		case "attach_mover":
			movers += fanout
		case "spawn_projectile":
			ents += fanout
		case "for_each_in_group":
			cm, ce := budget(op.Children, fanout*max1(groupMax), groupMax)
			movers += cm
			ents += ce
		case "times":
			cm, ce := budget(op.Children, fanout*max1(op.Count), groupMax)
			movers += cm
			ents += ce
		case "loop":
			cm, ce := budget(op.Children, fanout*max1(int(op.Arg)), groupMax)
			movers += cm
			ents += ce
		default:
			if len(op.Children) > 0 {
				cm, ce := budget(op.Children, fanout, groupMax)
				movers += cm
				ents += ce
			}
		}
	}
	return movers, ents
}

func max1(n int) int {
	if n < 1 {
		return 1
	}
	return n
}

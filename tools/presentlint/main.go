// Command presentlint enforces #449/#471 as architecture: presentation
// packages (litd/render, litd/audio) are a NON-HASHING trigger class. They must
// react to gameplay through the render-event snapshot (Snapshot.Events /
// EmitRenderEvent), never through the sim-hashing subscription path
// (api.OnEvent and its sugar, sim.World.Subscribe, sim trigger creation, or the
// OnDamage modifier sink). Attaching a render/audio consumer to the hashing
// path would make an audio-on game hash differently from an audio-off game,
// breaking replay (#210) and save/load (#204) parity — the #449 invariant.
//
// The check is name-based on call selectors, matching the grep-style rule #471
// describes: a method call whose name is in the forbidden set, made from any
// non-test .go file under a scanned package, is a loud failure. The allowed
// presentation sinks (OnAudio, OnCamera) set a Game field and never touch the
// sim, so they are not flagged.
//
// Usage: presentlint <dir> [<dir>...]   (defaults to litd/render litd/audio
// litd/match — match-flow drives presentation UI and must stay off the hashing
// path too, #665)
package main

import (
	"fmt"
	"go/ast"
	"go/parser"
	"go/token"
	"os"
	"path/filepath"
	"sort"
	"strings"
)

// forbidden maps a banned call-selector name to the reason it is banned for
// presentation code. These are the sim-hashing subscription entry points.
var forbidden = map[string]string{
	"OnEvent":       "api.Game.OnEvent subscribes on the sim's hashing trigger path (R-SIM-6)",
	"OnAbilityCast": "OnAbilityCast is sugar over OnEvent — hashing subscription path",
	"OnAttack":      "OnAttack is sugar over OnEvent — hashing subscription path",
	"OnBuffApplied": "OnBuffApplied is sugar over OnEvent — hashing subscription path",
	"OnDamage":      "OnDamage installs a sim damage modifier — presentation must never mutate the sim",
	"Subscribe":     "sim.World.Subscribe registers a hashing subscription (R-SIM-6)",
	"NewTrigger":    "creating a sim Trigger adds to the hashed trigger slab",
}

type finding struct {
	pos token.Position
	sel string
	why string
}

// scanDirs walks the given directories and returns every forbidden-call
// finding. Test files are skipped; build tags are ignored (fixtures use
// //go:build ignore so they never compile but still parse here).
func scanDirs(dirs []string) ([]finding, error) {
	fset := token.NewFileSet()
	var findings []finding
	for _, dir := range dirs {
		err := filepath.WalkDir(dir, func(path string, d os.DirEntry, err error) error {
			if err != nil {
				return err
			}
			if d.IsDir() || !strings.HasSuffix(path, ".go") || strings.HasSuffix(path, "_test.go") {
				return nil
			}
			f, perr := parser.ParseFile(fset, path, nil, 0)
			if perr != nil {
				return fmt.Errorf("parse %s: %w", path, perr)
			}
			ast.Inspect(f, func(n ast.Node) bool {
				call, ok := n.(*ast.CallExpr)
				if !ok {
					return true
				}
				sel, ok := call.Fun.(*ast.SelectorExpr)
				if !ok {
					return true
				}
				if why, banned := forbidden[sel.Sel.Name]; banned {
					findings = append(findings, finding{pos: fset.Position(sel.Sel.Pos()), sel: sel.Sel.Name, why: why})
				}
				return true
			})
			return nil
		})
		if err != nil {
			return nil, err
		}
	}
	sort.Slice(findings, func(i, j int) bool {
		if findings[i].pos.Filename != findings[j].pos.Filename {
			return findings[i].pos.Filename < findings[j].pos.Filename
		}
		return findings[i].pos.Line < findings[j].pos.Line
	})
	return findings, nil
}

func main() {
	dirs := os.Args[1:]
	if len(dirs) == 0 {
		dirs = []string{"litd/render", "litd/audio", "litd/match"}
	}
	findings, err := scanDirs(dirs)
	if err != nil {
		fmt.Fprintln(os.Stderr, "presentlint:", err)
		os.Exit(2)
	}
	for _, f := range findings {
		fmt.Fprintf(os.Stderr, "%s: presentation code calls %q — %s. Use the render-event snapshot (Snapshot.Events / EmitRenderEvent) instead (#449).\n",
			f.pos, f.sel, f.why)
	}
	if len(findings) > 0 {
		fmt.Fprintf(os.Stderr, "presentlint: %d violation(s) — presentation must not touch the hashing subscription path\n", len(findings))
		os.Exit(1)
	}
	fmt.Fprintf(os.Stderr, "presentlint: OK — %v are clean of the hashing subscription path\n", dirs)
}

// Command timerlint enforces R-TMR-8 (#557) as architecture: ability
// templates and other save-critical gameplay packages MUST schedule
// time with the serializable continuation API (AfterCont/LoopCont/
// CountCont) and never the convenience Go-closure sugar (Game.After /
// Game.Every), whose closures cannot be serialized and are dropped on
// load. A closure timer in persistent gameplay code is a latent
// save/load divergence: the behaviour silently vanishes after a load.
//
// The check is name-based on call selectors, the same grep-style rule
// presentlint uses: a call whose selector is After or Every, made from a
// non-test .go file under a scanned package, is a loud failure. Scanned
// packages are the ones that must be save-safe — ability templates by
// default. Transient schedulers (sched.After, Trigger.Every) live
// outside these packages and are not the target; if an ability template
// genuinely needs the closure form it must instead register a Cont.
//
// Usage: timerlint <dir> [<dir>...]   (defaults to ./abilities)
// A directory that does not exist is skipped (templates may not have
// landed yet), so the gate is safe to wire into preflight permanently.
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

// forbidden maps a banned call-selector name to why it is save-unsafe in
// gameplay/ability code.
var forbidden = map[string]string{
	"After": "Game.After holds a Go closure that is dropped on load — use AfterCont/CountCont with a registered Cont (R-TMR-8)",
	"Every": "Game.Every holds a Go closure that is dropped on load — use LoopCont with a registered Cont (R-TMR-8)",
}

type finding struct {
	pos token.Position
	sel string
	why string
}

func scanDirs(dirs []string) ([]finding, error) {
	fset := token.NewFileSet()
	var findings []finding
	for _, dir := range dirs {
		if st, err := os.Stat(dir); err != nil || !st.IsDir() {
			continue // not present yet — safe to skip (templates may not exist)
		}
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
		dirs = []string{"abilities"}
	}
	findings, err := scanDirs(dirs)
	if err != nil {
		fmt.Fprintln(os.Stderr, "timerlint:", err)
		os.Exit(2)
	}
	for _, f := range findings {
		fmt.Fprintf(os.Stderr, "%s: save-critical code calls %q — %s.\n", f.pos, f.sel, f.why)
	}
	if len(findings) > 0 {
		fmt.Fprintf(os.Stderr, "timerlint: %d violation(s) — ability/gameplay code must use the serializable continuation timer API\n", len(findings))
		os.Exit(1)
	}
	fmt.Fprintf(os.Stderr, "timerlint: OK — %v use only serializable timers\n", dirs)
}

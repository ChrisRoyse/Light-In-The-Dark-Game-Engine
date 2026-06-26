// Command eventlint enforces R-EVT-1 (#618) as architecture: custom event
// kinds MUST be registered at deterministic world setup, never mid-match
// from inside a handler/callback/coroutine. RegisterEvent /
// RegisterEventKind assigns ids sequentially in call order, so a
// registration driven by event dispatch, a timer, or (worst) wall-clock/
// UI input would assign ids in a run-dependent order and diverge the
// state hash + break save compatibility.
//
// The heuristic: a RegisterEvent/RegisterEventKind call is allowed at
// statement/top-level scope (world setup) and FLAGGED when it sits inside
// a function literal (closure) — the shape of an OnEvent handler, a timer
// callback, or a Run coroutine body, i.e. mid-match. A call is "inside a
// closure" if its position falls within any *ast.FuncLit range in the
// file. Setup helpers that are plain named functions are unaffected.
//
// Usage: eventlint <dir> [<dir>...]   (defaults to ./abilities and ./worlds)
// A directory that does not exist is skipped, so the gate is safe to wire
// into preflight permanently.
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

// flagged maps a banned-in-closure call-selector name to why.
var flagged = map[string]string{
	"RegisterEvent":     "custom event kinds must be registered at world setup, not inside a closure (handler/callback/coroutine) — mid-match registration assigns run-dependent ids and diverges the hash (R-EVT-1)",
	"RegisterEventKind": "custom event kinds must be registered at world setup, not inside a closure — see R-EVT-1",
}

type finding struct {
	pos token.Position
	sel string
	why string
}

type rng struct{ lo, hi token.Pos }

func scanDirs(dirs []string) ([]finding, error) {
	fset := token.NewFileSet()
	var findings []finding
	for _, dir := range dirs {
		if st, err := os.Stat(dir); err != nil || !st.IsDir() {
			continue // not present yet — safe to skip
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
			// Pass 1: collect every function-literal range in the file.
			var lits []rng
			ast.Inspect(f, func(n ast.Node) bool {
				if fl, ok := n.(*ast.FuncLit); ok {
					lits = append(lits, rng{fl.Pos(), fl.End()})
				}
				return true
			})
			inClosure := func(p token.Pos) bool {
				for _, r := range lits {
					if p >= r.lo && p < r.hi {
						return true
					}
				}
				return false
			}
			// Pass 2: flag banned calls that sit inside any closure.
			ast.Inspect(f, func(n ast.Node) bool {
				call, ok := n.(*ast.CallExpr)
				if !ok {
					return true
				}
				name := calleeName(call.Fun)
				why, banned := flagged[name]
				if banned && inClosure(call.Pos()) {
					findings = append(findings, finding{pos: fset.Position(call.Pos()), sel: name, why: why})
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

// calleeName returns the called function's identifier name for either a
// bare call (Foo) or a selector call (x.Foo).
func calleeName(fun ast.Expr) string {
	switch e := fun.(type) {
	case *ast.Ident:
		return e.Name
	case *ast.SelectorExpr:
		return e.Sel.Name
	}
	return ""
}

func main() {
	dirs := os.Args[1:]
	if len(dirs) == 0 {
		dirs = []string{"abilities", "worlds"}
	}
	findings, err := scanDirs(dirs)
	if err != nil {
		fmt.Fprintln(os.Stderr, "eventlint:", err)
		os.Exit(2)
	}
	for _, f := range findings {
		fmt.Fprintf(os.Stderr, "%s: %q inside a closure — %s.\n", f.pos, f.sel, f.why)
	}
	if len(findings) > 0 {
		fmt.Fprintf(os.Stderr, "eventlint: %d violation(s) — register custom event kinds at world setup\n", len(findings))
		os.Exit(1)
	}
	fmt.Fprintf(os.Stderr, "eventlint: OK — %v register custom kinds only at setup scope\n", dirs)
}

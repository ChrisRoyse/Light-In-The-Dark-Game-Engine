// importcheck enforces the PRD §4.1 import-graph rule: litd/sim never
// (transitively) imports litd/render, the G3N engine, or any GL/window
// package. Run as `go run ./tools/importcheck`; CI runs it as a
// required step.
//
// On violation it prints the full offending import chain, not just the
// verdict. It also prints the audited-package count and fails if that
// count is zero — the check is never vacuously green.
package main

import (
	"bytes"
	"encoding/json"
	"fmt"
	"io"
	"os"
	"os/exec"
	"strings"
)

const module = "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine"

// bannedPrefixes: any package whose import path starts with one of these
// must be unreachable from litd/sim.
var bannedPrefixes = []string{
	module + "/litd/render",
	"github.com/g3n/engine",
	"github.com/go-gl/glfw",
	"github.com/go-gl/gl",
	"github.com/go-gl/mathgl",
}

type pkg struct {
	ImportPath string
	Standard   bool
	Imports    []string
}

func main() {
	out, err := exec.Command("go", "list", "-deps", "-json", module+"/litd/sim/...").Output()
	if err != nil {
		var stderr string
		if ee, ok := err.(*exec.ExitError); ok {
			stderr = string(ee.Stderr)
		}
		fmt.Fprintf(os.Stderr, "importcheck: go list failed: %v\n%s", err, stderr)
		os.Exit(2)
	}

	pkgs, err := parsePkgs(bytes.NewReader(out))
	if err != nil {
		fmt.Fprintln(os.Stderr, "importcheck: decode go list output:", err)
		os.Exit(2)
	}

	simPkgs := rootsByPrefix(pkgs, module+"/litd/sim")
	if len(simPkgs) == 0 {
		fmt.Fprintln(os.Stderr, "importcheck: FAIL: zero litd/sim packages audited — check would be vacuously green")
		os.Exit(1)
	}

	violations := findViolations(pkgs, simPkgs, bannedPrefixes)
	for _, v := range violations {
		fmt.Printf("FAIL: %s reaches banned package %s\n  chain: %s\n", v.Root, v.Banned, strings.Join(v.Chain, "\n      -> "))
	}

	fmt.Printf("importcheck: audited %d litd/sim package(s), %d dep(s) total\n", len(simPkgs), len(pkgs))
	fmt.Println("banned prefixes checked:")
	for _, b := range bannedPrefixes {
		fmt.Println("  ", b)
	}
	if len(violations) > 0 {
		fmt.Printf("importcheck: FAIL — %d violation(s)\n", len(violations))
		os.Exit(1)
	}
	fmt.Println("importcheck: OK")
}

// Violation is one banned-package reachability finding: the root package that
// reaches a banned package, plus the full import chain that gets there.
type Violation struct {
	Root   string
	Banned string
	Chain  []string
}

// parsePkgs decodes a `go list -deps -json` stream into a path→pkg map.
func parsePkgs(r io.Reader) (map[string]pkg, error) {
	pkgs := map[string]pkg{}
	dec := json.NewDecoder(r)
	for {
		var p pkg
		if err := dec.Decode(&p); err == io.EOF {
			break
		} else if err != nil {
			return nil, err
		}
		pkgs[p.ImportPath] = p
	}
	return pkgs, nil
}

// rootsByPrefix returns every package path that starts with prefix.
func rootsByPrefix(pkgs map[string]pkg, prefix string) []string {
	var roots []string
	for path := range pkgs {
		if strings.HasPrefix(path, prefix) {
			roots = append(roots, path)
		}
	}
	return roots
}

// isBanned reports whether path is, or sits under, any banned prefix. The b+"/"
// boundary prevents a false match on a sibling like "litd/rendering".
func isBanned(path string, banned []string) bool {
	for _, b := range banned {
		if path == b || strings.HasPrefix(path, b+"/") {
			return true
		}
	}
	return false
}

// findViolations BFS-walks the import graph from each root and returns every
// banned package reachable, with the chain reaching it. Standard-library
// packages terminate traversal — they cannot reach project or banned code.
func findViolations(pkgs map[string]pkg, roots, banned []string) []Violation {
	var out []Violation
	for _, root := range roots {
		parent := map[string]string{root: ""}
		queue := []string{root}
		for len(queue) > 0 {
			cur := queue[0]
			queue = queue[1:]
			for _, imp := range pkgs[cur].Imports {
				if _, seen := parent[imp]; seen {
					continue
				}
				parent[imp] = cur
				if isBanned(imp, banned) {
					var chain []string
					for n := imp; n != ""; n = parent[n] {
						chain = append([]string{n}, chain...)
					}
					out = append(out, Violation{Root: root, Banned: imp, Chain: chain})
					continue
				}
				if !pkgs[imp].Standard {
					queue = append(queue, imp)
				}
			}
		}
	}
	return out
}

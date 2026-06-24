// importcheck enforces the project's static import-graph invariants (see
// the `rules` list). Today:
//   - PRD §4.1: litd/sim never (transitively) imports litd/render, the G3N
//     engine, or any GL/window package.
//   - #311: litd/config (the settings data layer) never imports litd/sim, so
//     settings add zero determinism surface.
// Run as `go run ./tools/importcheck`; preflight runs it as a required step.
//
// On violation it prints the full offending import chain, not just the
// verdict. Each rule fails if zero packages are audited under its root — the
// check is never vacuously green.
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
// must be unreachable from litd/sim (PRD §4.1).
var bannedPrefixes = []string{
	module + "/litd/render",
	"github.com/g3n/engine",
	"github.com/go-gl/glfw",
	"github.com/go-gl/gl",
	"github.com/go-gl/mathgl",
}

// rule is one import-graph invariant: no package under rootPrefix may
// (transitively) reach any banned prefix.
type rule struct {
	name       string
	rootPrefix string
	banned     []string
}

// rules enumerated here are the static import-graph invariants. The go list
// target below MUST include every rootPrefix's tree so its packages are present
// in the graph (else the vacuity guard fires).
var rules = []rule{
	{"litd/sim ⊥ render/G3N/GL (PRD §4.1)", module + "/litd/sim", bannedPrefixes},
	// #311: the settings DATA layer must never reach the sim — settings are a
	// pure presentation/config surface and must add zero determinism surface.
	// (litd/menu legitimately holds an *api.Game and so reaches sim transitively
	// via the api facade; its determinism isolation is proven dynamically by
	// litd/menu's sim-isolation test, not statically — only the config data
	// package is statically sim-free, and this locks that in.)
	{"litd/config ⊥ litd/sim (settings determinism, #311)", module + "/litd/config", []string{module + "/litd/sim"}},
}

// listTargets is the union of every rule's tree, so go list -deps loads each
// rooted graph into one pkg map.
var listTargets = []string{module + "/litd/sim/...", module + "/litd/config/..."}

type pkg struct {
	ImportPath string
	Standard   bool
	Imports    []string
}

func main() {
	args := append([]string{"list", "-deps", "-json"}, listTargets...)
	out, err := exec.Command("go", args...).Output()
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

	total := 0
	for _, ru := range rules {
		roots := rootsByPrefix(pkgs, ru.rootPrefix)
		if len(roots) == 0 {
			fmt.Fprintf(os.Stderr, "importcheck: FAIL: zero packages under %s audited — rule %q would be vacuously green\n", ru.rootPrefix, ru.name)
			os.Exit(1)
		}
		violations := findViolations(pkgs, roots, ru.banned)
		for _, v := range violations {
			fmt.Printf("FAIL [%s]: %s reaches banned package %s\n  chain: %s\n", ru.name, v.Root, v.Banned, strings.Join(v.Chain, "\n      -> "))
		}
		fmt.Printf("importcheck: rule %q — audited %d package(s) under %s, banned %v → %d violation(s)\n",
			ru.name, len(roots), ru.rootPrefix, ru.banned, len(violations))
		total += len(violations)
	}

	if total > 0 {
		fmt.Printf("importcheck: FAIL — %d violation(s) across %d rule(s)\n", total, len(rules))
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

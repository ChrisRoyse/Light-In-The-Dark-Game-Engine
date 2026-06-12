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

	pkgs := map[string]pkg{}
	dec := json.NewDecoder(bytes.NewReader(out))
	for {
		var p pkg
		if err := dec.Decode(&p); err == io.EOF {
			break
		} else if err != nil {
			fmt.Fprintln(os.Stderr, "importcheck: decode go list output:", err)
			os.Exit(2)
		}
		pkgs[p.ImportPath] = p
	}

	var simPkgs []string
	for path := range pkgs {
		if strings.HasPrefix(path, module+"/litd/sim") {
			simPkgs = append(simPkgs, path)
		}
	}
	if len(simPkgs) == 0 {
		fmt.Fprintln(os.Stderr, "importcheck: FAIL: zero litd/sim packages audited — check would be vacuously green")
		os.Exit(1)
	}

	banned := func(path string) bool {
		for _, b := range bannedPrefixes {
			if path == b || strings.HasPrefix(path, b+"/") {
				return true
			}
		}
		return false
	}

	// BFS from each sim package, recording parent links to reconstruct chains.
	violations := 0
	for _, root := range simPkgs {
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
				if banned(imp) {
					var chain []string
					for n := imp; n != ""; n = parent[n] {
						chain = append([]string{n}, chain...)
					}
					fmt.Printf("FAIL: %s reaches banned package %s\n  chain: %s\n", root, imp, strings.Join(chain, "\n      -> "))
					violations++
					continue
				}
				if !pkgs[imp].Standard {
					queue = append(queue, imp)
				}
			}
		}
	}

	fmt.Printf("importcheck: audited %d litd/sim package(s), %d dep(s) total\n", len(simPkgs), len(pkgs))
	fmt.Println("banned prefixes checked:")
	for _, b := range bannedPrefixes {
		fmt.Println("  ", b)
	}
	if violations > 0 {
		fmt.Printf("importcheck: FAIL — %d violation(s)\n", violations)
		os.Exit(1)
	}
	fmt.Println("importcheck: OK")
}

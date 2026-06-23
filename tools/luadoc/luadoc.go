// Command luadoc generates the public Lua scripting API reference from
// api-manifest.json (#187): one Markdown entry per ACTIVE (mapped) function — the
// surface a world author can call — excluding the tombstoned/deferred decls. It
// is one of the two public surfaces #187 publishes (the other is the world
// archive format spec); the engine source stays closed.
//
// -check is the drift gate: it regenerates the reference and fails if the
// committed file differs, so a manifest change that is not accompanied by a docs
// regeneration breaks the local gate (the no-CI equivalent of "drift fails the
// build"). Output is deterministic (functions sorted by name, no timestamps) so
// the committed file is stable across runs.
package main

import (
	"bytes"
	"encoding/json"
	"fmt"
	"os"
	"sort"
)

type manifest struct {
	SchemaVersion int        `json:"schemaVersion"`
	Functions     []function `json:"functions"`
}

type function struct {
	Name        string    `json:"name"`
	Origin      string    `json:"origin"`
	Disposition string    `json:"disposition"`
	Signature   signature `json:"signature"`
	GoMapping   *goMap    `json:"goMapping"`
}

type signature struct {
	Params  []param `json:"params"`
	Returns string  `json:"returns"`
}

type param struct {
	Name     string `json:"name"`
	JassType string `json:"jassType"`
}

type goMap struct {
	Package     string `json:"package"`
	Symbol      string `json:"symbol"`
	GoSignature string `json:"goSignature"`
}

// GenerateReference renders the public API reference. Only mapped (active)
// functions appear; they are sorted by name for a stable diff.
func GenerateReference(m manifest) string {
	active := make([]function, 0, len(m.Functions))
	for _, f := range m.Functions {
		if f.Disposition == "mapped" && f.GoMapping != nil {
			active = append(active, f)
		}
	}
	sort.Slice(active, func(i, j int) bool { return active[i].Name < active[j].Name })

	var b bytes.Buffer
	fmt.Fprintf(&b, "# Lua Scripting API Reference\n\n")
	fmt.Fprintf(&b, "Generated from `api-manifest.json` (schema v%d) by `tools/luadoc`. Do not hand-edit.\n\n", m.SchemaVersion)
	fmt.Fprintf(&b, "%d callable functions. Tombstoned/deferred declarations are intentionally omitted.\n", len(active))
	for _, f := range active {
		fmt.Fprintf(&b, "\n## %s\n\n", f.Name)
		fmt.Fprintf(&b, "`%s(%s) -> %s`\n\n", f.Name, formatParams(f.Signature.Params), retOf(f.Signature.Returns))
		fmt.Fprintf(&b, "- Source: `%s`\n", f.Origin)
		fmt.Fprintf(&b, "- Maps to: `%s.%s` — `%s`\n", f.GoMapping.Package, f.GoMapping.Symbol, f.GoMapping.GoSignature)
	}
	return b.String()
}

func formatParams(ps []param) string {
	if len(ps) == 0 {
		return ""
	}
	parts := make([]string, len(ps))
	for i, p := range ps {
		parts[i] = p.Name + ": " + p.JassType
	}
	out := parts[0]
	for _, s := range parts[1:] {
		out += ", " + s
	}
	return out
}

func retOf(r string) string {
	if r == "" {
		return "nothing"
	}
	return r
}

func loadManifest(path string) (manifest, error) {
	b, err := os.ReadFile(path)
	if err != nil {
		return manifest{}, err
	}
	var m manifest
	if err := json.Unmarshal(b, &m); err != nil {
		return manifest{}, fmt.Errorf("parse %s: %w", path, err)
	}
	if m.SchemaVersion == 0 || len(m.Functions) == 0 {
		return manifest{}, fmt.Errorf("%s: empty or unversioned manifest", path)
	}
	return m, nil
}

func main() {
	var manifestPath, outPath string
	var check bool
	args := os.Args[1:]
	for i := 0; i < len(args); i++ {
		switch args[i] {
		case "-manifest":
			i++
			manifestPath = args[i]
		case "-o":
			i++
			outPath = args[i]
		case "-check":
			check = true
		default:
			fmt.Fprintf(os.Stderr, "luadoc: unknown arg %q\n", args[i])
			os.Exit(2)
		}
	}
	if manifestPath == "" {
		manifestPath = "api-manifest.json"
	}
	if outPath == "" {
		outPath = "docs/api/lua-reference.md"
	}

	m, err := loadManifest(manifestPath)
	if err != nil {
		fmt.Fprintln(os.Stderr, "luadoc:", err)
		os.Exit(1)
	}
	ref := GenerateReference(m)

	if check {
		committed, err := os.ReadFile(outPath)
		if err != nil {
			fmt.Fprintf(os.Stderr, "luadoc -check: cannot read %s: %v\n(run `go run ./tools/luadoc` to generate it)\n", outPath, err)
			os.Exit(1)
		}
		if string(committed) != ref {
			fmt.Fprintf(os.Stderr, "luadoc -check: %s is STALE vs api-manifest.json — regenerate it (drift gate)\n", outPath)
			os.Exit(1)
		}
		fmt.Printf("luadoc -check: %s is in sync with the manifest\n", outPath)
		return
	}

	if err := os.WriteFile(outPath, []byte(ref), 0o644); err != nil {
		fmt.Fprintln(os.Stderr, "luadoc:", err)
		os.Exit(1)
	}
	fmt.Printf("luadoc: wrote %s\n", outPath)
}

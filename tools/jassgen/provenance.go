package main

// provenance.go implements the G-2 provenance-injection mode (#259): every
// exported litd/api verb that ports one or more JASS functions carries a
// machine-generated `// JASS: <sources>` line as the final line of its doc
// comment. The sources are the manifest goMapping entry name plus every
// collapsesWith member, aggregated across all manifest functions that map to the
// same Go symbol, sorted and de-duplicated — so SetLife shows every state native
// that folded onto it. The line is regenerated, never hand-maintained; a drift
// between the committed line and the manifest is a CI failure (staleness gate).
//
// Editing is line-based and keyed to AST positions: only the `// JASS:` line is
// ever inserted or rewritten, never any prose. go/printer is deliberately NOT
// used (it would reformat the whole file and clobber comment layout).

import (
	"fmt"
	"go/ast"
	goparser "go/parser"
	gotoken "go/token"
	"io/fs"
	"os"
	"path/filepath"
	"sort"
	"strings"
)

// provenancePrefix is the canonical leading text of a provenance line.
const provenancePrefix = "// JASS: "

// BuildProvenance maps each goMapping symbol in pkg to its sorted, de-duped JASS
// source names (entry name + collapsesWith, unioned across every manifest
// function mapping to that symbol). Pure core for direct testing.
func BuildProvenance(m Manifest, pkg string) map[string][]string {
	acc := map[string]map[string]bool{}
	for _, f := range m.Functions {
		if f.GoMapping == nil || f.GoMapping.Package != pkg {
			continue
		}
		sym := f.GoMapping.Symbol
		if acc[sym] == nil {
			acc[sym] = map[string]bool{}
		}
		acc[sym][f.Name] = true
		for _, c := range f.GoMapping.CollapsesWith {
			acc[sym][c] = true
		}
	}
	out := make(map[string][]string, len(acc))
	for sym, set := range acc {
		names := make([]string, 0, len(set))
		for n := range set {
			names = append(names, n)
		}
		sort.Strings(names)
		out[sym] = names
	}
	return out
}

// provenanceLine renders the canonical provenance comment for a symbol's sources.
func provenanceLine(sources []string) string {
	return provenancePrefix + strings.Join(sources, ", ")
}

// symbolOf returns the "<Type>.<Method>" / "<Func>" name for an exported
// FuncDecl, or "" if it is unexported or a method on an unexported receiver.
func symbolOf(fd *ast.FuncDecl) string {
	if !fd.Name.IsExported() {
		return ""
	}
	if fd.Recv != nil && len(fd.Recv.List) > 0 {
		recv := receiverTypeName(fd.Recv.List[0].Type)
		if !ast.IsExported(recv) {
			return ""
		}
		return recv + "." + fd.Name.Name
	}
	return fd.Name.Name
}

// applyProvenanceToFile rewrites one Go file's bytes so that every exported
// verb whose symbol is in prov ends its doc comment with the canonical JASS
// line — appending it when absent, rewriting it in place when present. All
// other bytes are untouched. Returns (newSrc, changed, error). A mapped symbol
// with no doc comment is a hard error (run the G-1 gate first).
func applyProvenanceToFile(filename string, src []byte, prov map[string][]string) ([]byte, bool, error) {
	fset := gotoken.NewFileSet()
	f, err := goparser.ParseFile(fset, filename, src, goparser.ParseComments)
	if err != nil {
		return nil, false, err
	}
	type edit struct {
		line    int // 1-based: line to replace, or line-after-which to insert
		replace bool
		text    string
	}
	var edits []edit
	for _, decl := range f.Decls {
		fd, ok := decl.(*ast.FuncDecl)
		if !ok {
			continue
		}
		sym := symbolOf(fd)
		if sym == "" {
			continue
		}
		sources, ok := prov[sym]
		if !ok {
			continue // export with no JASS source (new capability) — no line
		}
		want := provenanceLine(sources)
		if fd.Doc == nil {
			return nil, false, fmt.Errorf("%s: mapped symbol %s has no doc comment (G-1 must pass first)", filename, sym)
		}
		jassLine := -1
		for _, c := range fd.Doc.List {
			if strings.HasPrefix(c.Text, "// JASS:") {
				jassLine = fset.Position(c.Slash).Line
			}
		}
		if jassLine >= 0 {
			edits = append(edits, edit{line: jassLine, replace: true, text: want})
		} else {
			edits = append(edits, edit{line: fset.Position(fd.Doc.End()).Line, replace: false, text: want})
		}
	}
	if len(edits) == 0 {
		return src, false, nil
	}
	lines := strings.Split(string(src), "\n")
	// Apply bottom-up so earlier (higher-line) inserts/replaces don't shift the
	// indices of later (lower-line) ones.
	sort.Slice(edits, func(i, j int) bool { return edits[i].line > edits[j].line })
	changed := false
	for _, e := range edits {
		if e.replace {
			if idx := e.line - 1; lines[idx] != e.text {
				lines[idx] = e.text
				changed = true
			}
		} else {
			idx := e.line // 0-based position == insert immediately after the doc's last line
			lines = append(lines[:idx], append([]string{e.text}, lines[idx:]...)...)
			changed = true
		}
	}
	if !changed {
		return src, false, nil
	}
	return []byte(strings.Join(lines, "\n")), true, nil
}

// runProvenance injects (write=true) or audits (write=false) provenance lines
// across litd/api. In audit mode any file that would change is reported and the
// process exits nonzero — the staleness gate. Fatal on any I/O/parse error.
func runProvenance(write bool) {
	cs, sigs, sources := buildClassifiedUniverse()
	m, _ := BuildManifest(cs, sigs, sources)
	prov := BuildProvenance(m, apiPackageMapping)

	var stale []string
	updated := 0
	err := filepath.WalkDir(apiPackageDir, func(p string, d fs.DirEntry, err error) error {
		if err != nil {
			return err
		}
		if d.IsDir() || !strings.HasSuffix(p, ".go") || strings.HasSuffix(p, "_test.go") {
			return nil
		}
		srcBytes, err := os.ReadFile(p)
		if err != nil {
			return err
		}
		out, changed, err := applyProvenanceToFile(p, srcBytes, prov)
		if err != nil {
			return err
		}
		if !changed {
			return nil
		}
		if write {
			if err := os.WriteFile(p, out, 0o644); err != nil {
				return err
			}
			updated++
			fmt.Fprintln(os.Stderr, "provenance: updated", p)
		} else {
			stale = append(stale, p)
		}
		return nil
	})
	if err != nil {
		fatal(err)
	}
	if !write {
		if len(stale) > 0 {
			fmt.Fprintf(os.Stderr, "PROVENANCE STALE — %d file(s) drift from the manifest (run jassgen -provenance and commit):\n", len(stale))
			for _, s := range stale {
				fmt.Fprintln(os.Stderr, "  -", s)
			}
			os.Exit(1)
		}
		fmt.Fprintf(os.Stderr, "provenance: GREEN (%d symbols mapped; committed lines match the manifest)\n", len(prov))
		return
	}
	fmt.Fprintf(os.Stderr, "provenance: %d symbols mapped, %d file(s) updated\n", len(prov), updated)
}

// apiPackageMapping is the manifest goMapping.Package value for the public api
// surface (the dir is apiPackageDir).
const apiPackageMapping = "litd/api"

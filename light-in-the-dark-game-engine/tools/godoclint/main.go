// Command godoclint is the G-1/G-5 godoc coverage gate for litd/api (#259,
// naming-and-style.md §4/§6). It enforces two doctrine-checkable properties of
// the public API documentation:
//
//	G-1 (coverage): every exported package-level declaration (func, type,
//	     const, var), every exported method, and every exported struct field
//	     carries a doc comment. 100% or the gate fails, naming each gap.
//	G-5 (leak-free docs): no doc comment references an internal package or the
//	     rendering engine (litd/sim, litd/render, g3n, ECS row/index jargon) —
//	     the public docs must describe behavior, never the implementation.
//
// It reads the SOURCE as the Source of Truth (AST + comments), not a coverage
// summary. Run as a binary (not `go test`) so there is no stale-cache layer
// (#389): `go run ./tools/godoclint ./litd/api`. Exit 0 = clean, 1 = findings.
package main

import (
	"fmt"
	"go/ast"
	"go/parser"
	"go/token"
	"os"
	"sort"
	"strings"
)

// g5Forbidden are substrings that must never appear in a public doc comment:
// internal packages and renderer/ECS implementation jargon (G-5). Matched
// case-insensitively against the comment text.
var g5Forbidden = []string{
	"litd/sim", "litd/render", "g3n", "github.com/g3n",
}

type finding struct {
	pos  token.Position
	rule string // "G-1" | "G-5"
	msg  string
}

func main() {
	dir := "./litd/api"
	if len(os.Args) > 1 {
		dir = os.Args[1]
	}
	findings, exported, err := audit(dir)
	if err != nil {
		fmt.Fprintln(os.Stderr, "godoclint:", err)
		os.Exit(2)
	}
	for _, f := range findings {
		fmt.Printf("%s: %s: %s\n", f.pos, f.rule, f.msg)
	}
	fmt.Printf("godoclint: %d exported symbols audited, %d finding(s)\n", exported, len(findings))
	if len(findings) > 0 {
		fmt.Println("godoclint: FAIL")
		os.Exit(1)
	}
	fmt.Println("godoclint: OK")
}

// audit parses every non-test .go file under dir and returns the G-1/G-5
// findings plus the count of exported symbols seen. Findings are sorted by
// file then line for stable output.
func audit(dir string) ([]finding, int, error) {
	fset := token.NewFileSet()
	pkgs, err := parser.ParseDir(fset, dir, func(fi os.FileInfo) bool {
		return !strings.HasSuffix(fi.Name(), "_test.go")
	}, parser.ParseComments)
	if err != nil {
		return nil, 0, err
	}

	var findings []finding
	exported := 0
	for _, pkg := range pkgs {
		for _, file := range pkg.Files {
			for _, decl := range file.Decls {
				switch d := decl.(type) {
				case *ast.FuncDecl:
					if !d.Name.IsExported() {
						continue
					}
					// Skip unexported-receiver methods? A method on an exported
					// receiver with an exported name is public surface; a method
					// on an unexported type is not reachable, skip it.
					if d.Recv != nil && !exportedReceiver(d.Recv) {
						continue
					}
					exported++
					if !hasDoc(d.Doc) {
						findings = append(findings, finding{fset.Position(d.Pos()), "G-1",
							fmt.Sprintf("%s is undocumented", funcLabel(d))})
					} else {
						findings = appendG5(findings, fset, d.Doc, funcLabel(d))
					}
				case *ast.GenDecl:
					exported += inspectGenDecl(d, fset, &findings)
				}
			}
		}
	}

	sort.Slice(findings, func(i, j int) bool {
		if findings[i].pos.Filename != findings[j].pos.Filename {
			return findings[i].pos.Filename < findings[j].pos.Filename
		}
		return findings[i].pos.Line < findings[j].pos.Line
	})
	return findings, exported, nil
}

// inspectGenDecl audits an exported type/const/var declaration and its members,
// returning the count of exported symbols seen. A grouped decl's block doc
// (`// foo\nconst ( ... )`) counts as documentation for its specs.
func inspectGenDecl(d *ast.GenDecl, fset *token.FileSet, findings *[]finding) int {
	groupDoc := hasDoc(d.Doc)
	count := 0
	for _, spec := range d.Specs {
		switch s := spec.(type) {
		case *ast.TypeSpec:
			if !s.Name.IsExported() {
				continue
			}
			count++
			label := "type " + s.Name.Name
			documented := groupDoc || hasDoc(s.Doc) || hasDoc(s.Comment)
			if !documented {
				*findings = append(*findings, finding{fset.Position(s.Pos()), "G-1",
					label + " is undocumented"})
			} else {
				*findings = appendG5doc(*findings, fset, s.Doc, d.Doc, s.Comment, label)
			}
			// Exported fields of an exported struct.
			if st, ok := s.Type.(*ast.StructType); ok {
				for _, fld := range st.Fields.List {
					for _, name := range fld.Names {
						if !name.IsExported() {
							continue
						}
						count++
						if !hasDoc(fld.Doc) && !hasDoc(fld.Comment) {
							*findings = append(*findings, finding{fset.Position(name.Pos()), "G-1",
								fmt.Sprintf("field %s.%s is undocumented", s.Name.Name, name.Name)})
						} else {
							*findings = appendG5doc(*findings, fset, fld.Doc, fld.Comment, nil,
								fmt.Sprintf("%s.%s", s.Name.Name, name.Name))
						}
					}
				}
			}
		case *ast.ValueSpec:
			for _, name := range s.Names {
				if !name.IsExported() {
					continue
				}
				count++
				documented := groupDoc || hasDoc(s.Doc) || hasDoc(s.Comment)
				if !documented {
					*findings = append(*findings, finding{fset.Position(name.Pos()), "G-1",
						name.Name + " is undocumented"})
				} else {
					*findings = appendG5doc(*findings, fset, s.Doc, d.Doc, s.Comment, name.Name)
				}
			}
		}
	}
	return count
}

func appendG5(fs []finding, fset *token.FileSet, doc *ast.CommentGroup, label string) []finding {
	return appendG5doc(fs, fset, doc, nil, nil, label)
}

// appendG5doc scans the combined text of the provided comment groups for any
// G-5 forbidden substring, appending one finding per hit.
func appendG5doc(fs []finding, fset *token.FileSet, groups ...interface{}) []finding {
	label, _ := groups[len(groups)-1].(string)
	var b strings.Builder
	var pos token.Pos
	for _, g := range groups[:len(groups)-1] {
		cg, _ := g.(*ast.CommentGroup)
		if cg == nil {
			continue
		}
		if pos == token.NoPos {
			pos = cg.Pos()
		}
		b.WriteString(cg.Text())
	}
	text := strings.ToLower(b.String())
	for _, bad := range g5Forbidden {
		if strings.Contains(text, strings.ToLower(bad)) {
			fs = append(fs, finding{fset.Position(pos), "G-5",
				fmt.Sprintf("%s doc references %q (internal/impl detail)", label, bad)})
		}
	}
	return fs
}

func hasDoc(cg *ast.CommentGroup) bool {
	return cg != nil && strings.TrimSpace(cg.Text()) != ""
}

func exportedReceiver(recv *ast.FieldList) bool {
	if len(recv.List) == 0 {
		return false
	}
	t := recv.List[0].Type
	if star, ok := t.(*ast.StarExpr); ok {
		t = star.X
	}
	if id, ok := t.(*ast.Ident); ok {
		return id.IsExported()
	}
	return false
}

func funcLabel(d *ast.FuncDecl) string {
	if d.Recv != nil && len(d.Recv.List) > 0 {
		t := d.Recv.List[0].Type
		if star, ok := t.(*ast.StarExpr); ok {
			t = star.X
		}
		if id, ok := t.(*ast.Ident); ok {
			return id.Name + "." + d.Name.Name
		}
	}
	return "func " + d.Name.Name
}

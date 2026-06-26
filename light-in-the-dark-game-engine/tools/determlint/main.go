// determlint enforces the determinism hazard bans (determinism.md §2.3
// hazards 2–5, §7 enforcement table) over the gameplay packages:
//
//	range over map · time.Now/Since/Until · go statements · select ·
//	math.* imports (math/bits allowed) · math/rand · crypto/rand ·
//	float32/float64 declarations · raw arithmetic on fixed.F64 outside
//	litd/fixed (constant expressions exempt)
//
// Scope (grows with M3): litd/sim/..., litd/fixed, litd/prng,
// litd/statehash. Test files are not loaded (tests legitimately use
// math/big and math/rand as cross-check references). Any finding is a
// CI failure.
//
// Usage: go run ./tools/determlint ./litd/...
package main

import (
	"flag"
	"fmt"
	"go/ast"
	"go/token"
	"go/types"
	"os"
	"strings"

	"golang.org/x/tools/go/packages"
)

const modulePath = "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine"

const fixedPkgPath = modulePath + "/litd/fixed"

// scopedPrefixes is the in-repo scope config (issue #117): packages
// the determinism bans apply to. Exact path or path + "/" prefix.
var scopedPrefixes = []string{
	modulePath + "/litd/sim", // and litd/sim/...
	fixedPkgPath,             // exact (litd/fixed/gen is a code generator, exempt)
	modulePath + "/litd/prng",
	modulePath + "/litd/statehash",
}

func inScope(pkgPath string) bool {
	for _, p := range scopedPrefixes {
		if pkgPath == p || strings.HasPrefix(pkgPath, p+"/") {
			if pkgPath == fixedPkgPath+"/gen" {
				return false
			}
			return true
		}
	}
	return false
}

func main() {
	scopeAll := flag.Bool("scope-all", false, "treat every loaded package as scoped (fixture/testing use)")
	flag.Parse()
	if flag.NArg() == 0 {
		fmt.Fprintln(os.Stderr, "usage: determlint [-scope-all] <package patterns>")
		os.Exit(2)
	}
	findings, audited, err := run(flag.Args(), *scopeAll)
	if err != nil {
		fmt.Fprintf(os.Stderr, "determlint: %v\n", err)
		os.Exit(2)
	}
	for _, f := range findings {
		fmt.Println(f)
	}
	fmt.Printf("determlint: %d packages audited, %d findings\n", audited, len(findings))
	if audited == 0 {
		fmt.Fprintln(os.Stderr, "determlint: FAIL — zero scoped packages matched (gate must never be vacuous)")
		os.Exit(1)
	}
	if len(findings) > 0 {
		os.Exit(1)
	}
}

func run(patterns []string, scopeAll bool) ([]string, int, error) {
	cfg := &packages.Config{
		Mode: packages.NeedName | packages.NeedFiles | packages.NeedCompiledGoFiles |
			packages.NeedImports | packages.NeedTypes | packages.NeedSyntax | packages.NeedTypesInfo,
		Tests: false, // test files exempt: they use math/big, math/rand as references
	}
	pkgs, err := packages.Load(cfg, patterns...)
	if err != nil {
		return nil, 0, err
	}
	if packages.PrintErrors(pkgs) > 0 {
		return nil, 0, fmt.Errorf("packages failed to load")
	}
	var findings []string
	audited := 0
	for _, pkg := range pkgs {
		if !scopeAll && !inScope(pkg.PkgPath) {
			continue
		}
		audited++
		c := &checker{pkg: pkg, fset: pkg.Fset}
		for _, file := range pkg.Syntax {
			c.checkImports(file)
			ast.Inspect(file, c.inspect)
		}
		findings = append(findings, c.findings...)
	}
	return findings, audited, nil
}

type checker struct {
	pkg      *packages.Package
	fset     *token.FileSet
	findings []string
}

func (c *checker) report(pos token.Pos, format string, args ...any) {
	c.findings = append(c.findings,
		fmt.Sprintf("%s: %s", c.fset.Position(pos), fmt.Sprintf(format, args...)))
}

func (c *checker) checkImports(file *ast.File) {
	for _, imp := range file.Imports {
		path := strings.Trim(imp.Path.Value, `"`)
		switch {
		case path == "math/bits":
			// the one allowed math package: integer-exact
		case path == "math" || strings.HasPrefix(path, "math/"):
			c.report(imp.Pos(), "import %q banned in gameplay code (only math/bits is deterministic-safe)", path)
		case path == "crypto/rand":
			c.report(imp.Pos(), "import \"crypto/rand\" banned: all gameplay randomness flows through litd/prng (R-SIM-2)")
		case path == "time":
			// the import alone is not the hazard; Now/Since/Until uses are flagged below
		}
	}
}

func (c *checker) inspect(n ast.Node) bool {
	info := c.pkg.TypesInfo
	switch n := n.(type) {
	case *ast.RangeStmt:
		if _, isMap := info.TypeOf(n.X).Underlying().(*types.Map); isMap {
			c.report(n.Pos(), "range over map: iteration order is nondeterministic (hazard §2.3-2); iterate a sorted slice instead")
		}
	case *ast.GoStmt:
		c.report(n.Pos(), "go statement: gameplay code runs single-threaded on the scheduler (R-EXEC-1)")
	case *ast.SelectStmt:
		c.report(n.Pos(), "select statement: channel scheduling is nondeterministic (R-EXEC-1)")
	case *ast.CallExpr:
		if sel, ok := n.Fun.(*ast.SelectorExpr); ok {
			if obj, ok := info.Uses[sel.Sel].(*types.Func); ok && obj.Pkg() != nil && obj.Pkg().Path() == "time" {
				switch obj.Name() {
				case "Now", "Since", "Until":
					c.report(n.Pos(), "time.%s: wall-clock read inside gameplay code (hazard §2.3-3); ticks are the only clock", obj.Name())
				}
			}
		}
	case *ast.ValueSpec:
		for _, name := range n.Names {
			c.checkFloatType(name.Pos(), info.TypeOf(name))
		}
	case *ast.Field:
		if len(n.Names) > 0 {
			c.checkFloatType(n.Pos(), info.TypeOf(n.Names[0]))
		} else if n.Type != nil {
			c.checkFloatType(n.Pos(), info.TypeOf(n.Type))
		}
	case *ast.AssignStmt:
		if n.Tok == token.DEFINE {
			for _, lhs := range n.Lhs {
				if id, ok := lhs.(*ast.Ident); ok && id.Name != "_" {
					c.checkFloatType(id.Pos(), info.TypeOf(id))
				}
			}
		}
		switch n.Tok {
		case token.ADD_ASSIGN, token.SUB_ASSIGN, token.MUL_ASSIGN, token.QUO_ASSIGN, token.REM_ASSIGN:
			for _, lhs := range n.Lhs {
				if c.isF64(info.TypeOf(lhs)) && c.pkg.PkgPath != fixedPkgPath {
					c.report(n.Pos(), "raw %s on fixed.F64 outside litd/fixed: use the F64 methods (Add/Sub/Mul/Div) so semantics stay in one place", n.Tok)
				}
			}
		}
	case *ast.IncDecStmt:
		if c.isF64(info.TypeOf(n.X)) && c.pkg.PkgPath != fixedPkgPath {
			c.report(n.Pos(), "raw %s on fixed.F64 outside litd/fixed", n.Tok)
		}
	case *ast.BinaryExpr:
		switch n.Op {
		case token.ADD, token.SUB, token.MUL, token.QUO, token.REM:
			if c.pkg.PkgPath == fixedPkgPath {
				return true
			}
			if tv, ok := info.Types[n]; ok && tv.Value != nil {
				return true // constant expression: folded at compile time, deterministic
			}
			if c.isF64(info.TypeOf(n.X)) || c.isF64(info.TypeOf(n.Y)) {
				c.report(n.Pos(), "raw %s on fixed.F64 outside litd/fixed: use the F64 methods (Add/Sub/Mul/Div)", n.Op)
			}
		}
	}
	return true
}

func (c *checker) isF64(t types.Type) bool {
	named, ok := t.(*types.Named)
	if !ok {
		return false
	}
	obj := named.Obj()
	return obj.Pkg() != nil && obj.Pkg().Path() == fixedPkgPath && obj.Name() == "F64"
}

// checkFloatType reports if t contains float32/float64 anywhere in its
// structure (declarations only — hazard §2.3-1 keeps floats out of
// gameplay state entirely).
func (c *checker) checkFloatType(pos token.Pos, t types.Type) {
	if t == nil {
		return
	}
	if containsFloat(t, map[types.Type]bool{}) {
		c.report(pos, "float in gameplay declaration (%s): floats are banned from sim state and math (hazard §2.3-1)", t)
	}
}

func containsFloat(t types.Type, seen map[types.Type]bool) bool {
	if seen[t] {
		return false
	}
	seen[t] = true
	switch t := t.(type) {
	case *types.Basic:
		k := t.Kind()
		return k == types.Float32 || k == types.Float64 ||
			k == types.Complex64 || k == types.Complex128 ||
			k == types.UntypedFloat || k == types.UntypedComplex
	case *types.Named:
		return containsFloat(t.Underlying(), seen)
	case *types.Pointer:
		return containsFloat(t.Elem(), seen)
	case *types.Slice:
		return containsFloat(t.Elem(), seen)
	case *types.Array:
		return containsFloat(t.Elem(), seen)
	case *types.Map:
		return containsFloat(t.Key(), seen) || containsFloat(t.Elem(), seen)
	case *types.Struct:
		for i := 0; i < t.NumFields(); i++ {
			if containsFloat(t.Field(i).Type(), seen) {
				return true
			}
		}
	}
	return false
}

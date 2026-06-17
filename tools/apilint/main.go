// apilint mechanically enforces the public-API shape gates on litd/api
// (public-api-design.md §3, milestones.md §8 exit criterion 2). It is the
// machine half of the R-API-1…6 contract that the design doc states in prose:
//
//	G2.2/R-API-6  no G3N (or GL/window) type in any exported signature
//	G2.3          ≤5 positional params on an exported func/method
//	              (a trailing variadic options param does NOT count)
//	G2.4/R-API-1  no package-level func with a noun-handle parameter, except
//	              option constructors (which return an …Option / func type)
//	R-API-4       no exported Trigger / BoolExpr / Group / Location identifier
//	R-API-5       no error return on a gameplay verb (any noun method; any
//	              free func not in the setup allowlist NewGame/LoadMap)
//	R-API-5       every noun handle exposes Valid() bool
//	§2            a noun handle has zero exported fields
//	G-1           every exported func/method/type/const/var is documented
//	              (naming-and-style §4/§6; the godoc-coverage gate, #259)
//
// A "noun handle" is detected structurally: an exported struct that has a
// *Game field and zero exported fields. Event is exempt — it is a value
// payload that happens to carry a *Game to resolve its context nouns
// (public-api-design.md §2). This keeps the check from going stale as new
// handles land: a new {id; *Game} struct is audited automatically.
//
// Usage: go run ./tools/apilint <package patterns>   (default: ./litd/api)
// Any finding is a CI failure. The check fails if zero packages match
// (never vacuously green).
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

// maxPositionalParams is the G2.3 ceiling. A function with more than this
// many non-variadic params must move the long tail into functional options.
const maxPositionalParams = 5

// setupAllowlist names the construction-time funcs permitted to return error
// (R-API-5: "setup returns errors; gameplay verbs never do"). These are free
// functions, never methods.
var setupAllowlist = map[string]bool{
	"NewGame": true,
	"LoadMap": true,
}

// methodErrAllowlist names methods permitted to return error despite R-API-5's
// "gameplay verbs never error" rule. Two kinds qualify, neither a gameplay verb:
//   - persistence-boundary IO: Storage.Save/Load read/write an external byte
//     stream and MUST fail closed on bad magic / version / truncation
//     (doctrine §9), expressible only as an error.
//   - SETUP methods: construction-time verbs that validate their inputs before a
//     match runs. Game.DefineUnits (#387) seeds unit definitions (the method
//     analog of the NewGame/LoadMap free-func setup allowlist) and returns error
//     on invalid/conflicting defs — a load-time failure, never mid-match.
//
// Keyed "Type.Method".
var methodErrAllowlist = map[string]bool{
	"Storage.Save":     true,
	"Storage.Load":     true,
	"Game.DefineUnits": true,
}

// forbiddenIdents are exported identifiers the API must never expose — the
// trigger-zoo types events replaced (R-API-4) and the heap location type
// values replaced (R-API-2).
var forbiddenIdents = map[string]bool{
	"Trigger":  true,
	"BoolExpr": true,
	"Group":    true,
	"Location": true,
}

// valueTypeExemptions are exported types that carry a *Game field but are
// value payloads, not noun handles, so they are exempt from the handle rules
// (Valid()/zero-exported-fields). Only Event qualifies today.
var valueTypeExemptions = map[string]bool{
	"Event": true,
}

// bannedSignaturePkgs: an exported signature mentioning a type from any of
// these import paths violates R-API-6 (zero foreign-engine types in the
// public surface).
var bannedSignaturePkgs = []string{
	"github.com/g3n/engine",
	"github.com/go-gl/glfw",
	"github.com/go-gl/gl",
	"github.com/go-gl/mathgl",
}

func main() {
	flag.Parse()
	patterns := flag.Args()
	if len(patterns) == 0 {
		patterns = []string{"./litd/api"}
	}
	findings, audited, err := run(patterns)
	if err != nil {
		fmt.Fprintf(os.Stderr, "apilint: %v\n", err)
		os.Exit(2)
	}
	for _, f := range findings {
		fmt.Println(f)
	}
	fmt.Printf("apilint: %d package(s) audited, %d finding(s)\n", audited, len(findings))
	if audited == 0 {
		fmt.Fprintln(os.Stderr, "apilint: FAIL — zero packages matched (gate must never be vacuous)")
		os.Exit(1)
	}
	if len(findings) > 0 {
		fmt.Printf("apilint: FAIL — %d violation(s)\n", len(findings))
		os.Exit(1)
	}
	fmt.Println("apilint: OK")
}

func run(patterns []string) ([]string, int, error) {
	cfg := &packages.Config{
		Mode: packages.NeedName | packages.NeedFiles | packages.NeedCompiledGoFiles |
			packages.NeedImports | packages.NeedTypes | packages.NeedSyntax | packages.NeedTypesInfo,
		Tests: false, // test files are not part of the public surface
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
		audited++
		c := &checker{pkg: pkg, fset: pkg.Fset}
		c.check()
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

func (c *checker) check() {
	for _, file := range c.pkg.Syntax {
		for _, decl := range file.Decls {
			switch d := decl.(type) {
			case *ast.FuncDecl:
				c.checkFunc(d)
				c.checkDocFunc(d)
			case *ast.GenDecl:
				c.checkGenDecl(d)
				c.checkDocGen(d)
			}
		}
	}
}

// checkFunc applies the func/method rules: param count, free-func handle
// params, error returns, and foreign types in the signature.
func (c *checker) checkFunc(d *ast.FuncDecl) {
	if !d.Name.IsExported() {
		return
	}
	isMethod := d.Recv != nil && len(d.Recv.List) > 0

	// Only methods on EXPORTED receivers, and exported free funcs, are part of
	// the public surface. A method on an unexported type is internal.
	if isMethod && !c.receiverIsExported(d) {
		return
	}

	sig, _ := c.pkg.TypesInfo.Defs[d.Name].Type().(*types.Signature)
	if sig == nil {
		return
	}

	c.checkParamCount(d, sig)
	c.checkErrorReturn(d, sig, isMethod)
	c.checkForeignTypesInSig(d.Name.Pos(), c.funcName(d), sig)
	if !isMethod {
		c.checkFreeFuncHandleParam(d, sig)
	}
}

// checkParamCount enforces G2.3: at most maxPositionalParams non-variadic
// params. A trailing variadic (the functional-options slot) is exempt.
func (c *checker) checkParamCount(d *ast.FuncDecl, sig *types.Signature) {
	params := sig.Params()
	n := params.Len()
	if sig.Variadic() && n > 0 {
		n-- // the variadic options slot does not count toward the ceiling
	}
	if n > maxPositionalParams {
		c.report(d.Name.Pos(),
			"G2.3: %s has %d positional params (>%d): move the long tail into functional options (R-API-3)",
			c.funcName(d), n, maxPositionalParams)
	}
}

// checkErrorReturn enforces R-API-5: gameplay verbs never return error. Any
// method may not; a free func may only if it is in the setup allowlist.
func (c *checker) checkErrorReturn(d *ast.FuncDecl, sig *types.Signature, isMethod bool) {
	if !sigReturnsError(sig) {
		return
	}
	if !isMethod && setupAllowlist[d.Name.Name] {
		return // allowlisted setup constructor (NewGame/LoadMap)
	}
	if isMethod && methodErrAllowlist[c.funcName(d)] {
		return // allowlisted persistence-boundary IO method (Storage.Save/Load)
	}
	c.report(d.Name.Pos(),
		"R-API-5: %s returns error — gameplay verbs are no-ops on invalid handles, never error; only setup (NewGame/LoadMap) may return error",
		c.funcName(d))
}

// checkDocFunc enforces G-1 (naming-and-style §4/§6): every exported func and
// method on an exported receiver carries a doc comment. This is the godoc
// coverage gate (#259) — 100% of the exported surface documented.
func (c *checker) checkDocFunc(d *ast.FuncDecl) {
	if !d.Name.IsExported() {
		return
	}
	if d.Recv != nil && len(d.Recv.List) > 0 && !c.receiverIsExported(d) {
		return // method on an unexported type is internal
	}
	if !hasDoc(d.Doc) {
		c.report(d.Name.Pos(), "G-1: exported %s has no doc comment (godoc coverage must be 100%%)", c.funcName(d))
	}
}

// checkDocGen enforces G-1 for exported types, consts, and vars. A type needs a
// lead doc comment (its own or the block's). A const/var is documented by its
// own doc, its block's lead comment, or a trailing line comment — the latter is
// the conventional form for enum-value blocks (e.g. AllianceFlags bits).
func (c *checker) checkDocGen(d *ast.GenDecl) {
	if d.Tok != token.TYPE && d.Tok != token.CONST && d.Tok != token.VAR {
		return
	}
	blockDoc := hasDoc(d.Doc)
	for _, spec := range d.Specs {
		switch s := spec.(type) {
		case *ast.TypeSpec:
			if !s.Name.IsExported() {
				continue
			}
			if !hasDoc(s.Doc) && !blockDoc {
				c.report(s.Name.Pos(), "G-1: exported type %q has no doc comment", s.Name.Name)
			}
		case *ast.ValueSpec:
			own := hasDoc(s.Doc) || hasDoc(s.Comment)
			for _, n := range s.Names {
				if !n.IsExported() {
					continue
				}
				if !own && !blockDoc {
					c.report(n.Pos(), "G-1: exported %s %q has no doc comment", d.Tok.String(), n.Name)
				}
			}
		}
	}
}

// hasDoc reports whether a comment group carries non-empty text.
func hasDoc(g *ast.CommentGroup) bool { return g != nil && strings.TrimSpace(g.Text()) != "" }

// checkFreeFuncHandleParam enforces G2.4/R-API-1: a package-level func may not
// take a noun-handle param (it should be a method on that noun) unless it is
// an option constructor, identified by returning an option type.
func (c *checker) checkFreeFuncHandleParam(d *ast.FuncDecl, sig *types.Signature) {
	params := sig.Params()
	for i := 0; i < params.Len(); i++ {
		if c.isNounHandle(params.At(i).Type()) {
			if c.isValueConstructor(sig) {
				return // value/option constructor (e.g. ForPlayer(p Player) EventOption, TargetUnit(u Unit) OrderTarget)
			}
			c.report(d.Name.Pos(),
				"G2.4/R-API-1: package-level func %s takes a noun-handle param %s — make it a method on that noun (only value/option constructors may take a handle)",
				c.funcName(d), types.TypeString(params.At(i).Type(), relativeTo(c.pkg)))
			return
		}
	}
}

// checkForeignTypesInSig enforces R-API-6: no G3N/GL type anywhere in an
// exported signature (params or results).
func (c *checker) checkForeignTypesInSig(pos token.Pos, name string, sig *types.Signature) {
	report := func(t types.Type) {
		if path := foreignPkgPath(t); path != "" {
			c.report(pos, "R-API-6: signature of %s mentions foreign engine type from %q (zero G3N types in the public surface)", name, path)
		}
	}
	for i := 0; i < sig.Params().Len(); i++ {
		walkTypes(sig.Params().At(i).Type(), report)
	}
	for i := 0; i < sig.Results().Len(); i++ {
		walkTypes(sig.Results().At(i).Type(), report)
	}
}

// checkGenDecl applies the type-declaration rules: forbidden identifiers,
// the noun-handle requirements (Valid(), zero exported fields), and foreign
// types in exported struct fields.
func (c *checker) checkGenDecl(d *ast.GenDecl) {
	if d.Tok != token.TYPE {
		return
	}
	for _, spec := range d.Specs {
		ts, ok := spec.(*ast.TypeSpec)
		if !ok || !ts.Name.IsExported() {
			continue
		}
		name := ts.Name.Name
		if forbiddenIdents[name] {
			c.report(ts.Name.Pos(),
				"R-API-2/R-API-4: exported type %q is forbidden — the trigger zoo and heap location were replaced by events and value types", name)
		}
		named, ok := c.pkg.TypesInfo.Defs[ts.Name].Type().(*types.Named)
		if !ok {
			continue
		}
		st, ok := named.Underlying().(*types.Struct)
		if !ok {
			continue
		}
		c.checkStructForeignFields(ts.Name.Pos(), name, st)
		if c.structHasGameField(st) && !valueTypeExemptions[name] {
			c.checkHandleRules(ts.Name.Pos(), name, named, st)
		}
	}
}

// checkHandleRules enforces the noun-handle invariants: Valid() bool present,
// zero exported fields (public-api-design.md §2, R-API-5).
func (c *checker) checkHandleRules(pos token.Pos, name string, named *types.Named, st *types.Struct) {
	if !hasValidMethod(named) {
		c.report(pos, "R-API-5: noun handle %q has no Valid() bool method (every handle's validity must be queryable)", name)
	}
	for i := 0; i < st.NumFields(); i++ {
		f := st.Field(i)
		if f.Exported() {
			c.report(pos, "§2: noun handle %q has exported field %q — handles carry only unexported state", name, f.Name())
		}
	}
}

func (c *checker) checkStructForeignFields(pos token.Pos, typeName string, st *types.Struct) {
	for i := 0; i < st.NumFields(); i++ {
		f := st.Field(i)
		if !f.Exported() {
			continue
		}
		if path := firstForeignPath(f.Type()); path != "" {
			c.report(pos, "R-API-6: %s exported field %q exposes foreign engine type from %q", typeName, f.Name(), path)
		}
	}
}

// -- helpers ----------------------------------------------------------------

func (c *checker) funcName(d *ast.FuncDecl) string {
	if d.Recv != nil && len(d.Recv.List) > 0 {
		return c.receiverTypeName(d) + "." + d.Name.Name
	}
	return d.Name.Name
}

func (c *checker) receiverIsExported(d *ast.FuncDecl) bool {
	name := c.receiverTypeName(d)
	return name != "" && ast.IsExported(name)
}

func (c *checker) receiverTypeName(d *ast.FuncDecl) string {
	if d.Recv == nil || len(d.Recv.List) == 0 {
		return ""
	}
	t := d.Recv.List[0].Type
	if star, ok := t.(*ast.StarExpr); ok {
		t = star.X
	}
	if id, ok := t.(*ast.Ident); ok {
		return id.Name
	}
	return ""
}

// isNounHandle reports whether t is one of this package's noun-handle types
// (exported struct with a *Game field and zero exported fields, Event
// excepted). Pointers and the type itself both resolve.
func (c *checker) isNounHandle(t types.Type) bool {
	if p, ok := t.(*types.Pointer); ok {
		t = p.Elem()
	}
	named, ok := t.(*types.Named)
	if !ok || named.Obj().Pkg() == nil || named.Obj().Pkg().Path() != c.pkg.PkgPath {
		return false
	}
	name := named.Obj().Name()
	if !ast.IsExported(name) || valueTypeExemptions[name] {
		return false
	}
	st, ok := named.Underlying().(*types.Struct)
	if !ok || !c.structHasGameField(st) {
		return false
	}
	for i := 0; i < st.NumFields(); i++ {
		if st.Field(i).Exported() {
			return false // a struct with exported fields is a value type, not a handle
		}
	}
	return true
}

// structHasGameField reports whether st has a field of type *Game (the
// in-package Game type).
func (c *checker) structHasGameField(st *types.Struct) bool {
	for i := 0; i < st.NumFields(); i++ {
		p, ok := st.Field(i).Type().(*types.Pointer)
		if !ok {
			continue
		}
		named, ok := p.Elem().(*types.Named)
		if !ok {
			continue
		}
		obj := named.Obj()
		if obj.Pkg() != nil && obj.Pkg().Path() == c.pkg.PkgPath && obj.Name() == "Game" {
			return true
		}
	}
	return false
}

// isValueConstructor reports whether the signature is a pure constructor: it
// has at least one result and *every* result is either an in-package value
// type (a named type from this package that is not a noun handle — e.g.
// OrderTarget, EventOption) or a function type (the functional-option shape).
//
// R-API-1 only bans *mutating* free funcs that take a noun handle; a func that
// merely packages a handle into a value to hand back (TargetUnit(u) OrderTarget,
// ForPlayer(p) EventOption) is the sanctioned escape hatch. A void func (no
// results) or one returning a primitive/foreign type is treated as a mutator or
// a misplaced getter and stays flagged — it belongs on the noun as a method.
func (c *checker) isValueConstructor(sig *types.Signature) bool {
	res := sig.Results()
	if res.Len() == 0 {
		return false // void → mutator, not a constructor
	}
	for i := 0; i < res.Len(); i++ {
		t := res.At(i).Type()
		if _, ok := t.Underlying().(*types.Signature); ok {
			continue // function type — functional-option shape
		}
		named, ok := t.(*types.Named)
		if !ok {
			return false // primitive/foreign result → not a constructor
		}
		obj := named.Obj()
		if obj.Pkg() == nil || obj.Pkg().Path() != c.pkg.PkgPath {
			return false // result type lives outside the public package
		}
		if c.isNounHandle(named) {
			return false // returning a handle is not what makes this a constructor
		}
	}
	return true
}

func relativeTo(pkg *packages.Package) types.Qualifier {
	return func(other *types.Package) string {
		if other.Path() == pkg.PkgPath {
			return ""
		}
		return other.Name()
	}
}

func sigReturnsError(sig *types.Signature) bool {
	res := sig.Results()
	for i := 0; i < res.Len(); i++ {
		if named, ok := res.At(i).Type().(*types.Named); ok {
			if named.Obj().Name() == "error" && named.Obj().Pkg() == nil {
				return true
			}
		}
	}
	return false
}

func hasValidMethod(named *types.Named) bool {
	for i := 0; i < named.NumMethods(); i++ {
		m := named.Method(i)
		if m.Name() != "Valid" {
			continue
		}
		sig := m.Type().(*types.Signature)
		if sig.Params().Len() == 0 && sig.Results().Len() == 1 {
			if b, ok := sig.Results().At(0).Type().(*types.Basic); ok && b.Kind() == types.Bool {
				return true
			}
		}
	}
	return false
}

// foreignPkgPath returns the import path of a named type if it belongs to a
// banned foreign-engine package, else "".
func foreignPkgPath(t types.Type) string {
	named, ok := t.(*types.Named)
	if !ok || named.Obj().Pkg() == nil {
		return ""
	}
	path := named.Obj().Pkg().Path()
	for _, b := range bannedSignaturePkgs {
		if path == b || strings.HasPrefix(path, b+"/") {
			return path
		}
	}
	return ""
}

// walkTypes invokes fn for every named type reachable through t's structure
// (pointers, slices, arrays, maps, channels, signatures, tuples).
func walkTypes(t types.Type, fn func(types.Type)) {
	walkTypesSeen(t, fn, map[types.Type]bool{})
}

func walkTypesSeen(t types.Type, fn func(types.Type), seen map[types.Type]bool) {
	if t == nil || seen[t] {
		return
	}
	seen[t] = true
	switch t := t.(type) {
	case *types.Named:
		fn(t)
		// Do not descend into a named type's underlying struct fields here:
		// the field types are part of that type's own decl, not this signature.
	case *types.Pointer:
		walkTypesSeen(t.Elem(), fn, seen)
	case *types.Slice:
		walkTypesSeen(t.Elem(), fn, seen)
	case *types.Array:
		walkTypesSeen(t.Elem(), fn, seen)
	case *types.Map:
		walkTypesSeen(t.Key(), fn, seen)
		walkTypesSeen(t.Elem(), fn, seen)
	case *types.Chan:
		walkTypesSeen(t.Elem(), fn, seen)
	case *types.Signature:
		for i := 0; i < t.Params().Len(); i++ {
			walkTypesSeen(t.Params().At(i).Type(), fn, seen)
		}
		for i := 0; i < t.Results().Len(); i++ {
			walkTypesSeen(t.Results().At(i).Type(), fn, seen)
		}
	}
}

// firstForeignPath returns the path of the first banned foreign type reachable
// through t, else "".
func firstForeignPath(t types.Type) string {
	var found string
	walkTypes(t, func(tt types.Type) {
		if found != "" {
			return
		}
		if p := foreignPkgPath(tt); p != "" {
			found = p
		}
	})
	return found
}

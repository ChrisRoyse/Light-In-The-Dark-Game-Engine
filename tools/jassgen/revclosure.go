package main

// revclosure.go implements the reverse-closure gate (#260): the manifest proves
// every JASS function has a disposition (forward closure); this proves the
// converse — every exported litd/api *verb* (func or method) traces back to a
// manifest goMapping (a deduplicated JASS port) or to an explicit rule in
// new-capabilities.txt (a LitD-original capability / structural plumbing). An
// export in neither set is unaccounted scope and fails the gate. Exported types
// and vars are intentionally out of scope — this gate governs callable behavior,
// not data shapes (see new-capabilities.txt header).

import (
	"bufio"
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

// ruleKind enumerates the new-capabilities.txt rule forms.
type ruleKind int

const (
	ruleExact ruleKind = iota
	ruleSuffix
	rulePrefixFunc
)

// ClosureRule is one parsed whitelist rule.
type ClosureRule struct {
	Kind ruleKind
	Arg  string // exact symbol | ".Method" suffix | func-name prefix
}

// matches reports whether export (an "<Type>.<Method>" or bare "<Func>" name)
// is covered by this rule.
func (r ClosureRule) matches(export string) bool {
	switch r.Kind {
	case ruleExact:
		return export == r.Arg
	case ruleSuffix:
		// r.Arg is ".Method"; match any "<Type>.<Method>".
		dot := strings.IndexByte(export, '.')
		return dot >= 0 && export[dot:] == r.Arg
	case rulePrefixFunc:
		// applies to bare package funcs only (no dot in the export name).
		return !strings.Contains(export, ".") && strings.HasPrefix(export, r.Arg)
	}
	return false
}

// ParseAPIExports walks a Go package dir and returns its exported verbs — every
// exported package-level func and every exported method on an exported type — as
// "<Type>.<Method>" (methods) or "<Func>" (package funcs). Test files and
// unexported receivers are skipped. Types and vars are deliberately omitted.
func ParseAPIExports(dir string) ([]string, error) {
	fset := gotoken.NewFileSet()
	set := map[string]bool{}
	err := filepath.WalkDir(dir, func(p string, d fs.DirEntry, err error) error {
		if err != nil {
			return err
		}
		if d.IsDir() || !strings.HasSuffix(p, ".go") || strings.HasSuffix(p, "_test.go") {
			return nil
		}
		f, perr := goparser.ParseFile(fset, p, nil, 0)
		if perr != nil {
			return fmt.Errorf("parse %s: %w", p, perr)
		}
		for _, decl := range f.Decls {
			fd, ok := decl.(*ast.FuncDecl)
			if !ok || !fd.Name.IsExported() {
				continue
			}
			if fd.Recv != nil && len(fd.Recv.List) > 0 {
				recv := receiverTypeName(fd.Recv.List[0].Type)
				if ast.IsExported(recv) {
					set[recv+"."+fd.Name.Name] = true
				}
				continue
			}
			set[fd.Name.Name] = true
		}
		return nil
	})
	if err != nil {
		return nil, err
	}
	out := make([]string, 0, len(set))
	for k := range set {
		out = append(out, k)
	}
	sort.Strings(out)
	return out, nil
}

// receiverTypeName (shared with emit_stubs.go) unwraps a method receiver to its
// base type name.

// LoadClosureRules parses new-capabilities.txt. A missing file is an error: the
// gate must not silently pass for want of its whitelist.
func LoadClosureRules(path string) ([]ClosureRule, error) {
	f, err := os.Open(path)
	if err != nil {
		return nil, fmt.Errorf("reverse-closure whitelist: %w", err)
	}
	defer f.Close()
	var rules []ClosureRule
	sc := bufio.NewScanner(f)
	for ln := 1; sc.Scan(); ln++ {
		line := strings.TrimSpace(sc.Text())
		if line == "" || strings.HasPrefix(line, "#") {
			continue
		}
		fields := strings.Fields(line)
		if len(fields) < 2 {
			return nil, fmt.Errorf("%s:%d: malformed rule %q", path, ln, line)
		}
		switch fields[0] {
		case "exact":
			rules = append(rules, ClosureRule{ruleExact, fields[1]})
		case "suffix":
			if !strings.HasPrefix(fields[1], ".") {
				return nil, fmt.Errorf("%s:%d: suffix rule must start with '.', got %q", path, ln, fields[1])
			}
			rules = append(rules, ClosureRule{ruleSuffix, fields[1]})
		case "prefix":
			// form: "prefix func <Pfx>"
			if len(fields) < 3 || fields[1] != "func" {
				return nil, fmt.Errorf("%s:%d: prefix rule must be 'prefix func <Pfx>', got %q", path, ln, line)
			}
			rules = append(rules, ClosureRule{rulePrefixFunc, fields[2]})
		default:
			return nil, fmt.Errorf("%s:%d: unknown rule kind %q", path, ln, fields[0])
		}
	}
	if err := sc.Err(); err != nil {
		return nil, err
	}
	return rules, nil
}

// ReverseClosure returns the sorted set of exported verbs that trace to neither
// a manifest goMapping nor a whitelist rule. An empty result means the gate is
// green. This is the pure core (no I/O) for direct testing.
func ReverseClosure(exports []string, manifestSyms map[string]bool, rules []ClosureRule) []string {
	var unaccounted []string
	for _, e := range exports {
		if manifestSyms[e] {
			continue
		}
		covered := false
		for _, r := range rules {
			if r.matches(e) {
				covered = true
				break
			}
		}
		if !covered {
			unaccounted = append(unaccounted, e)
		}
	}
	sort.Strings(unaccounted)
	return unaccounted
}

// manifestGoMappingSet collects the distinct goMapping symbols for a package.
func manifestGoMappingSet(m Manifest, pkg string) map[string]bool {
	out := map[string]bool{}
	for _, f := range m.Functions {
		if f.GoMapping != nil && f.GoMapping.Package == pkg {
			out[f.GoMapping.Symbol] = true
		}
	}
	return out
}

const (
	apiPackageDir       = "litd/api"
	newCapabilitiesPath = "tools/jassgen/new-capabilities.txt"
)

// runRevClosure executes the reverse-closure gate end to end and exits nonzero
// on any unaccounted export.
func runRevClosure() {
	cs, sigs, sources := buildClassifiedUniverse()
	m, _ := BuildManifest(cs, sigs, sources)
	syms := manifestGoMappingSet(m, "litd/api")

	exports, err := ParseAPIExports(apiPackageDir)
	if err != nil {
		fatal(err)
	}
	rules, err := LoadClosureRules(newCapabilitiesPath)
	if err != nil {
		fatal(err)
	}
	unaccounted := ReverseClosure(exports, syms, rules)

	fmt.Fprintf(os.Stderr, "reverse-closure: %d exported verbs, %d manifest mappings, %d whitelist rules\n",
		len(exports), len(syms), len(rules))
	if len(unaccounted) > 0 {
		fmt.Fprintf(os.Stderr, "REVERSE-CLOSURE FAIL — %d export(s) trace to neither a manifest goMapping nor new-capabilities.txt:\n", len(unaccounted))
		for _, u := range unaccounted {
			fmt.Fprintln(os.Stderr, "  -", u)
		}
		fmt.Fprintln(os.Stderr, "add a JASS mapping (overrides.toml) or a whitelist rule (new-capabilities.txt).")
		os.Exit(1)
	}
	fmt.Fprintln(os.Stderr, "reverse-closure: GREEN (every exported verb traces to a mapping or declared capability)")
}

package main

import (
	"fmt"
	"sort"
)

// merge.go joins parsed .j native declarations with .d.ts function declarations
// by exact name, enriching each native's parameters with their TypeScript type
// and nullability and recording the handle subtype hierarchy. Every name that
// fails to join on either side becomes a Discrepancy — the known
// common.j/common.d.ts delta is explained entry-by-entry, never papered over
// (tooling.md §2.2 step 2).

// JassSig is the name + signature of a callable JASS symbol, abstracted over
// whether it came from a `native` declaration (.j common/commonai) or a
// `function` definition (.j blizzard). This lets one Merge join either side
// against .d.ts functions.
type JassSig struct {
	Name    string
	Params  []Param
	Returns string
}

// DeclsToSigs extracts native declarations (only) as signatures.
func DeclsToSigs(decls []Decl) []JassSig {
	var out []JassSig
	for _, d := range decls {
		if d.Kind == KindNative || d.Kind == KindConstantNative {
			out = append(out, JassSig{Name: d.Name, Params: d.Params, Returns: d.Returns})
		}
	}
	return out
}

// FuncsToSigs extracts blizzard.j function definitions as signatures.
func FuncsToSigs(funcs []Func) []JassSig {
	out := make([]JassSig, len(funcs))
	for i, f := range funcs {
		out[i] = JassSig{Name: f.Name, Params: f.Params, Returns: f.Returns}
	}
	return out
}

// MergedParam pairs a JASS parameter with its .d.ts enrichment.
type MergedParam struct {
	Name     string
	JassType string
	TSType   string
	Nullable bool
}

// MergedEntry is a native joined with its .d.ts function declaration.
type MergedEntry struct {
	Name        string
	Origin      string
	Params      []MergedParam
	JassReturns string
	TSReturns   string
}

// Discrepancy records a join failure or mismatch between .j and .d.ts.
type Discrepancy struct {
	Name   string
	Kind   string // "jass-only" | "dts-only" | "arity-mismatch"
	Detail string
}

// MergeResult is the full output of a merge pass.
type MergeResult struct {
	Entries       []MergedEntry
	Discrepancies []Discrepancy
	Hierarchy     map[string]string // handle subtype -> immediate base (from .d.ts)
	JassNatives   int               // count of .j-side symbols (natives or functions)
	DTSFunctions  int
}

// Merge joins origin-tagged JASS signatures with .d.ts function decls by exact
// name. .j symbols with no .d.ts partner → jass-only discrepancy; .d.ts
// functions with no .j partner → dts-only (e.g. common.ai AI-script functions);
// arity differences → arity-mismatch with both signatures printed.
func Merge(sigs []JassSig, origin string, dts []DTSDecl) MergeResult {
	res := MergeResult{Hierarchy: DTSHierarchy(dts)}

	dtsFns := map[string]DTSDecl{}
	for _, d := range dts {
		if d.Kind == "function" {
			dtsFns[d.Name] = d
			res.DTSFunctions++
		}
	}

	jassNames := map[string]bool{}
	for _, s := range sigs {
		res.JassNatives++
		jassNames[s.Name] = true

		dtsDecl, ok := dtsFns[s.Name]
		if !ok {
			res.Discrepancies = append(res.Discrepancies, Discrepancy{
				Name: s.Name, Kind: "jass-only",
				Detail: "present in .j, no matching .d.ts function",
			})
			continue
		}
		entry := MergedEntry{
			Name:        s.Name,
			Origin:      origin,
			JassReturns: s.Returns,
			TSReturns:   dtsDecl.Returns,
		}
		if len(s.Params) != len(dtsDecl.Params) {
			res.Discrepancies = append(res.Discrepancies, Discrepancy{
				Name: s.Name, Kind: "arity-mismatch",
				Detail: fmt.Sprintf(".j arity=%d %v  /  .d.ts arity=%d %v",
					len(s.Params), jassParamTypes(s.Params),
					len(dtsDecl.Params), dtsParamTypes(dtsDecl.Params)),
			})
		}
		// enrich pairwise up to the shorter arity
		n := len(s.Params)
		if len(dtsDecl.Params) < n {
			n = len(dtsDecl.Params)
		}
		for i := 0; i < n; i++ {
			entry.Params = append(entry.Params, MergedParam{
				Name:     s.Params[i].Name,
				JassType: s.Params[i].Type,
				TSType:   dtsDecl.Params[i].TSType,
				Nullable: dtsDecl.Params[i].Nullable,
			})
		}
		res.Entries = append(res.Entries, entry)
	}

	// dts-only: function declared with no .j partner.
	var dtsOnly []string
	for name := range dtsFns {
		if !jassNames[name] {
			dtsOnly = append(dtsOnly, name)
		}
	}
	sort.Strings(dtsOnly)
	for _, name := range dtsOnly {
		res.Discrepancies = append(res.Discrepancies, Discrepancy{
			Name: name, Kind: "dts-only",
			Detail: ".d.ts function with no matching .j symbol (AI-script helper or generated alias)",
		})
	}
	return res
}

func jassParamTypes(ps []Param) []string {
	out := make([]string, len(ps))
	for i, p := range ps {
		out[i] = p.Type
	}
	return out
}

func dtsParamTypes(ps []DTSParam) []string {
	out := make([]string, len(ps))
	for i, p := range ps {
		out[i] = p.TSType
	}
	return out
}

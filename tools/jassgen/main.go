package main

import (
	"flag"
	"fmt"
	"os"
	"path/filepath"
	"strings"
)

// defaultScriptsDir is where vendored war3-types JASS sources live. A bare
// input name (no path separator) is resolved against it.
const defaultScriptsDir = "repoes/war3-types/scripts"

func main() {
	dumpDecls := flag.String("dump-decls", "", "parse the named JASS file and dump declarations (name+signature) in source order")
	dumpBodies := flag.String("dump-bodies", "", "parse the named JASS file and dump function bodies (AST) in source order")
	dumpMerge := flag.Bool("dump-merge", false, "merge .j natives with .d.ts functions; dump enrichment + discrepancy list")
	dumpClasses := flag.Bool("dump-classes", false, "classify all source symbols D1-D5 (+ unclassified); dump per-class counts and verdicts")
	flag.Parse()

	switch {
	case *dumpDecls != "":
		runDumpDecls(resolveInput(*dumpDecls))
	case *dumpBodies != "":
		runDumpBodies(resolveInput(*dumpBodies))
	case *dumpMerge:
		runDumpMerge()
	case *dumpClasses:
		runDumpClasses()
	default:
		fmt.Fprintln(os.Stderr, "usage: jassgen -dump-decls <file.j> | -dump-bodies <file.j> | -dump-merge | -dump-classes")
		os.Exit(2)
	}
}

func runDumpClasses() {
	const scripts = "repoes/war3-types/scripts"
	bj := ParseFuncs(string(mustRead(scripts + "/blizzard.j")))
	commonNatives := ParseDecls(string(mustRead(scripts + "/common.j")))
	aiNatives := ParseDecls(string(mustRead(scripts + "/common.ai")))

	natives := append(append([]Decl{}, commonNatives...), aiNatives...)
	origins := map[string]string{}
	for _, d := range commonNatives {
		origins[d.Name] = "common"
	}
	for _, d := range aiNatives {
		if _, ok := origins[d.Name]; !ok {
			origins[d.Name] = "commonai"
		}
	}

	cs := ClassifyAll(bj, "blizzard", natives, origins)
	for _, c := range cs {
		fam := ""
		if c.Family != "" {
			fam = " family=" + c.Family
		}
		fmt.Printf("%-4s %-10s %-32s %s%s\n", c.Class, c.Origin, c.Name, c.Evidence, fam)
	}

	counts := TallyClasses(cs)
	fmt.Fprintf(os.Stderr, "--- classification counts (heuristic) ---\n")
	total := 0
	for _, k := range []Class{ClassD1, ClassD2, ClassD3, ClassD4, ClassD5, ClassUnclassified} {
		fmt.Fprintf(os.Stderr, "  %-14s %d\n", k, counts[k])
		total += counts[k]
	}
	fmt.Fprintf(os.Stderr, "  %-14s %d\n", "TOTAL", total)
}

// mergePair pairs a JASS source file with its .d.ts enrichment file.
type mergePair struct {
	origin   string
	jassPath string
	dtsPath  string
}

func runDumpMerge() {
	const scripts = "repoes/war3-types/scripts"
	const core = "repoes/war3-types/core"
	pairs := []mergePair{
		{"common", scripts + "/common.j", core + "/common.d.ts"},
		{"blizzard", scripts + "/blizzard.j", core + "/blizzard.d.ts"},
		{"commonai", scripts + "/common.ai", core + "/commonai.d.ts"},
	}
	totalDisc := 0
	for _, p := range pairs {
		jassSrc := mustRead(p.jassPath)
		dtsSrc := mustRead(p.dtsPath)
		var sigs []JassSig
		if p.origin == "blizzard" {
			sigs = FuncsToSigs(ParseFuncs(string(jassSrc))) // blizzard.j = BJ functions
		} else {
			sigs = DeclsToSigs(ParseDecls(string(jassSrc))) // common/commonai = natives
		}
		dts := ParseDTS(string(dtsSrc))
		res := Merge(sigs, p.origin, dts)

		fmt.Printf("=== %s: %s <-> %s ===\n", p.origin, p.jassPath, p.dtsPath)
		fmt.Printf("  jass natives:   %d\n", res.JassNatives)
		fmt.Printf("  dts functions:  %d\n", res.DTSFunctions)
		fmt.Printf("  merged entries: %d\n", len(res.Entries))
		fmt.Printf("  hierarchy types:%d\n", len(res.Hierarchy))
		fmt.Printf("  discrepancies:  %d\n", len(res.Discrepancies))
		byKind := map[string]int{}
		for _, d := range res.Discrepancies {
			byKind[d.Kind]++
		}
		for _, k := range []string{"jass-only", "dts-only", "arity-mismatch"} {
			if byKind[k] > 0 {
				fmt.Printf("    %-14s %d\n", k, byKind[k])
			}
		}
		for _, d := range res.Discrepancies {
			fmt.Printf("    [%s] %s: %s\n", d.Kind, d.Name, d.Detail)
		}
		// enrichment sample: first 3 entries, jassType|tsType per param
		for i, e := range res.Entries {
			if i >= 3 {
				break
			}
			fmt.Printf("  enrich %s -> %s/%s:\n", e.Name, e.JassReturns, e.TSReturns)
			for _, p := range e.Params {
				fmt.Printf("    %-16s jass=%-10s ts=%-12s nullable=%v\n", p.Name, p.JassType, p.TSType, p.Nullable)
			}
		}
		totalDisc += len(res.Discrepancies)
	}
	fmt.Fprintf(os.Stderr, "total discrepancies across all files: %d\n", totalDisc)
}

func runDumpDecls(path string) {
	src := mustRead(path)
	origin := OriginForFile(path)
	res := ParseDeclsFull(string(src))
	for i := range res.Decls {
		res.Decls[i].Origin = origin
	}
	for _, d := range res.Decls {
		// SoT line: source line, origin, verbatim normalized signature.
		fmt.Printf("%d\t[%s]\t%s\n", d.Line, d.Origin, d.Signature())
	}

	c := Tally(res.Decls)
	fmt.Fprintf(os.Stderr, "--- counts (%s, origin=%s) ---\n", path, origin)
	fmt.Fprintf(os.Stderr, "types:            %d\n", c.Types)
	fmt.Fprintf(os.Stderr, "native (plain):   %d\n", c.PlainNatives)
	fmt.Fprintf(os.Stderr, "constant native:  %d\n", c.ConstantNatives)
	fmt.Fprintf(os.Stderr, "total native:     %d\n", c.TotalNatives())
	fmt.Fprintf(os.Stderr, "excluded funcs:   %d\n", len(res.ExcludedFuncs))
}

// OriginForFile maps a JASS source path to its manifest origin enum value
// (tooling.md §2.4). Unknown files default to the base name without extension.
func OriginForFile(path string) string {
	base := filepath.Base(path)
	switch base {
	case "common.j":
		return "common"
	case "blizzard.j":
		return "blizzard"
	case "common.ai":
		return "commonai"
	default:
		return strings.TrimSuffix(base, filepath.Ext(base))
	}
}

func runDumpBodies(path string) {
	src := mustRead(path)
	funcs := ParseFuncs(string(src))
	shapeCounts := map[string]int{}
	for _, f := range funcs {
		fmt.Printf("%d\t%s", f.Line, f.DumpBody())
		shapeCounts[f.Shape()]++
	}
	fmt.Fprintf(os.Stderr, "--- counts (%s) ---\n", path)
	fmt.Fprintf(os.Stderr, "functions:        %d\n", len(funcs))
	fmt.Fprintf(os.Stderr, "globals decls:    %d\n", CountGlobals(string(src)))
	// stable ordering of shape report
	for _, s := range []string{"passthrough-return", "passthrough-call", "single-call-modified", "control-flow", "other", "empty"} {
		if n := shapeCounts[s]; n > 0 {
			fmt.Fprintf(os.Stderr, "  shape %-22s %d\n", s, n)
		}
	}
}

func mustRead(path string) []byte {
	src, err := os.ReadFile(path)
	if err != nil {
		fmt.Fprintf(os.Stderr, "jassgen: read %s: %v\n", path, err)
		os.Exit(1)
	}
	return src
}

// resolveInput maps a bare file name to the vendored scripts dir; an explicit
// path (containing a separator) is used as-is.
func resolveInput(in string) string {
	if strings.ContainsRune(in, filepath.Separator) || strings.Contains(in, "/") {
		return in
	}
	return filepath.Join(defaultScriptsDir, in)
}

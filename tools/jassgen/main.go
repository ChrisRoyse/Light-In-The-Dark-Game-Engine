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
	emit := flag.Bool("emit", false, "emit api-manifest.json (validated, byte-identical on re-run)")
	out := flag.String("o", "api-manifest.json", "output path for -emit")
	emitStubs := flag.Bool("emit-stubs", false, "generate compiling panic-body Go API stubs from manifest goMapping")
	emitTable := flag.Bool("emit-table", false, "generate the JASS->Go mapping table doc (one row per source function)")
	audit := flag.Bool("audit", false, "generate audit-report.{md,json}; nonzero exit on any M2 gate breach")
	check := flag.Bool("check", false, "reproducibility gate: regenerate outputs and fail if they differ from committed files")
	overridesPath := flag.String("overrides", "tools/jassgen/overrides.toml", "path to reviewed overrides.toml applied over heuristic classes")
	flag.Parse()
	overridesFilePath = *overridesPath

	switch {
	case *dumpDecls != "":
		runDumpDecls(resolveInput(*dumpDecls))
	case *dumpBodies != "":
		runDumpBodies(resolveInput(*dumpBodies))
	case *dumpMerge:
		runDumpMerge()
	case *dumpClasses:
		runDumpClasses()
	case *emit:
		runEmit(*out)
	case *emitStubs:
		runEmitStubs()
	case *emitTable:
		runEmitTable()
	case *audit:
		runAudit()
	case *check:
		runCheck()
	default:
		fmt.Fprintln(os.Stderr, "usage: jassgen -dump-decls <file.j> | -dump-bodies <file.j> | -dump-merge | -dump-classes | -emit | -audit | -check")
		os.Exit(2)
	}
}

// generateOutputs builds the manifest + audit bytes from the current sources.
func generateOutputs() (manifestBytes, auditJSON, auditMD []byte, report AuditReport) {
	cs, sigs, sources := buildClassifiedUniverse()
	m, _ := BuildManifest(cs, sigs, sources)
	if err := ValidateManifest(m); err != nil {
		fatal(fmt.Errorf("manifest failed schema validation: %w", err))
	}
	mb, err := MarshalManifest(m)
	if err != nil {
		fatal(err)
	}
	report = ComputeAudit(cs, m)
	aj, err := MarshalAuditJSON(report)
	if err != nil {
		fatal(err)
	}
	return mb, aj, []byte(RenderAuditMarkdown(report)), report
}

func runAudit() {
	_, auditJSON, auditMD, report := generateOutputs()
	if err := os.WriteFile("audit-report.json", auditJSON, 0o644); err != nil {
		fatal(err)
	}
	if err := os.WriteFile("audit-report.md", auditMD, 0o644); err != nil {
		fatal(err)
	}
	fmt.Fprintf(os.Stderr, "audit: total=%d unclassified=%d unmapped=%d mapped=%d tombstoned=%d\n",
		report.Total, report.Unclassified, report.Unmapped, report.Mapped, report.Tombstoned)
	if len(report.Violations) > 0 {
		fmt.Fprintf(os.Stderr, "M2 GATE FAIL — %d violation(s):\n", len(report.Violations))
		for _, v := range report.Violations {
			fmt.Fprintln(os.Stderr, "  -", v)
		}
		os.Exit(1)
	}
	fmt.Fprintln(os.Stderr, "M2 gates: GREEN")
}

// runCheck regenerates all outputs and fails if any differs from the committed
// file (reproducibility gate).
func runCheck() {
	mb, aj, amd, _ := generateOutputs()
	cs, _, _ := buildClassifiedUniverse()
	table, _ := RenderMappingTable(cs)
	fail := false
	for _, f := range []struct {
		path string
		want []byte
	}{
		{"api-manifest.json", mb},
		{"audit-report.json", aj},
		{"audit-report.md", amd},
		{mappingTablePath, []byte(table)},
	} {
		got, err := os.ReadFile(f.path)
		if err != nil {
			fmt.Fprintf(os.Stderr, "check: cannot read committed %s: %v\n", f.path, err)
			fail = true
			continue
		}
		if string(got) != string(f.want) {
			fmt.Fprintf(os.Stderr, "check: %s differs from regenerated output (re-run -emit/-audit and commit)\n", f.path)
			fail = true
		}
	}
	if fail {
		os.Exit(1)
	}
	fmt.Fprintln(os.Stderr, "check: all outputs reproducible (byte-identical to committed)")
}

// buildClassifiedUniverse parses all sources, classifies, and applies overrides.
// Returns the classifications, the per-symbol merged signatures, and the source
// metadata entries. Fatal on any error.
func buildClassifiedUniverse() ([]Classification, map[string]MergedEntry, []SourceEntry) {
	const scripts = "repoes/war3-types/scripts"
	const core = "repoes/war3-types/core"

	bj := ParseFuncs(string(mustRead(scripts + "/blizzard.j")))
	commonN := ParseDecls(string(mustRead(scripts + "/common.j")))
	aiN := ParseDecls(string(mustRead(scripts + "/common.ai")))

	natives := append(append([]Decl{}, commonN...), aiN...)
	origins := map[string]string{}
	for _, d := range commonN {
		origins[d.Name] = "common"
	}
	for _, d := range aiN {
		if _, ok := origins[d.Name]; !ok {
			origins[d.Name] = "commonai"
		}
	}

	cs := ClassifyAll(bj, "blizzard", natives, origins)
	ovs, err := LoadOverrides(overridesFilePath)
	if err != nil {
		fatal(err)
	}
	cs, err = ApplyOverrides(cs, ovs)
	if err != nil {
		fatal(err)
	}

	// Per-symbol signatures from the merge pass (jassType + tsType + returns).
	sigs := map[string]MergedEntry{}
	mergeInto := func(res MergeResult) {
		for _, e := range res.Entries {
			sigs[e.Name] = e
		}
	}
	mergeInto(Merge(FuncsToSigs(bj), "blizzard", ParseDTS(string(mustRead(core+"/blizzard.d.ts")))))
	mergeInto(Merge(DeclsToSigs(commonN), "common", ParseDTS(string(mustRead(core+"/common.d.ts")))))
	mergeInto(Merge(DeclsToSigs(aiN), "commonai", ParseDTS(string(mustRead(core+"/commonai.d.ts")))))

	cn := Tally(commonN)
	an := Tally(aiN)
	sources := []SourceEntry{}
	for _, sm := range []struct {
		path, file string
		count      int
	}{
		{scripts + "/common.j", "common.j", cn.TotalNatives()},
		{scripts + "/blizzard.j", "blizzard.j", len(bj)},
		{scripts + "/common.ai", "commonai", an.TotalNatives()},
	} {
		se, err := sourceMeta(sm.path, sm.file, sm.count)
		if err != nil {
			fatal(err)
		}
		sources = append(sources, se)
	}
	return cs, sigs, sources
}

func runEmit(outPath string) {
	cs, sigs, sources := buildClassifiedUniverse()
	m, skipped := BuildManifest(cs, sigs, sources)
	if err := ValidateManifest(m); err != nil {
		fatal(fmt.Errorf("manifest failed schema validation: %w", err))
	}
	data, err := MarshalManifest(m)
	if err != nil {
		fatal(err)
	}
	if err := os.WriteFile(outPath, data, 0o644); err != nil {
		fatal(err)
	}
	fmt.Fprintf(os.Stderr, "wrote %s: %d functions emitted, %d unresolved (no D1-D5 class / no mapping)\n",
		outPath, len(m.Functions), skipped)
}

const mappingTablePath = "docs/prd/03-api/jass-mapping/mapping-table.md"

func runEmitTable() {
	cs, _, _ := buildClassifiedUniverse()
	table, rows := RenderMappingTable(cs)
	if rows != 2642 {
		fatal(fmt.Errorf("mapping table has %d rows, want 2642 (one per source function)", rows))
	}
	if err := os.WriteFile(mappingTablePath, []byte(table), 0o644); err != nil {
		fatal(err)
	}
	fmt.Fprintf(os.Stderr, "wrote %s: %d rows\n", mappingTablePath, rows)
}

func runEmitStubs() {
	cs, sigs, sources := buildClassifiedUniverse()
	m, _ := BuildManifest(cs, sigs, sources)
	if err := ValidateManifest(m); err != nil {
		fatal(fmt.Errorf("manifest invalid, refusing to stub: %w", err))
	}
	emitted, skipped, err := GenerateStubs(m)
	if err != nil {
		fatal(err)
	}
	for pkg := range pkgTargets {
		fmt.Fprintf(os.Stderr, "%s: %d emitted, %d skipped (already implemented)\n", pkg, len(emitted[pkg]), len(skipped[pkg]))
		for _, s := range skipped[pkg] {
			fmt.Fprintf(os.Stderr, "    skip %s\n", s)
		}
	}
}

func fatal(err error) {
	fmt.Fprintln(os.Stderr, "jassgen:", err)
	os.Exit(1)
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

	// Apply reviewed overrides (human judgment); validation failures are fatal.
	ovs, err := LoadOverrides(overridesFilePath)
	if err != nil {
		fmt.Fprintln(os.Stderr, "jassgen:", err)
		os.Exit(1)
	}
	cs, err = ApplyOverrides(cs, ovs)
	if err != nil {
		fmt.Fprintln(os.Stderr, "jassgen:", err)
		os.Exit(1)
	}

	for _, c := range cs {
		extra := ""
		if c.Family != "" {
			extra += " family=" + c.Family
		}
		if c.Tombstone != "" {
			extra += " tombstone=" + c.Tombstone
		}
		if c.GoMapping != "" {
			extra += " -> " + c.GoMapping
		}
		fmt.Printf("%-4s %-9s %-9s %-32s %s%s\n", c.Class, c.ClassifiedBy, c.Origin, c.Name, c.Evidence, extra)
	}

	counts := TallyClasses(cs)
	fmt.Fprintf(os.Stderr, "--- classification counts ---\n")
	total := 0
	for _, k := range []Class{ClassD1, ClassD2, ClassD3, ClassD4, ClassD5, ClassUnclassified} {
		fmt.Fprintf(os.Stderr, "  %-14s %d\n", k, counts[k])
		total += counts[k]
	}
	fmt.Fprintf(os.Stderr, "  %-14s %d\n", "TOTAL", total)
	fmt.Fprintf(os.Stderr, "  overrides applied: %d (classifiedBy=override)\n", CountOverridden(cs))
}

// overridesFilePath is the active overrides path (set from the -overrides flag).
var overridesFilePath = "tools/jassgen/overrides.toml"

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

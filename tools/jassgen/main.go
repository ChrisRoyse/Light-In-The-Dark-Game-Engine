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
	flag.Parse()

	switch {
	case *dumpDecls != "":
		runDumpDecls(resolveInput(*dumpDecls))
	case *dumpBodies != "":
		runDumpBodies(resolveInput(*dumpBodies))
	default:
		fmt.Fprintln(os.Stderr, "usage: jassgen -dump-decls <file.j> | -dump-bodies <file.j>")
		os.Exit(2)
	}
}

func runDumpDecls(path string) {
	src := mustRead(path)
	decls := ParseDecls(string(src))
	for _, d := range decls {
		// SoT line: 1-based source line, kind, verbatim normalized signature.
		fmt.Printf("%d\t%s\n", d.Line, d.Signature())
	}

	c := Tally(decls)
	fmt.Fprintf(os.Stderr, "--- counts (%s) ---\n", path)
	fmt.Fprintf(os.Stderr, "types:            %d\n", c.Types)
	fmt.Fprintf(os.Stderr, "native (plain):   %d\n", c.PlainNatives)
	fmt.Fprintf(os.Stderr, "constant native:  %d\n", c.ConstantNatives)
	fmt.Fprintf(os.Stderr, "total native:     %d\n", c.TotalNatives())
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

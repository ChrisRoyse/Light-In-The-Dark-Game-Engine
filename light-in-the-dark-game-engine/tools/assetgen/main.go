package main

import (
	"flag"
	"fmt"
	"os"
)

func main() {
	if len(os.Args) < 2 {
		usage()
	}
	switch os.Args[1] {
	case "provenance":
		runProvenance(os.Args[2:])
	case "pipeline":
		runPipeline(os.Args[2:])
	default:
		usage()
	}
}

func usage() {
	fmt.Fprintln(os.Stderr, "usage:")
	fmt.Fprintln(os.Stderr, "  assetgen provenance --assets <dir> --path <rel> [fields...]")
	fmt.Fprintln(os.Stderr, "  assetgen pipeline --spec <assetgen.toml> --assets <dir> --scratch <dir> --checker <assetcheck-bin>")
	os.Exit(2)
}

func runProvenance(args []string) {
	fs := flag.NewFlagSet("provenance", flag.ExitOnError)
	assets := fs.String("assets", "assets", "assets root containing MANIFEST and the asset file")
	var e Entry
	fs.StringVar(&e.Path, "path", "", "asset path relative to assets/ (forward slashes)")
	fs.StringVar(&e.Pack, "pack", "", "pack/source name")
	fs.StringVar(&e.Source, "source", "", "source URL")
	fs.StringVar(&e.License, "license", "CC0-1.0", "SPDX license (CC0-1.0 or free-commercial)")
	fs.StringVar(&e.Retrieved, "retrieved", "", "generation date (YYYY-MM-DD)")
	fs.StringVar(&e.Category, "category", "", "triangle-budget category (optional)")
	fs.StringVar(&e.Generator, "generator", "", "generating model/tool + version")
	fs.StringVar(&e.Params, "params", "", "generation params or assetgen.toml ref")
	fs.StringVar(&e.Curator, "curator", "", "human sign-off (required)")
	fs.Parse(args)

	if err := AppendFile(*assets, e); err != nil {
		fmt.Fprintln(os.Stderr, "assetgen:", err)
		os.Exit(1)
	}
	fmt.Printf("wrote provenance entry for %s\n", e.Path)
}

func runPipeline(args []string) {
	fs := flag.NewFlagSet("pipeline", flag.ExitOnError)
	specPath := fs.String("spec", "", "path to assetgen.toml")
	assets := fs.String("assets", "assets", "assets root to commit accepted assets into")
	scratch := fs.String("scratch", "", "scratch directory for raw candidates (required; never assets/)")
	checker := fs.String("checker", "assetcheck", "path to the assetcheck binary for the gate")
	pack := fs.String("pack", "Light in the Dark (generated)", "provenance pack name")
	source := fs.String("source", "assetgen", "provenance source reference")
	retrieved := fs.String("retrieved", "", "generation date (YYYY-MM-DD); required")
	fs.Parse(args)

	if *specPath == "" || *scratch == "" || *retrieved == "" {
		fmt.Fprintln(os.Stderr, "assetgen pipeline: --spec, --scratch, and --retrieved are required")
		os.Exit(2)
	}

	f, err := os.Open(*specPath)
	if err != nil {
		fmt.Fprintln(os.Stderr, "assetgen pipeline:", err)
		os.Exit(1)
	}
	items, err := ParseSpec(f)
	f.Close()
	if err != nil {
		fmt.Fprintln(os.Stderr, "assetgen pipeline: spec parse:", err)
		os.Exit(1)
	}
	if err := os.MkdirAll(*scratch, 0o755); err != nil {
		fmt.Fprintln(os.Stderr, "assetgen pipeline:", err)
		os.Exit(1)
	}

	p := Pipeline{
		ScratchDir: *scratch, AssetsDir: *assets,
		Gen:  StubGenerator{},
		Cur:  NewInteractiveCurator(os.Stdin, os.Stdout),
		Chk:  ExecChecker{Bin: *checker},
		Pack: *pack, Source: *source, Retrieved: *retrieved,
		Log: os.Stdout,
	}
	rep, err := p.Run(items)
	if err != nil {
		fmt.Fprintln(os.Stderr, "assetgen pipeline:", err)
		os.Exit(1)
	}

	fmt.Printf("\npipeline: %d committed, %d blocked\n", len(rep.Committed), len(rep.Blocked))
	for cat, n := range rep.Rejected {
		fmt.Printf("  rejected[%s] = %d\n", cat, n)
	}
	if len(rep.Blocked) > 0 {
		os.Exit(1) // a blocked candidate is a non-clean run
	}
}

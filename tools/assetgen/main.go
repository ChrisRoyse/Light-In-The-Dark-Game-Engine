package main

import (
	"flag"
	"fmt"
	"os"
)

func main() {
	if len(os.Args) < 2 || os.Args[1] != "provenance" {
		fmt.Fprintln(os.Stderr, "usage: assetgen provenance --assets <dir> --path <rel> [fields...]")
		os.Exit(2)
	}
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
	fs.Parse(os.Args[2:])

	if err := AppendFile(*assets, e); err != nil {
		fmt.Fprintln(os.Stderr, "assetgen:", err)
		os.Exit(1)
	}
	fmt.Printf("wrote provenance entry for %s\n", e.Path)
}

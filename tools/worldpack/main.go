package main

import (
	"bufio"
	"crypto/sha256"
	"encoding/hex"
	"flag"
	"fmt"
	"io"
	"os"
	"strings"
)

// loadCategories reads a category map file: one `<rel-path> <category>` per line,
// blank lines and `#` comments ignored. It is how `worldpack pack` learns each
// embedded model's triangle-budget category (the make-level producer derives
// these from assets/MANIFEST). Fail-closed: a malformed or duplicate line errors.
func loadCategories(path string) (map[string]string, error) {
	f, err := os.Open(path)
	if err != nil {
		return nil, err
	}
	defer f.Close()
	m := map[string]string{}
	sc := bufio.NewScanner(f)
	n := 0
	for sc.Scan() {
		n++
		line := strings.TrimSpace(sc.Text())
		if line == "" || strings.HasPrefix(line, "#") {
			continue
		}
		fields := strings.Fields(line)
		if len(fields) != 2 {
			return nil, fmt.Errorf("%s line %d: want `<rel> <category>`, got %q", path, n, line)
		}
		if prev, dup := m[fields[0]]; dup {
			return nil, fmt.Errorf("%s line %d: duplicate category for %q (first %q)", path, n, fields[0], prev)
		}
		m[fields[0]] = fields[1]
	}
	return m, sc.Err()
}

func main() {
	if len(os.Args) < 2 {
		usage()
		os.Exit(2)
	}
	switch os.Args[1] {
	case "pack":
		fs := flag.NewFlagSet("pack", flag.ExitOnError)
		engine := fs.String("engine", "", "engine-version range to record in the manifest (e.g. \">=0.1.0 <0.2.0\")")
		author := fs.String("author", "", "hosting metadata: world author (may be empty)")
		title := fs.String("title", "", "hosting metadata: world title (may be empty)")
		desc := fs.String("description", "", "hosting metadata: world description (may be empty)")
		catFile := fs.String("categories", "", "path to a `<rel> <category>` map for embedded .glb models (unit|building|other); required if src embeds models")
		fs.Parse(os.Args[2:])
		if fs.NArg() != 2 {
			fmt.Fprintln(os.Stderr, "usage: worldpack pack [--engine RANGE] [--author A] [--title T] [--description D] [--categories FILE] <src-dir> <out.litdworld>")
			os.Exit(2)
		}
		var categories map[string]string
		if *catFile != "" {
			var err error
			if categories, err = loadCategories(*catFile); err != nil {
				fmt.Fprintln(os.Stderr, "worldpack: pack: categories:", err)
				os.Exit(1)
			}
		}
		if err := Pack(fs.Arg(0), fs.Arg(1), *engine, Hosting{Author: *author, Title: *title, Description: *desc}, categories); err != nil {
			fmt.Fprintln(os.Stderr, "worldpack: pack:", err)
			os.Exit(1)
		}
		sum, err := fileSHA256(fs.Arg(1))
		if err != nil {
			fmt.Fprintln(os.Stderr, "worldpack:", err)
			os.Exit(1)
		}
		fmt.Printf("%s  %s\n", sum, fs.Arg(1))
	case "unpack":
		fs := flag.NewFlagSet("unpack", flag.ExitOnError)
		fs.Parse(os.Args[2:])
		if fs.NArg() != 2 {
			fmt.Fprintln(os.Stderr, "usage: worldpack unpack <archive.litdworld> <dest-dir>")
			os.Exit(2)
		}
		if err := Unpack(fs.Arg(0), fs.Arg(1)); err != nil {
			fmt.Fprintln(os.Stderr, "worldpack: unpack:", err)
			os.Exit(1)
		}
		fmt.Printf("unpacked %s -> %s (all hashes verified)\n", fs.Arg(0), fs.Arg(1))
	default:
		usage()
		os.Exit(2)
	}
}

func usage() {
	fmt.Fprintln(os.Stderr, "usage: worldpack pack [--engine RANGE] <src-dir> <out.litdworld>")
	fmt.Fprintln(os.Stderr, "       worldpack unpack <archive.litdworld> <dest-dir>")
}

func fileSHA256(path string) (string, error) {
	f, err := os.Open(path)
	if err != nil {
		return "", err
	}
	defer f.Close()
	h := sha256.New()
	if _, err := io.Copy(h, f); err != nil {
		return "", err
	}
	return hex.EncodeToString(h.Sum(nil)), nil
}

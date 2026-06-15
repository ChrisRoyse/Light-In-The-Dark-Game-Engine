package main

import (
	"crypto/sha256"
	"encoding/hex"
	"flag"
	"fmt"
	"io"
	"os"
)

func main() {
	if len(os.Args) < 2 {
		usage()
		os.Exit(2)
	}
	switch os.Args[1] {
	case "pack":
		fs := flag.NewFlagSet("pack", flag.ExitOnError)
		engine := fs.String("engine", "", "engine-version range to record in the manifest (e.g. \">=0.1.0 <0.2.0\")")
		fs.Parse(os.Args[2:])
		if fs.NArg() != 2 {
			fmt.Fprintln(os.Stderr, "usage: worldpack pack [--engine RANGE] <src-dir> <out.litdworld>")
			os.Exit(2)
		}
		if err := Pack(fs.Arg(0), fs.Arg(1), *engine); err != nil {
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

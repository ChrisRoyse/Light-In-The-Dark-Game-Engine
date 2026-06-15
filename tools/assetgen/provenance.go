// Package main implements assetgen helpers. provenance.go is the provenance
// entry writer (#59; tooling.md §5.2 step 5, §5.3; G4.2/G4.7): every accepted
// generated asset gets a G4.7-complete assets/MANIFEST entry at commit time.
// The fields written are exactly those the assetcheck provenance check
// requires, so a written entry always passes that check. Writes are canonical
// (entries sorted by path) for deterministic diffs, and a duplicate path is a
// hard error — never a silent overwrite.
package main

import (
	"crypto/sha256"
	"encoding/hex"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"sort"
	"strings"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/tools/assetcheck/manifest"
)

// Entry is a fully-specified generated-asset provenance record.
type Entry struct {
	Path      string
	Pack      string
	Source    string
	License   string
	Retrieved string // YYYY-MM-DD
	SHA256    string // lowercase hex; computed from the file when written via AppendFile
	Category  string // optional triangle-budget category
	Generator string // generating model/tool + version (G4.7)
	Params    string // generation parameters or assetgen.toml ref (G4.7)
	Curator   string // human sign-off (G4.7)
}

// validate enforces the mandatory fields. A generated asset with no curator is
// refused — there is no accept-without-sign-off path.
func (e Entry) validate() error {
	for _, c := range []struct{ name, val string }{
		{"path", e.Path}, {"pack", e.Pack}, {"source", e.Source}, {"license", e.License},
		{"retrieved", e.Retrieved}, {"sha256", e.SHA256},
		{"generator", e.Generator}, {"params", e.Params}, {"curator", e.Curator},
	} {
		if strings.TrimSpace(c.val) == "" {
			return fmt.Errorf("provenance entry for %q missing required field %q", e.Path, c.name)
		}
	}
	if !manifest.LicenseAllowed(e.License) {
		return fmt.Errorf("license %q not allowed (policy: CC0-1.0 or free-commercial)", e.License)
	}
	if strings.Contains(e.Path, "\\") || strings.HasPrefix(e.Path, "/") || strings.Contains(e.Path, "..") {
		return fmt.Errorf("path %q must be relative with forward slashes", e.Path)
	}
	return nil
}

// fileSHA256 hashes a file's bytes (streaming).
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

// AppendFile fills e.SHA256 from the asset file at assetsDir/e.Path and appends
// the entry to assetsDir/MANIFEST. It is the only sanctioned path from an
// accepted generated asset to the ledger.
func AppendFile(assetsDir string, e Entry) error {
	sum, err := fileSHA256(filepath.Join(assetsDir, filepath.FromSlash(e.Path)))
	if err != nil {
		return fmt.Errorf("hash asset %q: %w", e.Path, err)
	}
	e.SHA256 = sum
	return AppendEntry(filepath.Join(assetsDir, "MANIFEST"), e)
}

// AppendEntry validates and merges one entry into the MANIFEST at manifestPath,
// rewriting it in canonical (path-sorted) order. A duplicate path is a hard
// error printing both the existing and the new entry.
func AppendEntry(manifestPath string, e Entry) error {
	if err := e.validate(); err != nil {
		return err
	}

	header := defaultHeader
	var existing []manifest.Asset
	if raw, rerr := os.ReadFile(manifestPath); rerr == nil {
		header = extractHeader(string(raw))
		parsed, perr := manifest.Parse(strings.NewReader(string(raw)))
		if perr != nil {
			return fmt.Errorf("existing MANIFEST does not parse: %w", perr)
		}
		existing = parsed
	} else if !os.IsNotExist(rerr) {
		return rerr
	}

	for _, a := range existing {
		if a.Path == e.Path {
			return fmt.Errorf("duplicate entry for %q — refusing silent overwrite.\n existing: %s\n new:      %s",
				e.Path, oneLine(a), oneLineEntry(e))
		}
	}

	all := append(existing, manifest.Asset{
		Path: e.Path, Pack: e.Pack, Source: e.Source, License: e.License,
		Retrieved: e.Retrieved, SHA256: strings.ToLower(e.SHA256), Category: e.Category,
		Provenance: "generated", Generator: e.Generator, Params: e.Params, Curator: e.Curator,
	})
	sort.Slice(all, func(i, j int) bool { return all[i].Path < all[j].Path })

	out := render(header, all)
	return os.WriteFile(manifestPath, []byte(out), 0o644)
}

const defaultHeader = "# assets/MANIFEST — asset provenance ledger (G4.2, G4.7).\n# Generated entries are written by tools/assetgen; do not hand-edit field order.\n"

// extractHeader returns the comment/blank lines before the first [[asset]].
func extractHeader(raw string) string {
	var b strings.Builder
	for _, line := range strings.Split(raw, "\n") {
		if strings.TrimSpace(line) == "[[asset]]" {
			break
		}
		b.WriteString(line)
		b.WriteByte('\n')
	}
	h := b.String()
	if strings.TrimSpace(h) == "" {
		return defaultHeader
	}
	return h
}

// render serializes the ledger in a fixed key order.
func render(header string, assets []manifest.Asset) string {
	var b strings.Builder
	b.WriteString(strings.TrimRight(header, "\n"))
	b.WriteByte('\n')
	for _, a := range assets {
		b.WriteString("\n[[asset]]\n")
		kv(&b, "path", a.Path)
		kv(&b, "pack", a.Pack)
		kv(&b, "source", a.Source)
		kv(&b, "license", a.License)
		kv(&b, "retrieved", a.Retrieved)
		kv(&b, "sha256", a.SHA256)
		kvOpt(&b, "category", a.Category)
		kvOpt(&b, "provenance", a.Provenance)
		kvOpt(&b, "generator", a.Generator)
		kvOpt(&b, "params", a.Params)
		kvOpt(&b, "curator", a.Curator)
	}
	return b.String()
}

func kv(b *strings.Builder, k, v string) { fmt.Fprintf(b, "%s = %q\n", k, v) }
func kvOpt(b *strings.Builder, k, v string) {
	if v != "" {
		kv(b, k, v)
	}
}

func oneLine(a manifest.Asset) string {
	return fmt.Sprintf("path=%q pack=%q license=%q sha256=%q", a.Path, a.Pack, a.License, a.SHA256)
}
func oneLineEntry(e Entry) string {
	return fmt.Sprintf("path=%q pack=%q license=%q sha256=%q", e.Path, e.Pack, e.License, e.SHA256)
}

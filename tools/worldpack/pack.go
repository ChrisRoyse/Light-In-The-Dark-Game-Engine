// Package main implements worldpack: a deterministic world-archive builder and
// unpacker (#10; D-2026-06-11-33, D-14). A given source directory always packs
// to a byte-identical `.litdworld` archive — sorted entry order, a fixed
// timestamp, uniform file mode, and Deflate — so the content hash is stable
// across machines and OSes. The archive carries a content-hash manifest
// (consumed by the hub and the M7 join-guard) and an engine-version range.
package main

import (
	"archive/zip"
	"crypto/sha256"
	"encoding/hex"
	"fmt"
	"io"
	"io/fs"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"time"
)

// manifestName is the reserved archive entry holding the content-hash TOC.
const manifestName = ".litdworld-manifest"

// fixedModTime pins every entry's timestamp to the ZIP epoch (1980-01-01 UTC),
// removing filesystem mtime as a source of nondeterminism.
var fixedModTime = time.Date(1980, 1, 1, 0, 0, 0, 0, time.UTC)

// fileEntry is one packed file with its streamed content hash.
type fileEntry struct {
	Rel  string
	Hash string
	Size int64
}

// collect walks srcDir, returning the relative paths of all regular files in
// sorted order, and erroring on a reserved name or a case collision.
func collect(srcDir string) ([]string, error) {
	var rels []string
	err := filepath.WalkDir(srcDir, func(p string, d fs.DirEntry, err error) error {
		if err != nil {
			return err
		}
		if d.IsDir() {
			return nil
		}
		if !d.Type().IsRegular() {
			return fmt.Errorf("%s is not a regular file (symlinks/devices are not packable)", p)
		}
		rel, err := filepath.Rel(srcDir, p)
		if err != nil {
			return err
		}
		rels = append(rels, filepath.ToSlash(rel))
		return nil
	})
	if err != nil {
		return nil, err
	}
	sort.Strings(rels)

	seen := make(map[string]string, len(rels))
	for _, rel := range rels {
		if rel == manifestName {
			return nil, fmt.Errorf("source contains reserved entry name %q", manifestName)
		}
		lc := strings.ToLower(rel)
		if prev, ok := seen[lc]; ok {
			return nil, fmt.Errorf("filename case collision: %q and %q resolve to the same archive entry", prev, rel)
		}
		seen[lc] = rel
	}
	return rels, nil
}

// hashFiles streams each file through SHA-256 (no full-file buffering).
func hashFiles(srcDir string, rels []string) ([]fileEntry, error) {
	entries := make([]fileEntry, 0, len(rels))
	for _, rel := range rels {
		f, err := os.Open(filepath.Join(srcDir, filepath.FromSlash(rel)))
		if err != nil {
			return nil, err
		}
		h := sha256.New()
		n, err := io.Copy(h, f)
		f.Close()
		if err != nil {
			return nil, err
		}
		entries = append(entries, fileEntry{Rel: rel, Hash: hex.EncodeToString(h.Sum(nil)), Size: n})
	}
	return entries, nil
}

// Hosting carries the world's hosting metadata (D-23): present in every archive
// from day one, values may be empty pre-M9. Single-line each (newlines are
// stripped — the manifest is a line format).
type Hosting struct {
	Author      string
	Title       string
	Description string
}

// oneLine collapses any newlines so a metadata value can't break the line-based
// manifest format (and trims surrounding space).
func oneLine(s string) string {
	s = strings.ReplaceAll(s, "\r", " ")
	s = strings.ReplaceAll(s, "\n", " ")
	return strings.TrimSpace(s)
}

// AggregateHash is the whole-archive fingerprint: SHA-256 over each entry's
// per-file hash, taken in Rel-sorted order (so it is independent of pack order
// and of the zip layout). A single declared value lets a loader detect any
// added/removed/rehashed entry with one comparison (D-14: "SHA-256 per entry +
// aggregate"). It is computed from the per-entry hashes alone — the validator
// recomputes it from the manifest rows without re-reading file bytes.
func AggregateHash(entries []fileEntry) string {
	sorted := make([]fileEntry, len(entries))
	copy(sorted, entries)
	sort.Slice(sorted, func(i, j int) bool { return sorted[i].Rel < sorted[j].Rel })
	h := sha256.New()
	for _, e := range sorted {
		io.WriteString(h, e.Hash)
		io.WriteString(h, "\n")
	}
	return hex.EncodeToString(h.Sum(nil))
}

// buildManifest renders the deterministic content-hash TOC. Header order is
// fixed (version, engine-range, hosting metadata, aggregate, files) so the
// archive is byte-stable across runs.
func buildManifest(engineRange string, host Hosting, entries []fileEntry) string {
	var b strings.Builder
	b.WriteString("litdworld-version: 1\n")
	fmt.Fprintf(&b, "engine-range: %s\n", engineRange)
	fmt.Fprintf(&b, "author: %s\n", oneLine(host.Author))
	fmt.Fprintf(&b, "title: %s\n", oneLine(host.Title))
	fmt.Fprintf(&b, "description: %s\n", oneLine(host.Description))
	fmt.Fprintf(&b, "aggregate-sha256: %s\n", AggregateHash(entries))
	fmt.Fprintf(&b, "files: %d\n", len(entries))
	for _, e := range entries {
		fmt.Fprintf(&b, "%s %d %s\n", e.Hash, e.Size, e.Rel)
	}
	return b.String()
}

// Pack writes a deterministic archive of srcDir to outPath.
func Pack(srcDir, outPath, engineRange string, host Hosting) error {
	st, err := os.Stat(srcDir)
	if err != nil || !st.IsDir() {
		return fmt.Errorf("source %q is not a directory", srcDir)
	}
	rels, err := collect(srcDir)
	if err != nil {
		return err
	}
	entries, err := hashFiles(srcDir, rels)
	if err != nil {
		return err
	}
	manifest := buildManifest(engineRange, host, entries)

	// Sort all entry names (manifest included) for a stable archive layout.
	names := make([]string, 0, len(rels)+1)
	names = append(names, manifestName)
	names = append(names, rels...)
	sort.Strings(names)

	out, err := os.Create(outPath)
	if err != nil {
		return err
	}
	zw := zip.NewWriter(out)
	for _, name := range names {
		hdr := &zip.FileHeader{Name: name, Method: zip.Deflate}
		hdr.Modified = fixedModTime
		hdr.SetMode(0o644)
		w, err := zw.CreateHeader(hdr)
		if err != nil {
			zw.Close()
			out.Close()
			return err
		}
		if name == manifestName {
			if _, err := io.WriteString(w, manifest); err != nil {
				zw.Close()
				out.Close()
				return err
			}
			continue
		}
		f, err := os.Open(filepath.Join(srcDir, filepath.FromSlash(name)))
		if err != nil {
			zw.Close()
			out.Close()
			return err
		}
		_, err = io.Copy(w, f)
		f.Close()
		if err != nil {
			zw.Close()
			out.Close()
			return err
		}
	}
	if err := zw.Close(); err != nil {
		out.Close()
		return err
	}
	return out.Close()
}

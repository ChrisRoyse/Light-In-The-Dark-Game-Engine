// Package worldarchive is the in-engine read path for world archives
// (.litdworld, #205 "archive read path in litd/asset"; #209 "the game loads its
// own map THROUGH the archive"). It opens a packed archive, FULLY verifies it
// against the embedded content-hash manifest, and exposes the verified payload
// as an fs.FS — so the rest of the engine (mapdata.Load, the Lua world loader)
// reads an archive exactly as it reads a directory, with no archive-specific
// code downstream.
//
// Fail-closed (R-FMT-2, doctrine §2.4): Open verifies the manifest schema, every
// per-entry SHA-256, the aggregate fingerprint, and the engine-version range
// (optionally that the running engine satisfies it) BEFORE returning a usable
// FS. Any mismatch is a loud error and no FS is handed back — a tampered or
// malformed archive never loads partially. The manifest parser here is
// deliberately independent of the worldpack writer and the assetcheck validator
// (a writer or tooling bug cannot mask a load-time verification bug).
package worldarchive

import (
	"archive/zip"
	"crypto/sha256"
	"encoding/hex"
	"fmt"
	"io"
	"io/fs"
	"sort"
	"strconv"
	"strings"
)

// manifestName is the reserved archive entry holding the content-hash TOC.
const manifestName = ".litdworld-manifest"

// FileEntry is one verified payload file's manifest row.
type FileEntry struct {
	Hash string
	Size int64
}

// Manifest is the parsed, verified archive table of contents.
type Manifest struct {
	Version     int
	EngineRange string
	Author      string
	Title       string
	Description string
	Aggregate   string
	Files       map[string]FileEntry // payload rel-path -> entry
}

// Archive is an opened, fully verified world archive. FS serves the verified
// payload files (the manifest entry is hidden). Close releases the underlying
// file handle.
type Archive struct {
	rc       *zip.ReadCloser
	Manifest Manifest
}

// Open opens path and verifies it end to end. engineVersion, when non-empty,
// must satisfy the manifest's engine-range (the loader join-guard); pass "" to
// check only that the range is well-formed. The returned Archive must be Closed.
func Open(path, engineVersion string) (*Archive, error) {
	rc, err := zip.OpenReader(path)
	if err != nil {
		return nil, fmt.Errorf("worldarchive: open %q: %w", path, err)
	}
	man, err := verify(&rc.Reader, engineVersion)
	if err != nil {
		rc.Close()
		return nil, fmt.Errorf("worldarchive: %q: %w", path, err)
	}
	return &Archive{rc: rc, Manifest: man}, nil
}

// FS returns an fs.FS over the verified archive payload. A zip.Reader is itself
// an fs.FS, so mapdata.Load / the Lua loader read it like a directory.
func (a *Archive) FS() fs.FS { return &a.rc.Reader }

// Close releases the archive's file handle.
func (a *Archive) Close() error { return a.rc.Close() }

// verify parses the manifest and checks schema, per-entry hashes, the aggregate,
// and the engine-range. It reads every payload entry's bytes from fsys — the
// SoT — rather than trusting the manifest's own numbers.
func verify(fsys fs.FS, engineVersion string) (Manifest, error) {
	body, err := fs.ReadFile(fsys, manifestName)
	if err != nil {
		return Manifest{}, fmt.Errorf("no %s entry: %w", manifestName, err)
	}
	man, err := parseManifest(string(body))
	if err != nil {
		return Manifest{}, err
	}
	if man.EngineRange == "" || !validEngineRange(man.EngineRange) {
		return Manifest{}, fmt.Errorf("engine-range %q is missing or not well-formed", man.EngineRange)
	}
	if engineVersion != "" && !satisfiesRange(engineVersion, man.EngineRange) {
		return Manifest{}, fmt.Errorf("engine %s does not satisfy engine-range %q", engineVersion, man.EngineRange)
	}

	// Walk the actual payload, hashing each file; cross-check against the manifest.
	seen := map[string]bool{}
	walkErr := fs.WalkDir(fsys, ".", func(p string, d fs.DirEntry, err error) error {
		if err != nil {
			return err
		}
		if d.IsDir() || p == manifestName {
			return nil
		}
		data, rerr := fs.ReadFile(fsys, p)
		if rerr != nil {
			return fmt.Errorf("read entry %q: %w", p, rerr)
		}
		want, listed := man.Files[p]
		if !listed {
			return fmt.Errorf("entry %q is not listed in the manifest", p)
		}
		sum := sha256.Sum256(data)
		if got := hex.EncodeToString(sum[:]); got != want.Hash {
			return fmt.Errorf("entry %q content hash %s does not match manifest %s", p, got, want.Hash)
		}
		seen[p] = true
		return nil
	})
	if walkErr != nil {
		return Manifest{}, walkErr
	}
	for rel := range man.Files {
		if !seen[rel] {
			return Manifest{}, fmt.Errorf("manifest lists %q but the archive has no such entry", rel)
		}
	}
	if got := aggregate(man.Files); got != man.Aggregate {
		return Manifest{}, fmt.Errorf("aggregate hash %s does not match manifest %s", got, man.Aggregate)
	}
	return man, nil
}

// aggregate recomputes the whole-archive fingerprint: SHA-256 over each entry's
// per-file hash in rel-sorted order (the same formula worldpack uses).
func aggregate(files map[string]FileEntry) string {
	rels := make([]string, 0, len(files))
	for rel := range files {
		rels = append(rels, rel)
	}
	sort.Strings(rels)
	h := sha256.New()
	for _, rel := range rels {
		io.WriteString(h, files[rel].Hash)
		io.WriteString(h, "\n")
	}
	return hex.EncodeToString(h.Sum(nil))
}

// parseManifest reads the line-format TOC. Required header fields (version,
// engine-range, author, title, description, aggregate-sha256) must be present
// — a missing field is a schema error (D-23 hosting fields are mandatory; values
// may be empty).
func parseManifest(body string) (Manifest, error) {
	m := Manifest{Files: map[string]FileEntry{}}
	var sawVersion, sawAuthor, sawTitle, sawDesc, sawAgg bool
	header := true
	for _, line := range strings.Split(body, "\n") {
		if line == "" {
			continue
		}
		if header {
			switch {
			case strings.HasPrefix(line, "litdworld-version:"):
				v := strings.TrimSpace(strings.TrimPrefix(line, "litdworld-version:"))
				n, err := strconv.Atoi(v)
				if err != nil {
					return Manifest{}, fmt.Errorf("malformed litdworld-version %q", v)
				}
				m.Version, sawVersion = n, true
				continue
			case strings.HasPrefix(line, "engine-range:"):
				m.EngineRange = strings.TrimSpace(strings.TrimPrefix(line, "engine-range:"))
				continue
			case strings.HasPrefix(line, "author:"):
				m.Author, sawAuthor = strings.TrimSpace(strings.TrimPrefix(line, "author:")), true
				continue
			case strings.HasPrefix(line, "title:"):
				m.Title, sawTitle = strings.TrimSpace(strings.TrimPrefix(line, "title:")), true
				continue
			case strings.HasPrefix(line, "description:"):
				m.Description, sawDesc = strings.TrimSpace(strings.TrimPrefix(line, "description:")), true
				continue
			case strings.HasPrefix(line, "aggregate-sha256:"):
				m.Aggregate, sawAgg = strings.TrimSpace(strings.TrimPrefix(line, "aggregate-sha256:")), true
				continue
			case strings.HasPrefix(line, "files:"):
				header = false
				continue
			default:
				return Manifest{}, fmt.Errorf("malformed manifest header line: %q", line)
			}
		}
		parts := strings.SplitN(line, " ", 3)
		if len(parts) != 3 {
			return Manifest{}, fmt.Errorf("malformed manifest row: %q", line)
		}
		size, err := strconv.ParseInt(parts[1], 10, 64)
		if err != nil {
			return Manifest{}, fmt.Errorf("malformed size in manifest row: %q", line)
		}
		m.Files[parts[2]] = FileEntry{Hash: parts[0], Size: size}
	}
	switch {
	case !sawVersion:
		return Manifest{}, fmt.Errorf("manifest missing litdworld-version header")
	case !sawAuthor:
		return Manifest{}, fmt.Errorf("manifest missing hosting-metadata field: author")
	case !sawTitle:
		return Manifest{}, fmt.Errorf("manifest missing hosting-metadata field: title")
	case !sawDesc:
		return Manifest{}, fmt.Errorf("manifest missing hosting-metadata field: description")
	case !sawAgg:
		return Manifest{}, fmt.Errorf("manifest missing aggregate-sha256 header")
	}
	return m, nil
}

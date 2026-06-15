package main

import (
	"archive/zip"
	"bufio"
	"crypto/sha256"
	"encoding/hex"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"strconv"
	"strings"
)

// manifestEntry is one row of the parsed content-hash TOC.
type manifestEntry struct {
	Hash string
	Size int64
	Rel  string
}

// parseManifest reads the manifest body into a path→entry map and the engine range.
func parseManifest(body string) (engineRange string, byPath map[string]manifestEntry, err error) {
	byPath = map[string]manifestEntry{}
	sc := bufio.NewScanner(strings.NewReader(body))
	header := true
	for sc.Scan() {
		line := sc.Text()
		if header {
			switch {
			case strings.HasPrefix(line, "litdworld-version:"):
				continue
			case strings.HasPrefix(line, "engine-range:"):
				engineRange = strings.TrimSpace(strings.TrimPrefix(line, "engine-range:"))
				continue
			case strings.HasPrefix(line, "files:"):
				header = false
				continue
			default:
				return "", nil, fmt.Errorf("malformed manifest header: %q", line)
			}
		}
		// file row: "<hash> <size> <path>" — path may contain spaces, so split
		// into exactly three fields.
		parts := strings.SplitN(line, " ", 3)
		if len(parts) != 3 {
			return "", nil, fmt.Errorf("malformed manifest row: %q", line)
		}
		size, perr := strconv.ParseInt(parts[1], 10, 64)
		if perr != nil {
			return "", nil, fmt.Errorf("malformed size in manifest row %q: %w", line, perr)
		}
		e := manifestEntry{Hash: parts[0], Size: size, Rel: parts[2]}
		byPath[e.Rel] = e
	}
	if err := sc.Err(); err != nil {
		return "", nil, err
	}
	return engineRange, byPath, nil
}

// Unpack restores an archive into destDir, verifying every file's bytes against
// the embedded content-hash manifest. A missing/extra/mismatched entry is an
// error — the round-trip is lossless and tamper-evident.
func Unpack(archivePath, destDir string) error {
	zr, err := zip.OpenReader(archivePath)
	if err != nil {
		return err
	}
	defer zr.Close()

	// Read the manifest first.
	var manBody string
	for _, f := range zr.File {
		if f.Name == manifestName {
			rc, err := f.Open()
			if err != nil {
				return err
			}
			b, err := io.ReadAll(rc)
			rc.Close()
			if err != nil {
				return err
			}
			manBody = string(b)
			break
		}
	}
	if manBody == "" {
		return fmt.Errorf("archive has no %s manifest", manifestName)
	}
	_, want, err := parseManifest(manBody)
	if err != nil {
		return err
	}

	if err := os.MkdirAll(destDir, 0o755); err != nil {
		return err
	}
	restored := map[string]bool{}
	for _, f := range zr.File {
		if f.Name == manifestName {
			continue
		}
		clean := filepath.Clean(filepath.FromSlash(f.Name))
		if filepath.IsAbs(clean) || clean == ".." || strings.HasPrefix(clean, ".."+string(filepath.Separator)) {
			return fmt.Errorf("unsafe path in archive: %q", f.Name)
		}
		exp, ok := want[f.Name]
		if !ok {
			return fmt.Errorf("entry %q is not in the manifest", f.Name)
		}
		dest := filepath.Join(destDir, clean)
		if err := os.MkdirAll(filepath.Dir(dest), 0o755); err != nil {
			return err
		}
		rc, err := f.Open()
		if err != nil {
			return err
		}
		outF, err := os.Create(dest)
		if err != nil {
			rc.Close()
			return err
		}
		h := sha256.New()
		_, err = io.Copy(io.MultiWriter(outF, h), rc)
		outF.Close()
		rc.Close()
		if err != nil {
			return err
		}
		got := hex.EncodeToString(h.Sum(nil))
		if got != exp.Hash {
			return fmt.Errorf("hash mismatch for %q: manifest %s, restored %s", f.Name, exp.Hash, got)
		}
		restored[f.Name] = true
	}
	for name := range want {
		if !restored[name] {
			return fmt.Errorf("manifest lists %q but the archive has no such entry", name)
		}
	}
	return nil
}

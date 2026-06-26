// Package manifest parses and verifies assets/MANIFEST, the asset
// provenance ledger (G4.2, G4.7; docs/prd/09-roadmap/tooling.md §3.2).
//
// The format is a strict subset of TOML: comment lines, [[asset]] table
// headers, and key = "string" pairs. Anything else is a parse error —
// the ledger fails closed rather than silently skipping malformed entries.
package manifest

import (
	"bufio"
	"crypto/sha256"
	"encoding/hex"
	"fmt"
	"io"
	"io/fs"
	"os"
	"path/filepath"
	"sort"
	"strconv"
	"strings"
)

// Asset is one [[asset]] entry.
type Asset struct {
	Path      string // relative to assets/, forward slashes
	Pack      string
	Source    string
	License   string // SPDX id; policy: CC0-1.0 (free-commercial needs operator sign-off)
	Retrieved string // YYYY-MM-DD
	SHA256    string // lowercase hex
	Bytes     int64  // optional: file size in bytes; 0 = unspecified (#539). Lets the
	// binary+assets size budget (#310) be summed without the (gitignored) files present.
	Category string // optional: triangle-budget category (unit/building/other); "" = uncategorized
	// Generated-asset provenance (G4.7). Provenance is "generated" for assetgen
	// output, "" (downloaded) otherwise; the remaining fields are mandatory only
	// when Provenance == "generated".
	Provenance string // "generated" | "" (downloaded)
	Generator  string // generating model/tool + version
	Params     string // generation parameters or an assetgen.toml reference
	Curator    string // human sign-off
	Line       int    // line number of the [[asset]] header, for diagnostics
}

var requiredKeys = []string{"path", "pack", "source", "license", "retrieved", "sha256"}
var optionalKeys = []string{"bytes", "category", "provenance", "generator", "params", "curator"}

// LicenseAllowed reports whether a license string satisfies policy: CC0-1.0, or
// free-commercial (the latter requires curator sign-off, enforced separately).
func LicenseAllowed(license string) bool {
	return license == "CC0-1.0" || license == "free-commercial"
}

// Parse reads the MANIFEST format. It returns an error on the first
// malformed line, duplicate key, missing required key, or duplicate path.
func Parse(r io.Reader) ([]Asset, error) {
	sc := bufio.NewScanner(r)
	var assets []Asset
	var cur map[string]string
	var curLine int
	seenPaths := make(map[string]int)

	flush := func() error {
		if cur == nil {
			return nil
		}
		for _, k := range requiredKeys {
			if cur[k] == "" {
				return fmt.Errorf("line %d: [[asset]] missing required key %q", curLine, k)
			}
		}
		p := cur["path"]
		if strings.Contains(p, "\\") || strings.HasPrefix(p, "/") || strings.Contains(p, "..") {
			return fmt.Errorf("line %d: path %q must be relative with forward slashes", curLine, p)
		}
		if prev, dup := seenPaths[p]; dup {
			return fmt.Errorf("line %d: duplicate entry for path %q (first at line %d)", curLine, p, prev)
		}
		seenPaths[p] = curLine
		var nbytes int64
		if s := cur["bytes"]; s != "" {
			v, err := strconv.ParseInt(s, 10, 64)
			if err != nil || v < 0 {
				return fmt.Errorf("line %d: bytes %q must be a non-negative integer", curLine, s)
			}
			nbytes = v
		}
		assets = append(assets, Asset{
			Path: p, Pack: cur["pack"], Source: cur["source"], License: cur["license"],
			Retrieved: cur["retrieved"], SHA256: strings.ToLower(cur["sha256"]),
			Bytes:    nbytes,
			Category: cur["category"], Provenance: cur["provenance"],
			Generator: cur["generator"], Params: cur["params"], Curator: cur["curator"],
			Line: curLine,
		})
		return nil
	}

	n := 0
	for sc.Scan() {
		n++
		line := strings.TrimSpace(sc.Text())
		switch {
		case line == "" || strings.HasPrefix(line, "#"):
		case line == "[[asset]]":
			if err := flush(); err != nil {
				return nil, err
			}
			cur, curLine = make(map[string]string), n
		default:
			k, v, ok := strings.Cut(line, "=")
			if !ok {
				return nil, fmt.Errorf("line %d: not a comment, [[asset]], or key = \"value\": %q", n, line)
			}
			if cur == nil {
				return nil, fmt.Errorf("line %d: key outside any [[asset]] table", n)
			}
			k = strings.TrimSpace(k)
			v = strings.TrimSpace(v)
			if i := strings.Index(v, "#"); i >= 0 && !insideQuotes(v, i) {
				v = strings.TrimSpace(v[:i])
			}
			if len(v) < 2 || v[0] != '"' || v[len(v)-1] != '"' {
				return nil, fmt.Errorf("line %d: value for %q must be a double-quoted string", n, k)
			}
			v = v[1 : len(v)-1]
			valid := false
			for _, rk := range requiredKeys {
				if k == rk {
					valid = true
				}
			}
			for _, ok := range optionalKeys {
				if k == ok {
					valid = true
				}
			}
			if !valid {
				return nil, fmt.Errorf("line %d: unknown key %q", n, k)
			}
			if _, dup := cur[k]; dup {
				return nil, fmt.Errorf("line %d: duplicate key %q in entry at line %d", n, k, curLine)
			}
			cur[k] = v
		}
	}
	if err := sc.Err(); err != nil {
		return nil, err
	}
	if err := flush(); err != nil {
		return nil, err
	}
	return assets, nil
}

func insideQuotes(s string, idx int) bool {
	in := false
	for i, c := range s {
		if i == idx {
			return in
		}
		if c == '"' {
			in = !in
		}
	}
	return in
}

// Violation is one provenance failure. RuleID is one of
// PROV-UNLISTED, PROV-MISSING, PROV-HASH, PROV-LICENSE.
type Violation struct {
	Path   string
	RuleID string
	Msg    string
}

func (v Violation) String() string { return v.Path + ": " + v.RuleID + ": " + v.Msg }

// Load parses assetsDir/MANIFEST and returns its entries. It is the read-only
// counterpart to Verify — callers that need the parsed ledger (e.g. the
// triangle-budget gate reading per-asset categories) use this.
func Load(assetsDir string) ([]Asset, error) {
	f, err := os.Open(filepath.Join(assetsDir, "MANIFEST"))
	if err != nil {
		return nil, fmt.Errorf("open MANIFEST: %w", err)
	}
	defer f.Close()
	return Parse(f)
}

// Verify checks the ledger in assetsDir/MANIFEST against the files on disk:
// every file listed exactly once, every entry present, every hash matching,
// every license CC0-1.0. Returned violations are sorted by path.
func Verify(assetsDir string) ([]Violation, error) {
	return VerifyPrefix(assetsDir, "")
}

// VerifyPrefix checks only the files and MANIFEST entries under prefix. Prefix
// is relative to assetsDir and must use forward slashes.
func VerifyPrefix(assetsDir string, prefix string) ([]Violation, error) {
	f, err := os.Open(filepath.Join(assetsDir, "MANIFEST"))
	if err != nil {
		return nil, fmt.Errorf("open MANIFEST: %w", err)
	}
	defer f.Close()
	assets, err := Parse(f)
	if err != nil {
		return nil, fmt.Errorf("parse MANIFEST: %w", err)
	}

	var violations []Violation
	prefix = strings.Trim(prefix, "/")
	if prefix != "" {
		prefix += "/"
	}

	listed := make(map[string]Asset, len(assets))
	for _, a := range assets {
		if prefix != "" && !strings.HasPrefix(a.Path, prefix) {
			continue
		}
		listed[a.Path] = a
	}

	onDisk := make(map[string]bool)
	walkRoot := assetsDir
	if prefix != "" {
		walkRoot = filepath.Join(assetsDir, filepath.FromSlash(strings.TrimSuffix(prefix, "/")))
	}
	err = filepath.WalkDir(walkRoot, func(p string, d fs.DirEntry, err error) error {
		if err != nil {
			return err
		}
		if d.IsDir() {
			return nil
		}
		rel, err := filepath.Rel(assetsDir, p)
		if err != nil {
			return err
		}
		rel = filepath.ToSlash(rel)
		if rel == "MANIFEST" || rel == "CREDITS.md" {
			return nil
		}
		onDisk[rel] = true
		if _, ok := listed[rel]; !ok {
			violations = append(violations, Violation{rel, "PROV-UNLISTED", "file under assets/ has no MANIFEST entry"})
		}
		return nil
	})
	if err != nil {
		return nil, err
	}

	for _, a := range assets {
		if prefix != "" && !strings.HasPrefix(a.Path, prefix) {
			continue
		}
		if !onDisk[a.Path] {
			violations = append(violations, Violation{a.Path, "PROV-MISSING", fmt.Sprintf("MANIFEST entry (line %d) but file does not exist", a.Line)})
			continue
		}
		full := filepath.Join(assetsDir, filepath.FromSlash(a.Path))
		sum, err := fileSHA256(full)
		if err != nil {
			return nil, err
		}
		if sum != a.SHA256 {
			violations = append(violations, Violation{a.Path, "PROV-HASH", fmt.Sprintf("sha256 mismatch: MANIFEST has %s, file is %s", a.SHA256, sum)})
		}
		// #539: when a byte size is declared, it must match the file on disk —
		// otherwise the binary+assets size budget (#310) would sum stale figures.
		if a.Bytes > 0 {
			if fi, err := os.Stat(full); err == nil && fi.Size() != a.Bytes {
				violations = append(violations, Violation{a.Path, "PROV-SIZE", fmt.Sprintf("size mismatch: MANIFEST has %d bytes, file is %d", a.Bytes, fi.Size())})
			}
		}
		if !LicenseAllowed(a.License) {
			violations = append(violations, Violation{a.Path, "PROV-LICENSE", fmt.Sprintf("license %q: policy allows only CC0-1.0 or free-commercial", a.License)})
		}
	}

	sort.Slice(violations, func(i, j int) bool {
		if violations[i].Path != violations[j].Path {
			return violations[i].Path < violations[j].Path
		}
		return violations[i].RuleID < violations[j].RuleID
	})
	return violations, nil
}

// TotalBytes sums the declared byte sizes across assets and reports the paths
// that declare none (Bytes == 0). It is what the binary+assets size budget
// (#310) needs: the expected on-disk asset footprint computed from the MANIFEST
// alone, WITHOUT the (gitignored) files present. A non-empty missing list means
// the sum is a lower bound — the caller must decide whether that is gate-worthy
// (a size gate should refuse to pass while any entry lacks a byte size, rather
// than silently undercount).
func TotalBytes(assets []Asset) (sum int64, missing []string) {
	for _, a := range assets {
		if a.Bytes <= 0 {
			missing = append(missing, a.Path)
			continue
		}
		sum += a.Bytes
	}
	sort.Strings(missing)
	return sum, missing
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

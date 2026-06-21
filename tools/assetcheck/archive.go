package main

// World-archive validation (#37; tooling.md §3; M6 D-14, D-20 sandbox). The
// `assetcheck archive <file>` subcommand validates a packed `.litdworld`:
//   - zip structure + the `.litdworld-manifest` schema,
//   - per-file SHA-256 in the manifest matches actual content (ARCHIVE-HASH),
//   - the engine-version range is present and well-formed (ARCHIVE-VERSION),
//   - embedded .glb assets pass the standard glTF rules,
//   - Lua sandbox-safety: no io/os/net reference and no require/loadfile/dofile
//     (ARCHIVE-LUA), found by a real Lua lexer that skips strings and comments
//     so a literal like "ghost" is never a false positive.
// Findings are one line each; there are no bypass flags.

import (
	"archive/zip"
	"bufio"
	"crypto/sha256"
	"encoding/hex"
	"flag"
	"fmt"
	"io"
	"os"
	"sort"
	"strconv"
	"strings"

	lualint "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/asset/lualint"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/semver"
)

// runArchiveCmd handles `assetcheck archive [--json] <file>`.
func runArchiveCmd(args []string) {
	fs := flag.NewFlagSet("archive", flag.ExitOnError)
	jsonMode := fs.Bool("json", false, "emit findings as JSON for CI annotation")
	engineVersion := fs.String("engine-version", "", "if set, refuse archives whose engine-range does not admit this engine semver (loader join-guard)")
	fs.Parse(args)
	if fs.NArg() != 1 {
		fmt.Fprintln(os.Stderr, "usage: assetcheck archive [--json] [--engine-version X.Y.Z] <archive.litdworld>")
		os.Exit(2)
	}
	findings, entries, err := checkArchive(fs.Arg(0), *engineVersion)
	if err != nil {
		fmt.Fprintln(os.Stderr, "assetcheck: archive:", err)
		os.Exit(2)
	}
	emitFindings(findings, *jsonMode, entries)
	if len(findings) > 0 {
		os.Exit(1)
	}
}

const archiveManifestName = ".litdworld-manifest"

// checkArchive validates one world archive. It returns findings, the embedded
// file count (for the summary), and an error only when the file cannot be
// opened as a zip at all. If engineVersion is non-empty, the manifest's
// engine-range must admit it (the loader join-guard) — otherwise the range is
// only checked for well-formedness.
func checkArchive(path, engineVersion string) ([]finding, int, error) {
	zr, err := zip.OpenReader(path)
	if err != nil {
		return nil, 0, err
	}
	defer zr.Close()

	var findings []finding
	add := func(p, rule, msg string) { findings = append(findings, finding{p, rule, msg}) }

	// Read the manifest.
	var manBody string
	haveManifest := false
	for _, f := range zr.File {
		if f.Name == archiveManifestName {
			rc, e := f.Open()
			if e != nil {
				return nil, 0, e
			}
			b, e := io.ReadAll(rc)
			rc.Close()
			if e != nil {
				return nil, 0, e
			}
			manBody, haveManifest = string(b), true
			break
		}
	}
	if !haveManifest {
		add(archiveManifestName, "ARCHIVE-SCHEMA", "archive has no .litdworld-manifest entry")
		return sortFindings(findings), len(zr.File), nil
	}

	engineRange, aggregate, want, schemaErr := parseArchiveManifest(manBody)
	if schemaErr != nil {
		add(archiveManifestName, "ARCHIVE-SCHEMA", schemaErr.Error())
		return sortFindings(findings), len(zr.File), nil
	}
	if engineRange == "" {
		add(archiveManifestName, "ARCHIVE-VERSION", "manifest has no engine-version range")
	} else if !semver.ValidRange(engineRange) {
		add(archiveManifestName, "ARCHIVE-VERSION", fmt.Sprintf("engine-version range %q is not well-formed", engineRange))
	} else if engineVersion != "" && !semver.Satisfies(engineVersion, engineRange) {
		add(archiveManifestName, "ARCHIVE-VERSION", fmt.Sprintf("engine %s does not satisfy archive engine-range %q", engineVersion, engineRange))
	}
	// Aggregate hash: the declared whole-archive fingerprint must equal the value
	// recomputed from the per-entry rows (D-14 "aggregate"). A mismatch means the
	// row set or the aggregate field was altered after pack.
	if got := recomputeAggregate(want); got != aggregate {
		add(archiveManifestName, "ARCHIVE-HASH", fmt.Sprintf("aggregate hash %s… does not match manifest %s…", short(got), short(aggregate)))
	}

	// Per-entry hash + asset/Lua checks.
	seen := map[string]bool{}
	for _, f := range zr.File {
		if f.Name == archiveManifestName {
			continue
		}
		seen[f.Name] = true
		data, rerr := readZipEntry(f)
		if rerr != nil {
			add(f.Name, "ARCHIVE-READ", rerr.Error())
			continue
		}
		exp, listed := want[f.Name]
		if !listed {
			add(f.Name, "ARCHIVE-HASH", "entry is not listed in the manifest")
		} else {
			sum := sha256.Sum256(data)
			got := hex.EncodeToString(sum[:])
			if got != exp {
				add(f.Name, "ARCHIVE-HASH", fmt.Sprintf("content hash %s… does not match manifest %s…", short(got), short(exp)))
			}
		}
		checkArchiveEntry(f.Name, data, add)
	}
	// Manifest rows with no corresponding entry.
	for name := range want {
		if !seen[name] {
			add(name, "ARCHIVE-HASH", "manifest lists this file but the archive has no such entry")
		}
	}

	return sortFindings(findings), len(zr.File), nil
}

func readZipEntry(f *zip.File) ([]byte, error) {
	rc, err := f.Open()
	if err != nil {
		return nil, err
	}
	defer rc.Close()
	return io.ReadAll(rc)
}

func short(h string) string {
	if len(h) > 8 {
		return h[:8]
	}
	return h
}

func sortFindings(f []finding) []finding {
	sort.Slice(f, func(i, j int) bool {
		if f[i].Path != f[j].Path {
			return f[i].Path < f[j].Path
		}
		return f[i].Rule < f[j].Rule
	})
	return f
}

// checkArchiveEntry runs the per-file rules: glTF validation on .glb, Lua
// sandbox lint on .lua.
func checkArchiveEntry(name string, data []byte, add func(p, rule, msg string)) {
	lower := strings.ToLower(name)
	switch {
	case strings.HasSuffix(lower, ".glb"):
		info, err := parseGLBBytes(data)
		if err != nil {
			add(name, "GLTF-CORE", err.Error())
			return
		}
		for _, u := range info.ExternalURIs {
			add(name, "GLTF-URI", fmt.Sprintf("external resource reference (%s) — archives must be self-contained", u))
		}
		for _, e := range append(append([]string{}, info.ExtensionsUsed...), info.ExtensionsRequired...) {
			if cn, bad := compressionExtensions[e]; bad {
				add(name, "GLTF-COMPRESS", fmt.Sprintf("%s compression (%s) — G3N cannot decode (R-FMT-3)", cn, e))
			} else if !allowedGLTFExtensions[e] {
				add(name, "GLTF-EXT", fmt.Sprintf("extension %q not permitted; core profile allows only KHR_materials_unlit (R-FMT-1)", e))
			}
		}
	case strings.HasSuffix(lower, ".lua"):
		for _, v := range lualint.SandboxLint(data) {
			add(name, "ARCHIVE-LUA", v)
		}
	}
}

// parseArchiveManifest parses the worldpack content-hash TOC independently of
// the writer (so a writer bug cannot mask a verification bug).
func parseArchiveManifest(body string) (engineRange, aggregate string, byPath map[string]string, err error) {
	byPath = map[string]string{}
	sc := bufio.NewScanner(strings.NewReader(body))
	header := true
	sawVersion := false
	version := 1
	sawAuthor, sawTitle, sawDesc, sawAggregate := false, false, false, false
	for sc.Scan() {
		line := sc.Text()
		if header {
			switch {
			case strings.HasPrefix(line, "litdworld-version:"):
				v := strings.TrimSpace(strings.TrimPrefix(line, "litdworld-version:"))
				n, perr := strconv.Atoi(v)
				if perr != nil {
					return "", "", nil, fmt.Errorf("malformed litdworld-version %q", v)
				}
				version, sawVersion = n, true
				continue
			case strings.HasPrefix(line, "engine-range:"):
				engineRange = strings.TrimSpace(strings.TrimPrefix(line, "engine-range:"))
				continue
			case strings.HasPrefix(line, "author:"):
				sawAuthor = true
				continue
			case strings.HasPrefix(line, "title:"):
				sawTitle = true
				continue
			case strings.HasPrefix(line, "description:"):
				sawDesc = true
				continue
			case strings.HasPrefix(line, "aggregate-sha256:"):
				aggregate = strings.TrimSpace(strings.TrimPrefix(line, "aggregate-sha256:"))
				sawAggregate = true
				continue
			case strings.HasPrefix(line, "files:"):
				header = false
				continue
			default:
				return "", "", nil, fmt.Errorf("malformed manifest header line: %q", line)
			}
		}
		// v1 row: "<hash> <size> <rel>". v2 row: "<hash> <size> <category> <rel>"
		// (#424). rel is the trailing field (may contain spaces); the version
		// header selects the field count.
		nFields := 3
		if version >= 2 {
			nFields = 4
		}
		parts := strings.SplitN(line, " ", nFields)
		if len(parts) != nFields {
			return "", "", nil, fmt.Errorf("malformed manifest row: %q", line)
		}
		if _, perr := strconv.ParseInt(parts[1], 10, 64); perr != nil {
			return "", "", nil, fmt.Errorf("malformed size in manifest row: %q", line)
		}
		byPath[parts[len(parts)-1]] = parts[0]
	}
	if err := sc.Err(); err != nil {
		return "", "", nil, err
	}
	if !sawVersion {
		return "", "", nil, fmt.Errorf("manifest missing litdworld-version header")
	}
	// Hosting metadata (D-23) must be present from day one — values may be empty,
	// but the FIELDS are mandatory so hosting tooling can rely on them.
	switch {
	case !sawAuthor:
		return "", "", nil, fmt.Errorf("manifest missing hosting-metadata field: author")
	case !sawTitle:
		return "", "", nil, fmt.Errorf("manifest missing hosting-metadata field: title")
	case !sawDesc:
		return "", "", nil, fmt.Errorf("manifest missing hosting-metadata field: description")
	case !sawAggregate:
		return "", "", nil, fmt.Errorf("manifest missing aggregate-sha256 header")
	}
	return engineRange, aggregate, byPath, nil
}

// recomputeAggregate rebuilds the whole-archive fingerprint from the parsed
// manifest rows (Rel-sorted SHA-256 of each per-entry hash) — the same formula
// worldpack.AggregateHash uses, reimplemented here so a writer bug cannot mask a
// verification bug.
func recomputeAggregate(byPath map[string]string) string {
	rels := make([]string, 0, len(byPath))
	for rel := range byPath {
		rels = append(rels, rel)
	}
	sort.Strings(rels)
	h := sha256.New()
	for _, rel := range rels {
		io.WriteString(h, byPath[rel])
		io.WriteString(h, "\n")
	}
	return hex.EncodeToString(h.Sum(nil))
}

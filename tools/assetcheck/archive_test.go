package main

// #37 world-archive FSV. SoT = `assetcheck archive` findings over fixture
// .litdworld archives we assemble with known content. Hashes are X+X=Y
// verifiable: a manifest row's SHA-256 is computed from the same bytes we
// pack, so a deliberate mismatch is detectable.

import (
	"archive/zip"
	"crypto/sha256"
	"encoding/hex"
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"testing"
)

// buildArchive writes a .litdworld zip from files plus a manifest. If
// hashOverride[name] is set, that (wrong) hash is written into the manifest
// instead of the real one — to simulate tampering.
func buildArchive(t *testing.T, files map[string]string, engineRange string, hashOverride map[string]string, omitVersion bool) string {
	t.Helper()
	names := make([]string, 0, len(files))
	for k := range files {
		names = append(names, k)
	}
	sort.Strings(names)

	var man strings.Builder
	if !omitVersion {
		man.WriteString("litdworld-version: 1\n")
	}
	fmt.Fprintf(&man, "engine-range: %s\n", engineRange)
	// Hosting metadata (D-23) — required fields, values may be empty.
	man.WriteString("author: test-author\n")
	man.WriteString("title: Test World\n")
	man.WriteString("description: \n")
	// Per-entry hashes (post-override), then the aggregate over them in sorted
	// order — kept self-consistent so a hash-override case exercises the per-entry
	// path, not the aggregate path.
	rowHash := make(map[string]string, len(names))
	for _, name := range names {
		sum := sha256.Sum256([]byte(files[name]))
		h := hex.EncodeToString(sum[:])
		if ov, ok := hashOverride[name]; ok {
			h = ov
		}
		rowHash[name] = h
	}
	agg := sha256.New()
	for _, name := range names { // names is already sorted
		agg.Write([]byte(rowHash[name] + "\n"))
	}
	fmt.Fprintf(&man, "aggregate-sha256: %s\n", hex.EncodeToString(agg.Sum(nil)))
	fmt.Fprintf(&man, "files: %d\n", len(names))
	for _, name := range names {
		fmt.Fprintf(&man, "%s %d %s\n", rowHash[name], len(files[name]), name)
	}

	path := filepath.Join(t.TempDir(), "world.litdworld")
	out, err := os.Create(path)
	if err != nil {
		t.Fatal(err)
	}
	zw := zip.NewWriter(out)
	write := func(name, body string) {
		w, err := zw.Create(name)
		if err != nil {
			t.Fatal(err)
		}
		if _, err := w.Write([]byte(body)); err != nil {
			t.Fatal(err)
		}
	}
	write(archiveManifestName, man.String())
	for _, name := range names {
		write(name, files[name])
	}
	if err := zw.Close(); err != nil {
		t.Fatal(err)
	}
	out.Close()
	return path
}

// buildArchiveRawManifest writes an archive with a caller-supplied manifest body
// (for malformed/incomplete-header cases the normal builder can't express).
func buildArchiveRawManifest(t *testing.T, manifest string, files map[string]string) string {
	t.Helper()
	path := filepath.Join(t.TempDir(), "world.litdworld")
	out, err := os.Create(path)
	if err != nil {
		t.Fatal(err)
	}
	zw := zip.NewWriter(out)
	w, _ := zw.Create(archiveManifestName)
	w.Write([]byte(manifest))
	for name, body := range files {
		fw, _ := zw.Create(name)
		fw.Write([]byte(body))
	}
	if err := zw.Close(); err != nil {
		t.Fatal(err)
	}
	out.Close()
	return path
}

// TestArchiveMissingHostingFieldRejected — D-23 / #205 edge 4: a manifest missing
// a hosting-metadata FIELD (not just an empty value) is an ARCHIVE-SCHEMA refusal;
// present-but-empty values pass.
func TestArchiveMissingHostingFieldRejected(t *testing.T) {
	body := "ok\n"
	sum := sha256.Sum256([]byte(body))
	entryHash := hex.EncodeToString(sum[:])
	row := fmt.Sprintf("%s %d world.toml\n", entryHash, len(body))
	// Correct aggregate over the single row's hash (sorted order, trivially one).
	a := sha256.Sum256([]byte(entryHash + "\n"))
	aggLine := "aggregate-sha256: " + hex.EncodeToString(a[:]) + "\n"

	// Missing `description:` field entirely → schema error.
	missing := "litdworld-version: 1\nengine-range: *\nauthor: a\ntitle: t\n" + aggLine + "files: 1\n" + row
	got := runArchive(t, buildArchiveRawManifest(t, missing, map[string]string{"world.toml": body}))
	if !hasRule(got, "ARCHIVE-SCHEMA") {
		t.Fatalf("missing hosting field 'description' not rejected: findings=%v", got)
	}
	t.Logf("FSV #205 edge4: manifest missing 'description' → ARCHIVE-SCHEMA (rejected)")

	// All fields present but EMPTY → passes (no schema finding).
	empty := "litdworld-version: 1\nengine-range: *\nauthor: \ntitle: \ndescription: \n" + aggLine + "files: 1\n" + row
	got = runArchive(t, buildArchiveRawManifest(t, empty, map[string]string{"world.toml": body}))
	if hasRule(got, "ARCHIVE-SCHEMA") {
		t.Fatalf("empty-but-present hosting values wrongly rejected: findings=%v", got)
	}
	t.Logf("FSV #205 edge4: empty hosting values present → accepted")
}

// TestArchiveAggregateMismatch — D-14: the declared aggregate-sha256 must equal
// the value recomputed from the per-entry rows. Correct rows but a tampered
// aggregate field is an ARCHIVE-HASH refusal (distinct from the per-entry path).
func TestArchiveAggregateMismatch(t *testing.T) {
	body := "ok\n"
	sum := sha256.Sum256([]byte(body))
	row := fmt.Sprintf("%s %d world.toml\n", hex.EncodeToString(sum[:]), len(body))
	// Deliberately wrong aggregate; every per-entry row is correct.
	bad := "litdworld-version: 1\nengine-range: *\nauthor: \ntitle: \ndescription: \n" +
		"aggregate-sha256: " + strings.Repeat("0", 64) + "\nfiles: 1\n" + row
	got := runArchive(t, buildArchiveRawManifest(t, bad, map[string]string{"world.toml": body}))
	if !hasRule(got, "ARCHIVE-HASH") {
		t.Fatalf("tampered aggregate not rejected: findings=%v", got)
	}
	t.Logf("FSV #205 D-14: rows valid but aggregate-sha256 tampered → ARCHIVE-HASH (rejected): %v", got)

	// Missing aggregate header entirely → schema error.
	noAgg := "litdworld-version: 1\nengine-range: *\nauthor: \ntitle: \ndescription: \nfiles: 1\n" + row
	got = runArchive(t, buildArchiveRawManifest(t, noAgg, map[string]string{"world.toml": body}))
	if !hasRule(got, "ARCHIVE-SCHEMA") {
		t.Fatalf("missing aggregate header not rejected: findings=%v", got)
	}
	t.Logf("FSV #205 D-14: manifest missing aggregate-sha256 → ARCHIVE-SCHEMA (rejected)")
}

// hasRule reports whether any finding carries the given rule code.
func hasRule(findings []finding, rule string) bool {
	for _, f := range findings {
		if f.Rule == rule {
			return true
		}
	}
	return false
}

func runArchive(t *testing.T, path string) []finding {
	t.Helper()
	findings, _, err := checkArchive(path, "")
	if err != nil {
		t.Fatalf("checkArchive: %v", err)
	}
	return findings
}

func runArchiveVersion(t *testing.T, path, engineVersion string) []finding {
	t.Helper()
	findings, _, err := checkArchive(path, engineVersion)
	if err != nil {
		t.Fatalf("checkArchive: %v", err)
	}
	return findings
}

// TestArchiveEngineRangeSatisfaction — #205 edge 2: an archive declaring
// engine-range ">=99.0.0" is refused for a current engine like 1.0.0 (out of
// range), while an in-range engine passes. The parsed range is named in the
// error.
func TestArchiveEngineRangeSatisfaction(t *testing.T) {
	files := map[string]string{"world.toml": "name = \"v\"\n"}
	arc := buildArchive(t, files, ">=99.0.0", nil, false)

	// Current engine 1.0.0 is below the floor → refusal.
	got := runArchiveVersion(t, arc, "1.0.0")
	if !hasRule(got, "ARCHIVE-VERSION") {
		t.Fatalf("engine 1.0.0 vs range >=99.0.0 not refused: findings=%v", got)
	}
	t.Logf("FSV #205 edge2: engine 1.0.0 vs range >=99.0.0 → ARCHIVE-VERSION: %v", got)

	// A future engine 99.1.0 satisfies it → no version finding.
	got = runArchiveVersion(t, arc, "99.1.0")
	if hasRule(got, "ARCHIVE-VERSION") {
		t.Fatalf("engine 99.1.0 wrongly refused for range >=99.0.0: %v", got)
	}
	t.Logf("FSV #205 edge2: engine 99.1.0 satisfies >=99.0.0 → accepted")

	// Bounded range admits the middle, refuses the upper-exclusive bound.
	arc2 := buildArchive(t, files, ">=0.1.0 <0.2.0", nil, false)
	if got := runArchiveVersion(t, arc2, "0.1.5"); hasRule(got, "ARCHIVE-VERSION") {
		t.Fatalf("0.1.5 should satisfy >=0.1.0 <0.2.0: %v", got)
	}
	if got := runArchiveVersion(t, arc2, "0.2.0"); !hasRule(got, "ARCHIVE-VERSION") {
		t.Fatalf("0.2.0 should violate <0.2.0: %v", got)
	}
	t.Logf("FSV #205 edge2: >=0.1.0 <0.2.0 admits 0.1.5, refuses 0.2.0")
}

// TestArchiveValidPasses — a well-formed archive with matching hashes, a valid
// version range, and clean Lua passes.
func TestArchiveValidPasses(t *testing.T) {
	files := map[string]string{
		"world.toml":       "name = \"ok\"\n",
		"scripts/main.lua": "local x = 1\nprint(\"ghost\") -- the word os appears only in this string/comment\n",
	}
	arc := buildArchive(t, files, ">=0.1.0 <0.2.0", nil, false)
	got := runArchive(t, arc)
	t.Logf("FSV valid archive: findings=%v", got)
	if len(got) != 0 {
		t.Fatalf("valid archive should pass, got %v", got)
	}
}

// TestArchiveTamperedHash — a file whose bytes disagree with its manifest hash
// is named with expected/actual prefixes.
func TestArchiveTamperedHash(t *testing.T) {
	files := map[string]string{"data.bin": "real payload"}
	// manifest claims a different hash than the bytes
	bogus := strings.Repeat("a", 64)
	arc := buildArchive(t, files, "*", map[string]string{"data.bin": bogus}, false)
	got := runArchive(t, arc)
	t.Logf("FSV tampered hash: findings=%v", got)
	if len(got) != 1 || got[0].Rule != "ARCHIVE-HASH" || got[0].Path != "data.bin" {
		t.Fatalf("want one ARCHIVE-HASH for data.bin, got %v", got)
	}
	if !strings.Contains(got[0].Msg, "aaaaaaaa") {
		t.Fatalf("finding should cite the manifest hash prefix, got %q", got[0].Msg)
	}
}

// TestArchiveLuaForbidden — io.open is flagged with the file and line; the
// "ghost" string literal in the SAME file is not a false positive.
func TestArchiveLuaForbidden(t *testing.T) {
	lua := "local name = \"ghost\"\n" + // line 1: string contains "os" — must NOT flag
		"local g = 5\n" + // line 2
		"local f = io.open(\"x\")\n" + // line 3: io -> flag
		"os.execute(\"ls\")\n" + // line 4: os -> flag
		"require(\"evil\")\n" // line 5: require -> flag
	files := map[string]string{"scripts/bad.lua": lua}
	arc := buildArchive(t, files, "*", nil, false)
	got := runArchive(t, arc)
	t.Logf("FSV lua sandbox: findings=%v", got)
	var luaFindings []finding
	for _, f := range got {
		if f.Rule == "ARCHIVE-LUA" {
			luaFindings = append(luaFindings, f)
		}
	}
	if len(luaFindings) != 3 {
		t.Fatalf("want exactly 3 ARCHIVE-LUA (io, os, require), got %v", got)
	}
	joined := fmt.Sprint(luaFindings)
	for _, want := range []string{"line 3", "line 4", "line 5", "\"io\"", "\"os\"", "\"require\""} {
		if !strings.Contains(joined, want) {
			t.Fatalf("missing %q in lua findings: %v", want, luaFindings)
		}
	}
	if strings.Contains(joined, "line 1") {
		t.Fatalf("string literal \"ghost\" must not be flagged: %v", luaFindings)
	}
}

// TestArchiveFieldAccessNotFlagged — t.os / obj:os() are field accesses, not
// the global os.
func TestArchiveFieldAccessNotFlagged(t *testing.T) {
	lua := "local t = {}\nt.os = 1\nt.io = 2\nlocal y = t.os\n"
	files := map[string]string{"a.lua": lua}
	arc := buildArchive(t, files, "*", nil, false)
	got := runArchive(t, arc)
	t.Logf("FSV field-access: findings=%v (want none)", got)
	for _, f := range got {
		if f.Rule == "ARCHIVE-LUA" {
			t.Fatalf("field access must not be flagged, got %v", got)
		}
	}
}

// TestArchiveMissingVersion — an archive whose manifest omits the engine range
// fails ARCHIVE-VERSION.
func TestArchiveMissingVersion(t *testing.T) {
	files := map[string]string{"world.toml": "x=1\n"}
	arc := buildArchive(t, files, "", nil, false) // empty engine-range
	got := runArchive(t, arc)
	t.Logf("FSV missing version: findings=%v", got)
	if len(got) != 1 || got[0].Rule != "ARCHIVE-VERSION" {
		t.Fatalf("want one ARCHIVE-VERSION, got %v", got)
	}
}

// TestArchiveMalformedVersion — a non-semver range is rejected.
func TestArchiveMalformedVersion(t *testing.T) {
	files := map[string]string{"world.toml": "x=1\n"}
	arc := buildArchive(t, files, ">=banana", nil, false)
	got := runArchive(t, arc)
	t.Logf("FSV malformed version: findings=%v", got)
	if len(got) != 1 || got[0].Rule != "ARCHIVE-VERSION" || !strings.Contains(got[0].Msg, "well-formed") {
		t.Fatalf("want ARCHIVE-VERSION well-formed, got %v", got)
	}
}

// TestArchiveEmbeddedGLB — a Draco-compressed .glb inside the archive is
// rejected by the standard glTF rule.
func TestArchiveEmbeddedGLB(t *testing.T) {
	glb := buildGLB(t, gltfDoc([]string{"KHR_draco_mesh_compression"}, nil))
	files := map[string]string{"models/unit.glb": string(glb)}
	arc := buildArchive(t, files, "*", nil, false)
	got := runArchive(t, arc)
	t.Logf("FSV embedded glb: findings=%v", got)
	found := false
	for _, f := range got {
		if f.Rule == "GLTF-COMPRESS" && f.Path == "models/unit.glb" {
			found = true
		}
	}
	if !found {
		t.Fatalf("embedded Draco glb should be GLTF-COMPRESS, got %v", got)
	}
}

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
	fmt.Fprintf(&man, "files: %d\n", len(names))
	for _, name := range names {
		sum := sha256.Sum256([]byte(files[name]))
		h := hex.EncodeToString(sum[:])
		if ov, ok := hashOverride[name]; ok {
			h = ov
		}
		fmt.Fprintf(&man, "%s %d %s\n", h, len(files[name]), name)
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

func runArchive(t *testing.T, path string) []finding {
	t.Helper()
	findings, _, err := checkArchive(path)
	if err != nil {
		t.Fatalf("checkArchive: %v", err)
	}
	return findings
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

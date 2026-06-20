package worldarchive

// In-engine archive read-path FSV (#205 "archive read path in litd/asset"; #209
// "the game loads its own map THROUGH the archive — zero behavioral diff vs
// directory load, same data fingerprint"). SoT = the bytes served by the
// verified archive FS, cross-checked against the same files read from the real
// directory: mapdata.Load over the archive must produce the IDENTICAL
// Fingerprint as mapdata.Load over the directory.

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

	mapdata "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/asset/mapdata"
)

// packDir packs srcDir into a deterministic .litdworld at out, mirroring the
// worldpack manifest format (independent of the worldpack tool — this is a test
// fixture). hostMeta supplies author/title/description; if dropField is
// non-empty that hosting field is omitted (to exercise the schema refusal).
func packDir(t *testing.T, srcDir, out, engineRange, dropField string) {
	t.Helper()
	type ent struct {
		rel, hash string
		size      int64
		body      []byte
	}
	var ents []ent
	err := filepath.WalkDir(srcDir, func(p string, d os.DirEntry, err error) error {
		if err != nil || d.IsDir() {
			return err
		}
		b, e := os.ReadFile(p)
		if e != nil {
			return e
		}
		rel, _ := filepath.Rel(srcDir, p)
		sum := sha256.Sum256(b)
		ents = append(ents, ent{filepath.ToSlash(rel), hex.EncodeToString(sum[:]), int64(len(b)), b})
		return nil
	})
	if err != nil {
		t.Fatal(err)
	}
	sort.Slice(ents, func(i, j int) bool { return ents[i].rel < ents[j].rel })

	// aggregate over per-entry hashes, rel-sorted.
	agg := sha256.New()
	for _, e := range ents {
		agg.Write([]byte(e.hash + "\n"))
	}

	var man strings.Builder
	man.WriteString("litdworld-version: 1\n")
	fmt.Fprintf(&man, "engine-range: %s\n", engineRange)
	if dropField != "author" {
		man.WriteString("author: Light in the Dark\n")
	}
	if dropField != "title" {
		man.WriteString("title: First Flame\n")
	}
	if dropField != "description" {
		man.WriteString("description: ashen-veil duel\n")
	}
	fmt.Fprintf(&man, "aggregate-sha256: %s\n", hex.EncodeToString(agg.Sum(nil)))
	fmt.Fprintf(&man, "files: %d\n", len(ents))
	for _, e := range ents {
		fmt.Fprintf(&man, "%s %d %s\n", e.hash, e.size, e.rel)
	}

	f, err := os.Create(out)
	if err != nil {
		t.Fatal(err)
	}
	defer f.Close()
	zw := zip.NewWriter(f)
	mw, _ := zw.Create(manifestName)
	mw.Write([]byte(man.String()))
	for _, e := range ents {
		w, _ := zw.Create(e.rel)
		w.Write(e.body)
	}
	if err := zw.Close(); err != nil {
		t.Fatal(err)
	}
}

// stageFirstFlame builds a world tree: the REAL committed firstflame map under
// data/maps/firstflame/ plus a tiny script, and returns the staging dir.
func stageFirstFlame(t *testing.T) string {
	t.Helper()
	stage := t.TempDir()
	mapDst := filepath.Join(stage, "data", "maps", "firstflame")
	if err := os.MkdirAll(mapDst, 0o755); err != nil {
		t.Fatal(err)
	}
	// repo root is three levels up from litd/asset/worldarchive.
	mapSrc := filepath.Join("..", "..", "..", "data", "maps", "firstflame")
	entries, err := os.ReadDir(mapSrc)
	if err != nil {
		t.Fatalf("read firstflame map: %v", err)
	}
	for _, e := range entries {
		b, err := os.ReadFile(filepath.Join(mapSrc, e.Name()))
		if err != nil {
			t.Fatal(err)
		}
		if err := os.WriteFile(filepath.Join(mapDst, e.Name()), b, 0o644); err != nil {
			t.Fatal(err)
		}
	}
	if err := os.MkdirAll(filepath.Join(stage, "scripts"), 0o755); err != nil {
		t.Fatal(err)
	}
	os.WriteFile(filepath.Join(stage, "scripts", "main.lua"), []byte("-- entry\n"), 0o644)
	return stage
}

// TestArchiveLoadFingerprintMatchesDirectory — the #209 keystone: the map loaded
// from the verified archive FS has the same Fingerprint as the map loaded from
// the directory, proving the archive serves byte-identical data.
func TestArchiveLoadFingerprintMatchesDirectory(t *testing.T) {
	stage := stageFirstFlame(t)
	arcPath := filepath.Join(t.TempDir(), "firstflame.litdworld")
	packDir(t, stage, arcPath, ">=0.1.0 <0.2.0", "")

	// Directory load (the dev path) — SoT baseline.
	dirMap, err := mapdata.Load(os.DirFS(stage), "data/maps/firstflame")
	if err != nil {
		t.Fatalf("directory load: %v", err)
	}

	// Archive load (the shipped path).
	arc, err := Open(arcPath, "")
	if err != nil {
		t.Fatalf("Open archive: %v", err)
	}
	defer arc.Close()
	arcMap, err := mapdata.Load(arc.FS(), "data/maps/firstflame")
	if err != nil {
		t.Fatalf("archive load: %v", err)
	}

	if dirMap.Fingerprint != arcMap.Fingerprint {
		t.Fatalf("fingerprint differs: directory %#x vs archive %#x", dirMap.Fingerprint, arcMap.Fingerprint)
	}
	t.Logf("FSV #209 keystone: archive-load fingerprint %#x == directory-load %#x (byte-identical map served from the archive)", arcMap.Fingerprint, dirMap.Fingerprint)

	// Verified manifest carries the real hosting metadata.
	if arc.Manifest.Title != "First Flame" || arc.Manifest.Author == "" {
		t.Fatalf("manifest metadata not carried: %+v", arc.Manifest)
	}
	t.Logf("FSV manifest: title=%q author=%q engine-range=%q", arc.Manifest.Title, arc.Manifest.Author, arc.Manifest.EngineRange)
}

// TestArchiveTamperRefused — flipping a byte inside any payload entry makes Open
// fail closed (the SoT bytes no longer match the manifest hash).
func TestArchiveTamperRefused(t *testing.T) {
	stage := stageFirstFlame(t)
	arcPath := filepath.Join(t.TempDir(), "ff.litdworld")
	packDir(t, stage, arcPath, "*", "")

	// Rewrite the zip with one entry's bytes altered (manifest unchanged).
	tampered := filepath.Join(t.TempDir(), "tampered.litdworld")
	rewriteEntry(t, arcPath, tampered, "data/maps/firstflame/terrain.toml", func(b []byte) []byte {
		return append(b, []byte("\n# injected\n")...)
	})

	if _, err := Open(tampered, ""); err == nil {
		t.Fatal("tampered archive opened without error (must fail closed)")
	} else {
		t.Logf("FSV tamper: Open refused — %v", err)
	}
}

// TestArchiveEngineVersionGuard — Open refuses when the running engine does not
// satisfy the manifest's engine-range.
func TestArchiveEngineVersionGuard(t *testing.T) {
	stage := stageFirstFlame(t)
	arcPath := filepath.Join(t.TempDir(), "ff.litdworld")
	packDir(t, stage, arcPath, ">=99.0.0", "")

	if _, err := Open(arcPath, "1.0.0"); err == nil {
		t.Fatal("engine 1.0.0 vs range >=99.0.0 opened (must refuse)")
	} else {
		t.Logf("FSV engine-guard: %v", err)
	}
	arc, err := Open(arcPath, "99.2.0")
	if err != nil {
		t.Fatalf("engine 99.2.0 should satisfy >=99.0.0: %v", err)
	}
	arc.Close()
	t.Logf("FSV engine-guard: engine 99.2.0 satisfies >=99.0.0 → opened")
}

// TestArchiveMissingHostingFieldRefused — a manifest missing a hosting field is
// a schema refusal (D-23).
func TestArchiveMissingHostingFieldRefused(t *testing.T) {
	stage := stageFirstFlame(t)
	arcPath := filepath.Join(t.TempDir(), "ff.litdworld")
	packDir(t, stage, arcPath, "*", "description") // omit description

	if _, err := Open(arcPath, ""); err == nil {
		t.Fatal("archive missing 'description' opened (must refuse)")
	} else {
		t.Logf("FSV schema: %v", err)
	}
}

// TestArchiveSandboxLintRefused — defense in depth (#205 'not a validator bypass
// at load'): a hash-valid archive whose Lua references a sandbox-forbidden global
// is refused at Open, even though every content hash matches.
func TestArchiveSandboxLintRefused(t *testing.T) {
	stage := stageFirstFlame(t)
	// Replace the benign script with sandbox-violating Lua BEFORE packing, so the
	// manifest hashes are all correct — only the lint must catch it.
	os.WriteFile(filepath.Join(stage, "scripts", "main.lua"),
		[]byte("local f = io.open('/etc/passwd')\n"), 0o644)
	arcPath := filepath.Join(t.TempDir(), "evil.litdworld")
	packDir(t, stage, arcPath, "*", "")

	if _, err := Open(arcPath, ""); err == nil {
		t.Fatal("archive with io.open Lua opened (must refuse on lint)")
	} else if !strings.Contains(err.Error(), "sandbox lint") {
		t.Fatalf("expected sandbox-lint refusal, got: %v", err)
	} else {
		t.Logf("FSV defense-in-depth: hash-valid archive with forbidden Lua refused — %v", err)
	}
}

// rewriteEntry copies src→dst, applying mut to the named entry's bytes.
func rewriteEntry(t *testing.T, src, dst, name string, mut func([]byte) []byte) {
	t.Helper()
	zr, err := zip.OpenReader(src)
	if err != nil {
		t.Fatal(err)
	}
	defer zr.Close()
	out, err := os.Create(dst)
	if err != nil {
		t.Fatal(err)
	}
	defer out.Close()
	zw := zip.NewWriter(out)
	for _, f := range zr.File {
		rc, _ := f.Open()
		b := make([]byte, f.UncompressedSize64)
		readFull(t, rc, b)
		rc.Close()
		if f.Name == name {
			b = mut(b)
		}
		w, _ := zw.Create(f.Name)
		w.Write(b)
	}
	if err := zw.Close(); err != nil {
		t.Fatal(err)
	}
}

func readFull(t *testing.T, r interface{ Read([]byte) (int, error) }, b []byte) {
	t.Helper()
	n := 0
	for n < len(b) {
		m, err := r.Read(b[n:])
		n += m
		if err != nil {
			break
		}
	}
}

// TestArchiveServesFrozenContentAfterDirEdit — #209 FSV edge 2: once packed, the
// archive is the source of truth. Editing the map in the (dev) directory WITHOUT
// repacking must NOT change what the archive serves — the directory fingerprint
// diverges while the archive's stays frozen.
func TestArchiveServesFrozenContentAfterDirEdit(t *testing.T) {
	stage := stageFirstFlame(t)
	arcPath := filepath.Join(t.TempDir(), "firstflame.litdworld")
	packDir(t, stage, arcPath, ">=0.1.0 <0.2.0", "")

	// Fingerprint the map exactly as the archive packed it.
	arc, err := Open(arcPath, "")
	if err != nil {
		t.Fatalf("Open archive: %v", err)
	}
	packed, err := mapdata.Load(arc.FS(), "data/maps/firstflame")
	arc.Close()
	if err != nil {
		t.Fatalf("archive load: %v", err)
	}

	// Edit a fingerprinted field (biome) in the DIRECTORY only — no repack.
	tomlPath := filepath.Join(stage, "data", "maps", "firstflame", "terrain.toml")
	b, err := os.ReadFile(tomlPath)
	if err != nil {
		t.Fatal(err)
	}
	edited := strings.Replace(string(b), `biome = "ashen-veil"`, `biome = "frostmere"`, 1)
	if edited == string(b) {
		t.Fatal("test setup: biome line not found to edit")
	}
	if err := os.WriteFile(tomlPath, []byte(edited), 0o644); err != nil {
		t.Fatal(err)
	}

	// The directory now serves DIFFERENT content...
	dirMap, err := mapdata.Load(os.DirFS(stage), "data/maps/firstflame")
	if err != nil {
		t.Fatalf("directory reload: %v", err)
	}
	if dirMap.Fingerprint == packed.Fingerprint {
		t.Fatal("edit did not change the directory fingerprint — test is blind")
	}

	// ...but re-opening the UNCHANGED archive still serves the original bytes.
	arc2, err := Open(arcPath, "")
	if err != nil {
		t.Fatalf("re-open archive: %v", err)
	}
	defer arc2.Close()
	frozen, err := mapdata.Load(arc2.FS(), "data/maps/firstflame")
	if err != nil {
		t.Fatalf("archive reload: %v", err)
	}
	if frozen.Fingerprint != packed.Fingerprint {
		t.Fatalf("archive content changed without a repack: %#x != %#x", frozen.Fingerprint, packed.Fingerprint)
	}
	t.Logf("FSV #209 edge: a dir edit moved the directory fp %#x→%#x, but the archive fp stayed %#x — the archive is the source of truth", packed.Fingerprint, dirMap.Fingerprint, frozen.Fingerprint)
}

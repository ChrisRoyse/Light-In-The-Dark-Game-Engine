package manifest

import (
	"crypto/sha256"
	"encoding/hex"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func writeAsset(t *testing.T, dir, rel, content string) string {
	t.Helper()
	p := filepath.Join(dir, filepath.FromSlash(rel))
	if err := os.MkdirAll(filepath.Dir(p), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(p, []byte(content), 0o644); err != nil {
		t.Fatal(err)
	}
	sum := sha256.Sum256([]byte(content))
	return hex.EncodeToString(sum[:])
}

func writeManifest(t *testing.T, dir, body string) {
	t.Helper()
	if err := os.WriteFile(filepath.Join(dir, "MANIFEST"), []byte(body), 0o644); err != nil {
		t.Fatal(err)
	}
}

func entry(path, license, sha string) string {
	return "[[asset]]\n" +
		"path = \"" + path + "\"\n" +
		"pack = \"Synthetic Test Pack 1.0\"\n" +
		"source = \"https://example.com/synthetic\"\n" +
		"license = \"" + license + "\"\n" +
		"retrieved = \"2026-06-11\"\n" +
		"sha256 = \"" + sha + "\"\n"
}

func TestHappyPath(t *testing.T) {
	dir := t.TempDir()
	sum := writeAsset(t, dir, "units/knight.glb", "synthetic-glb-bytes-knight")
	writeManifest(t, dir, "# header\n"+entry("units/knight.glb", "CC0-1.0", sum))
	v, err := Verify(dir)
	if err != nil {
		t.Fatal(err)
	}
	if len(v) != 0 {
		t.Fatalf("want 0 violations, got %v", v)
	}
}

func TestUnlistedFile(t *testing.T) {
	dir := t.TempDir()
	writeAsset(t, dir, "rogue.glb", "unlisted-bytes")
	writeManifest(t, dir, "# empty ledger\n")
	v, err := Verify(dir)
	if err != nil {
		t.Fatal(err)
	}
	if len(v) != 1 || v[0].RuleID != "PROV-UNLISTED" || v[0].Path != "rogue.glb" {
		t.Fatalf("want one PROV-UNLISTED for rogue.glb, got %v", v)
	}
}

func TestListedButMissing(t *testing.T) {
	dir := t.TempDir()
	writeManifest(t, dir, entry("gone.glb", "CC0-1.0", strings.Repeat("a", 64)))
	v, err := Verify(dir)
	if err != nil {
		t.Fatal(err)
	}
	if len(v) != 1 || v[0].RuleID != "PROV-MISSING" {
		t.Fatalf("want one PROV-MISSING, got %v", v)
	}
}

func TestHashMismatch(t *testing.T) {
	dir := t.TempDir()
	sum := writeAsset(t, dir, "m.glb", "original")
	writeManifest(t, dir, entry("m.glb", "CC0-1.0", sum))
	// flip the bytes after recording the hash
	writeAsset(t, dir, "m.glb", "Original")
	v, err := Verify(dir)
	if err != nil {
		t.Fatal(err)
	}
	if len(v) != 1 || v[0].RuleID != "PROV-HASH" {
		t.Fatalf("want one PROV-HASH, got %v", v)
	}
}

func TestNonCC0License(t *testing.T) {
	dir := t.TempDir()
	sum := writeAsset(t, dir, "p.glb", "bytes")
	writeManifest(t, dir, entry("p.glb", "CC-BY-4.0", sum))
	v, err := Verify(dir)
	if err != nil {
		t.Fatal(err)
	}
	if len(v) != 1 || v[0].RuleID != "PROV-LICENSE" {
		t.Fatalf("want one PROV-LICENSE, got %v", v)
	}
}

func TestParseFailsClosed(t *testing.T) {
	bad := []struct{ name, body string }{
		{"missing key", "[[asset]]\npath = \"a.glb\"\n"},
		{"unknown key", entry("a.glb", "CC0-1.0", strings.Repeat("a", 64)) + "extra = \"x\"\n"},
		{"unquoted value", "[[asset]]\npath = a.glb\n"},
		{"key outside table", "path = \"a.glb\"\n"},
		{"duplicate path", entry("a.glb", "CC0-1.0", strings.Repeat("a", 64)) + entry("a.glb", "CC0-1.0", strings.Repeat("b", 64))},
		{"absolute path", entry("/etc/passwd", "CC0-1.0", strings.Repeat("a", 64))},
		{"dotdot path", entry("../escape.glb", "CC0-1.0", strings.Repeat("a", 64))},
		{"garbage line", "[[asset]]\n???\n"},
	}
	for _, tc := range bad {
		if _, err := Parse(strings.NewReader(tc.body)); err == nil {
			t.Errorf("%s: want parse error, got nil", tc.name)
		}
	}
}

func TestParseRealManifest(t *testing.T) {
	// the committed MANIFEST must parse and its entry count must equal the
	// number of asset files on disk (1:1 ledger)
	f, err := os.Open("../../../assets/MANIFEST")
	if err != nil {
		t.Skip("MANIFEST not present:", err)
	}
	defer f.Close()
	assets, err := Parse(f)
	if err != nil {
		t.Fatal(err)
	}
	onDisk := 0
	err = filepath.WalkDir("../../../assets", func(p string, d os.DirEntry, err error) error {
		if err != nil {
			return err
		}
		if !d.IsDir() && filepath.Base(p) != "MANIFEST" {
			onDisk++
		}
		return nil
	})
	if err != nil {
		t.Fatal(err)
	}
	if len(assets) != onDisk {
		t.Fatalf("MANIFEST has %d entries but %d files on disk", len(assets), onDisk)
	}
}

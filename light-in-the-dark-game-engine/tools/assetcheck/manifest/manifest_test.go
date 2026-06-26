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

// #539 FSV: the optional `bytes` field — correct size passes, a stale size is a
// PROV-SIZE violation, malformed sizes fail closed at parse, and TotalBytes sums
// the declared footprint (what the #310 size gate needs) while naming entries
// that declare none. SoT = Verify's violation set + Parse's error + TotalBytes.
func TestManifestBytesFieldFSV(t *testing.T) {
	dir := t.TempDir()
	content := "synthetic-glb-bytes-knight" // 26 bytes
	sum := writeAsset(t, dir, "units/knight.glb", content)
	withBytes := func(n string) string {
		return entry("units/knight.glb", "CC0-1.0", sum) + "bytes = \"" + n + "\"\n"
	}

	// Correct declared size → no violation.
	writeManifest(t, dir, withBytes("26"))
	if v, err := Verify(dir); err != nil || len(v) != 0 {
		t.Fatalf("correct bytes: violations=%v err=%v, want none", v, err)
	}
	t.Log("FSV bytes: declared size matching the file on disk → clean")

	// Stale declared size → PROV-SIZE, naming both figures.
	writeManifest(t, dir, withBytes("999"))
	v, err := Verify(dir)
	if err != nil {
		t.Fatal(err)
	}
	if len(v) != 1 || v[0].RuleID != "PROV-SIZE" {
		t.Fatalf("stale bytes: violations=%v, want one PROV-SIZE", v)
	}
	if !strings.Contains(v[0].Msg, "999") || !strings.Contains(v[0].Msg, "26") {
		t.Fatalf("PROV-SIZE msg %q should name MANIFEST=999 and file=26", v[0].Msg)
	}
	t.Logf("FSV bytes: stale size → %s", v[0])

	// Malformed sizes fail closed at parse.
	for _, bad := range []string{"abc", "-5", "1.5"} {
		if _, err := Parse(strings.NewReader(withBytes(bad))); err == nil {
			t.Fatalf("Parse accepted bytes=%q (want refusal)", bad)
		}
	}
	t.Log("FSV bytes: non-negative-integer-only, malformed sizes refused at parse")

	// TotalBytes: sum declared, report the undeclared.
	assets := []Asset{
		{Path: "a.glb", Bytes: 100},
		{Path: "b.glb", Bytes: 250},
		{Path: "c.glb"}, // undeclared
	}
	sum2, missing := TotalBytes(assets)
	if sum2 != 350 {
		t.Fatalf("TotalBytes sum=%d, want 350", sum2)
	}
	if len(missing) != 1 || missing[0] != "c.glb" {
		t.Fatalf("TotalBytes missing=%v, want [c.glb]", missing)
	}
	t.Logf("FSV TotalBytes: sum=%d missing=%v (a size gate must refuse while any entry lacks a size)", sum2, missing)

	// Omitting bytes stays valid (field is optional, back-compat with the
	// 634 existing entries that predate it).
	writeManifest(t, dir, entry("units/knight.glb", "CC0-1.0", sum))
	if v, err := Verify(dir); err != nil || len(v) != 0 {
		t.Fatalf("omitted bytes: violations=%v err=%v, want none (optional field)", v, err)
	}
	t.Log("FSV bytes: field optional — entries without it still verify")
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

func TestCreditsMetadataIgnoredFSV(t *testing.T) {
	dir := t.TempDir()
	sum := writeAsset(t, dir, "music/theme.ogg", "synthetic-ogg-bytes")
	writeManifest(t, dir, "# header\n"+entry("music/theme.ogg", "CC0-1.0", sum))
	if err := os.WriteFile(filepath.Join(dir, "CREDITS.md"), []byte("# Credits\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	t.Logf("FSV #101 manifest before verify: asset=music/theme.ogg credits=CREDITS.md")
	v, err := Verify(dir)
	if err != nil {
		t.Fatal(err)
	}
	t.Logf("FSV #101 manifest after verify: violations=%v", v)
	if len(v) != 0 {
		t.Fatalf("CREDITS.md should be root metadata, not an unlisted asset: %v", v)
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
		rel, relErr := filepath.Rel("../../../assets", p)
		if relErr != nil {
			return relErr
		}
		rel = filepath.ToSlash(rel)
		if !d.IsDir() && rel != "MANIFEST" && rel != "CREDITS.md" {
			onDisk++
		}
		return nil
	})
	if err != nil {
		t.Fatal(err)
	}
	if onDisk == 0 {
		// asset binaries are gitignored (MANIFEST is the tracked ledger);
		// a fresh checkout legitimately has none — the 1:1 reconcile only
		// applies where the assets actually live
		t.Skipf("no asset files on disk (gitignored); MANIFEST parsed OK with %d entries", len(assets))
	}
	if len(assets) != onDisk {
		t.Fatalf("MANIFEST has %d entries but %d files on disk", len(assets), onDisk)
	}
}

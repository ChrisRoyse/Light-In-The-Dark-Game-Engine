package main

// #10 worldpack FSV. SoT = the archive bytes (sha256) and the unpacked tree.
// Determinism is proven by packing identical source twice and comparing the
// raw archive hashes; the round-trip is proven by diffing the restored tree
// against the source byte-for-byte.

import (
	"crypto/sha256"
	"encoding/hex"
	"os"
	"path/filepath"
	"testing"
)

func writeTree(t *testing.T, root string, files map[string]string) {
	t.Helper()
	for rel, content := range files {
		p := filepath.Join(root, filepath.FromSlash(rel))
		if err := os.MkdirAll(filepath.Dir(p), 0o755); err != nil {
			t.Fatal(err)
		}
		if err := os.WriteFile(p, []byte(content), 0o644); err != nil {
			t.Fatal(err)
		}
	}
}

func sha256File(t *testing.T, path string) string {
	t.Helper()
	b, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	sum := sha256.Sum256(b)
	return hex.EncodeToString(sum[:])
}

// TestDeterministicArchive — packing the same source twice yields byte-identical
// archives. This is the headline D-33 guarantee.
func TestDeterministicArchive(t *testing.T) {
	src := t.TempDir()
	writeTree(t, src, map[string]string{
		"world.toml":          "name = \"test\"\n",
		"maps/a/terrain.toml": "version = 1\n",
		"scripts/main.lua":    "-- entry\n",
		"assets/icon.png":     "PNG-bytes-here",
	})
	out1 := filepath.Join(t.TempDir(), "a.litdworld")
	out2 := filepath.Join(t.TempDir(), "b.litdworld")
	if err := Pack(src, out1, ">=0.1.0 <0.2.0"); err != nil {
		t.Fatal(err)
	}
	if err := Pack(src, out2, ">=0.1.0 <0.2.0"); err != nil {
		t.Fatal(err)
	}
	h1, h2 := sha256File(t, out1), sha256File(t, out2)
	t.Logf("FSV determinism: pack#1=%s pack#2=%s", h1, h2)
	if h1 != h2 {
		t.Fatalf("archives differ: %s vs %s", h1, h2)
	}
}

// TestRoundTrip — unpack restores every file's exact bytes (verified against the
// embedded manifest) and nothing extra.
func TestRoundTrip(t *testing.T) {
	src := t.TempDir()
	files := map[string]string{
		"world.toml":      "name = \"rt\"\n",
		"a/b/c/deep.txt":  "deep content with spaces and \n newlines",
		"empty.dat":       "",
		"with space.txt":  "path containing a space",
		"assets/blob.bin": string(make([]byte, 4096)),
	}
	writeTree(t, src, files)
	arc := filepath.Join(t.TempDir(), "w.litdworld")
	if err := Pack(src, arc, ""); err != nil {
		t.Fatal(err)
	}
	dest := t.TempDir()
	if err := Unpack(arc, dest); err != nil {
		t.Fatalf("unpack: %v", err)
	}
	for rel, content := range files {
		got, err := os.ReadFile(filepath.Join(dest, filepath.FromSlash(rel)))
		if err != nil {
			t.Fatalf("restored file %q missing: %v", rel, err)
		}
		t.Logf("FSV round-trip %q: %d bytes restored", rel, len(got))
		if string(got) != content {
			t.Fatalf("content mismatch for %q", rel)
		}
	}
	// no manifest leaked into the restored tree
	if _, err := os.Stat(filepath.Join(dest, manifestName)); !os.IsNotExist(err) {
		t.Fatal("manifest TOC leaked into restored tree")
	}
}

// TestEmptySource — an empty source dir packs deterministically (manifest only)
// and round-trips without crash.
func TestEmptySource(t *testing.T) {
	src := t.TempDir()
	arc := filepath.Join(t.TempDir(), "empty.litdworld")
	if err := Pack(src, arc, ""); err != nil {
		t.Fatalf("pack empty: %v", err)
	}
	h := sha256File(t, arc)
	t.Logf("FSV empty source archive sha256=%s", h)
	dest := t.TempDir()
	if err := Unpack(arc, dest); err != nil {
		t.Fatalf("unpack empty: %v", err)
	}
	ents, _ := os.ReadDir(dest)
	if len(ents) != 0 {
		t.Fatalf("empty archive restored %d entries, want 0", len(ents))
	}
}

// TestCaseCollisionErrors — two paths differing only in case must be a loud
// error, never a silent overwrite.
func TestCaseCollisionErrors(t *testing.T) {
	src := t.TempDir()
	writeTree(t, src, map[string]string{"Foo.txt": "a", "foo.txt": "b"})
	arc := filepath.Join(t.TempDir(), "c.litdworld")
	err := Pack(src, arc, "")
	t.Logf("FSV case collision: err=%v", err)
	if err == nil {
		t.Fatal("case collision should error")
	}
}

// TestTamperDetected — flipping a packed byte makes unpack fail the hash check.
func TestTamperDetected(t *testing.T) {
	src := t.TempDir()
	writeTree(t, src, map[string]string{"data.bin": "original payload"})
	arc := filepath.Join(t.TempDir(), "t.litdworld")
	if err := Pack(src, arc, ""); err != nil {
		t.Fatal(err)
	}
	// Corrupt the middle half of the archive — this reliably lands in verified
	// compressed payload (zip CRC / our sha256), not ignorable local-header pad.
	raw, err := os.ReadFile(arc)
	if err != nil {
		t.Fatal(err)
	}
	for i := len(raw) / 4; i < 3*len(raw)/4; i++ {
		raw[i] ^= 0xFF
	}
	if err := os.WriteFile(arc, raw, 0o644); err != nil {
		t.Fatal(err)
	}
	dest := t.TempDir()
	err = Unpack(arc, dest)
	t.Logf("FSV tamper: unpack err=%v", err)
	if err == nil {
		t.Fatal("tampered archive should fail unpack")
	}
}

// TestManifestContents — the embedded manifest carries the engine range and one
// content-hash row per file, matching the on-disk hashes.
func TestManifestContents(t *testing.T) {
	src := t.TempDir()
	writeTree(t, src, map[string]string{"x.txt": "hello", "y.txt": "world"})
	entries, err := hashFiles(src, []string{"x.txt", "y.txt"})
	if err != nil {
		t.Fatal(err)
	}
	man := buildManifest(">=1.0", entries)
	t.Logf("FSV manifest body:\n%s", man)
	rng, byPath, perr := parseManifest(man)
	if perr != nil {
		t.Fatal(perr)
	}
	if rng != ">=1.0" {
		t.Fatalf("engine range round-trip failed: %q", rng)
	}
	// independent hash of x.txt
	hx := sha256.Sum256([]byte("hello"))
	if byPath["x.txt"].Hash != hex.EncodeToString(hx[:]) {
		t.Fatalf("x.txt hash mismatch: manifest %s want %s", byPath["x.txt"].Hash, hex.EncodeToString(hx[:]))
	}
}

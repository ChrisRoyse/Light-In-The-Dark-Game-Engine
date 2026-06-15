package main

// #59 provenance-writer FSV. SoT = the written assets/MANIFEST bytes, re-read
// and re-parsed via the manifest package. The round-trip with the assetcheck
// provenance reader (#35) is proven live in the closing comment; here we prove
// the writer's own guarantees: full G4.7 fields, refusal on missing curator,
// hard error on duplicate path, deterministic sorted ordering.

import (
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/tools/assetcheck/manifest"
)

func goodEntry(path string) Entry {
	return Entry{
		Path: path, Pack: "assetgen", Source: "https://e.com", License: "CC0-1.0",
		Retrieved: "2026-06-14", SHA256: strings.Repeat("a", 64),
		Generator: "fable-5 v1", Params: "assetgen.toml#x", Curator: "paul",
	}
}

// TestWriteEntryFullFields — a written entry carries every G4.7 field and the
// re-parsed MANIFEST reflects them exactly.
func TestWriteEntryFullFields(t *testing.T) {
	mp := filepath.Join(t.TempDir(), "MANIFEST")
	if err := AppendEntry(mp, goodEntry("gen/tree.glb")); err != nil {
		t.Fatalf("AppendEntry: %v", err)
	}
	raw, _ := os.ReadFile(mp)
	t.Logf("FSV written MANIFEST:\n%s", raw)
	assets, err := manifest.Parse(strings.NewReader(string(raw)))
	if err != nil {
		t.Fatalf("written MANIFEST does not parse: %v", err)
	}
	if len(assets) != 1 {
		t.Fatalf("want 1 entry, got %d", len(assets))
	}
	a := assets[0]
	if a.Provenance != "generated" || a.Generator != "fable-5 v1" || a.Params != "assetgen.toml#x" || a.Curator != "paul" {
		t.Fatalf("G4.7 fields not written: %+v", a)
	}
}

// TestWriteRefusesMissingCurator — no sign-off, no write.
func TestWriteRefusesMissingCurator(t *testing.T) {
	mp := filepath.Join(t.TempDir(), "MANIFEST")
	e := goodEntry("gen/x.glb")
	e.Curator = ""
	err := AppendEntry(mp, e)
	t.Logf("FSV missing curator: err=%v", err)
	if err == nil || !strings.Contains(err.Error(), "curator") {
		t.Fatalf("missing curator should be refused naming the field, got %v", err)
	}
	if _, statErr := os.Stat(mp); statErr == nil {
		t.Fatal("MANIFEST should not be written on refusal")
	}
}

// TestWriteDuplicatePathHardError — second write of the same path errors and
// does not overwrite.
func TestWriteDuplicatePathHardError(t *testing.T) {
	mp := filepath.Join(t.TempDir(), "MANIFEST")
	if err := AppendEntry(mp, goodEntry("gen/dup.glb")); err != nil {
		t.Fatal(err)
	}
	before, _ := os.ReadFile(mp)
	second := goodEntry("gen/dup.glb")
	second.Curator = "other"
	err := AppendEntry(mp, second)
	t.Logf("FSV duplicate path: err=%v", err)
	if err == nil || !strings.Contains(err.Error(), "duplicate") {
		t.Fatalf("duplicate path should hard-error, got %v", err)
	}
	after, _ := os.ReadFile(mp)
	if string(before) != string(after) {
		t.Fatal("MANIFEST changed on a refused duplicate write")
	}
}

// TestWriteDeterministicOrdering — adding entries in any order yields the same
// sorted MANIFEST (deterministic diffs).
func TestWriteDeterministicOrdering(t *testing.T) {
	mp1 := filepath.Join(t.TempDir(), "MANIFEST")
	for _, p := range []string{"b/2.glb", "a/1.glb", "c/3.glb"} {
		if err := AppendEntry(mp1, goodEntry(p)); err != nil {
			t.Fatal(err)
		}
	}
	mp2 := filepath.Join(t.TempDir(), "MANIFEST")
	for _, p := range []string{"c/3.glb", "a/1.glb", "b/2.glb"} {
		if err := AppendEntry(mp2, goodEntry(p)); err != nil {
			t.Fatal(err)
		}
	}
	a, _ := os.ReadFile(mp1)
	b, _ := os.ReadFile(mp2)
	t.Logf("FSV ordering: insertion order does not affect bytes (len %d == %d)", len(a), len(b))
	if string(a) != string(b) {
		t.Fatalf("insertion order affected output:\n--- mp1 ---\n%s\n--- mp2 ---\n%s", a, b)
	}
	// sorted: a/1, b/2, c/3
	idx := strings.Index(string(a), "a/1.glb")
	if idx < 0 || strings.Index(string(a), "b/2.glb") < idx {
		t.Fatalf("entries not path-sorted:\n%s", a)
	}
}

// TestWriteRejectsBadLicense — a non-policy license is refused.
func TestWriteRejectsBadLicense(t *testing.T) {
	mp := filepath.Join(t.TempDir(), "MANIFEST")
	e := goodEntry("gen/x.glb")
	e.License = "CC-BY-4.0"
	err := AppendEntry(mp, e)
	t.Logf("FSV bad license: err=%v", err)
	if err == nil || !strings.Contains(err.Error(), "license") {
		t.Fatalf("bad license should be refused, got %v", err)
	}
}

// TestAppendFileComputesHash — AppendFile fills sha256 from the real bytes and
// the recorded hash verifies via manifest.Verify.
func TestAppendFileComputesHash(t *testing.T) {
	dir := t.TempDir()
	if err := os.MkdirAll(filepath.Join(dir, "gen"), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(dir, "gen", "rock.png"), []byte("rock-bytes"), 0o644); err != nil {
		t.Fatal(err)
	}
	e := goodEntry("gen/rock.png")
	e.SHA256 = "" // AppendFile must compute it
	if err := AppendFile(dir, e); err != nil {
		t.Fatalf("AppendFile: %v", err)
	}
	v, err := manifest.Verify(dir)
	if err != nil {
		t.Fatal(err)
	}
	t.Logf("FSV AppendFile hash: manifest.Verify violations=%v (want none)", v)
	if len(v) != 0 {
		t.Fatalf("written entry should verify clean, got %v", v)
	}
}

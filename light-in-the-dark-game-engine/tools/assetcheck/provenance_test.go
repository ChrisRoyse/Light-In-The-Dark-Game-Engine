package main

// #35 provenance FSV. SoT = assetcheck findings over a fixture assets/ tree +
// MANIFEST. Base ledger rules (unlisted/stale/license) plus generated-asset
// G4.7 field rules are exercised, each read back finding-by-finding.

import (
	"bytes"
	"crypto/sha256"
	"encoding/hex"
	"fmt"
	"os"
	"path/filepath"
	"testing"
)

// provFixture writes raw MANIFEST entries (full control over provenance fields)
// alongside files on disk.
type provFixture struct {
	dir string
	man bytes.Buffer
}

func newProvFixture(t *testing.T) *provFixture {
	f := &provFixture{dir: t.TempDir()}
	f.man.WriteString("# provenance test ledger\n")
	return f
}

// file writes a real file and returns its sha256.
func (f *provFixture) file(t *testing.T, rel string, content string) string {
	t.Helper()
	p := filepath.Join(f.dir, filepath.FromSlash(rel))
	if err := os.MkdirAll(filepath.Dir(p), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(p, []byte(content), 0o644); err != nil {
		t.Fatal(err)
	}
	sum := sha256.Sum256([]byte(content))
	return hex.EncodeToString(sum[:])
}

// entry appends a MANIFEST [[asset]] table from key/value pairs.
func (f *provFixture) entry(kv ...string) {
	f.man.WriteString("[[asset]]\n")
	for i := 0; i+1 < len(kv); i += 2 {
		fmt.Fprintf(&f.man, "%s = %q\n", kv[i], kv[i+1])
	}
}

func (f *provFixture) run(t *testing.T) []finding {
	t.Helper()
	if err := os.WriteFile(filepath.Join(f.dir, "MANIFEST"), f.man.Bytes(), 0o644); err != nil {
		t.Fatal(err)
	}
	files, err := listFiles(f.dir)
	if err != nil {
		t.Fatal(err)
	}
	findings, _ := check(f.dir, files, "", newWaiverSet())
	return findings
}

func provOnly(fs []finding) []finding {
	var out []finding
	for _, fd := range fs {
		if len(fd.Rule) >= 4 && fd.Rule[:4] == "PROV" {
			out = append(out, fd)
		}
	}
	return out
}

// TestProvUnlistedAndStale — an unlisted file fails by path; a MANIFEST entry
// with no file fails as stale.
func TestProvUnlistedAndStale(t *testing.T) {
	f := newProvFixture(t)
	f.file(t, "stray.png", "PNG-bytes") // on disk, NOT listed
	// listed entry for a file that does not exist:
	f.entry("path", "gone.png", "pack", "P", "source", "https://e.com", "license", "CC0-1.0", "retrieved", "2026-06-11", "sha256", "00")
	got := provOnly(f.run(t))
	t.Logf("FSV unlisted+stale: findings=%v", got)
	rules := map[string]string{}
	for _, fd := range got {
		rules[fd.Path] = fd.Rule
	}
	if rules["stray.png"] != "PROV-UNLISTED" {
		t.Fatalf("stray.png should be PROV-UNLISTED, got %v", got)
	}
	if rules["gone.png"] != "PROV-MISSING" {
		t.Fatalf("gone.png should be PROV-MISSING, got %v", got)
	}
}

// TestProvGeneratedMissingCurator — a generated asset with every G4.7 field
// except curator fails naming the missing field.
func TestProvGeneratedMissingCurator(t *testing.T) {
	f := newProvFixture(t)
	sum := f.file(t, "gen/tree.png", "generated-bytes")
	f.entry(
		"path", "gen/tree.png", "pack", "assetgen", "source", "https://e.com",
		"license", "CC0-1.0", "retrieved", "2026-06-11", "sha256", sum,
		"provenance", "generated", "generator", "fable-5 v1", "params", "assetgen.toml#tree",
		// curator deliberately omitted
	)
	got := provOnly(f.run(t))
	t.Logf("FSV generated-missing-curator: findings=%v", got)
	if len(got) != 1 || got[0].Rule != "PROV-GENFIELD" || got[0].Path != "gen/tree.png" {
		t.Fatalf("want one PROV-GENFIELD for gen/tree.png, got %v", got)
	}
	if !bytes.Contains([]byte(got[0].Msg), []byte("curator")) {
		t.Fatalf("finding should name the missing curator field, got %q", got[0].Msg)
	}
}

// TestProvGeneratedComplete — a generated asset with all G4.7 fields passes.
func TestProvGeneratedComplete(t *testing.T) {
	f := newProvFixture(t)
	sum := f.file(t, "gen/rock.png", "generated-rock")
	f.entry(
		"path", "gen/rock.png", "pack", "assetgen", "source", "https://e.com",
		"license", "CC0-1.0", "retrieved", "2026-06-11", "sha256", sum,
		"provenance", "generated", "generator", "fable-5 v1", "params", "assetgen.toml#rock", "curator", "paul",
	)
	got := provOnly(f.run(t))
	t.Logf("FSV generated-complete: findings=%v (want none)", got)
	if len(got) != 0 {
		t.Fatalf("complete generated asset should pass, got %v", got)
	}
}

// TestProvFreeCommercialSignoff — free-commercial license needs a curator;
// missing it fails, present it passes.
func TestProvFreeCommercialSignoff(t *testing.T) {
	f := newProvFixture(t)
	sum := f.file(t, "fc/sword.png", "fc-bytes")
	f.entry("path", "fc/sword.png", "pack", "P", "source", "https://e.com",
		"license", "free-commercial", "retrieved", "2026-06-11", "sha256", sum)
	got := provOnly(f.run(t))
	t.Logf("FSV free-commercial no-signoff: findings=%v", got)
	if len(got) != 1 || got[0].Rule != "PROV-SIGNOFF" {
		t.Fatalf("free-commercial without curator should be PROV-SIGNOFF, got %v", got)
	}

	f2 := newProvFixture(t)
	sum2 := f2.file(t, "fc/sword.png", "fc-bytes")
	f2.entry("path", "fc/sword.png", "pack", "P", "source", "https://e.com",
		"license", "free-commercial", "retrieved", "2026-06-11", "sha256", sum2, "curator", "paul")
	got2 := provOnly(f2.run(t))
	t.Logf("FSV free-commercial signed: findings=%v (want none)", got2)
	if len(got2) != 0 {
		t.Fatalf("signed free-commercial should pass, got %v", got2)
	}
}

// TestProvUnknownProvenanceValue — a typo'd provenance type fails.
func TestProvUnknownProvenanceValue(t *testing.T) {
	f := newProvFixture(t)
	sum := f.file(t, "x/y.png", "z")
	f.entry("path", "x/y.png", "pack", "P", "source", "https://e.com",
		"license", "CC0-1.0", "retrieved", "2026-06-11", "sha256", sum, "provenance", "synthesised")
	got := provOnly(f.run(t))
	t.Logf("FSV unknown-provenance: findings=%v", got)
	if len(got) != 1 || got[0].Rule != "PROV-GENFIELD" || !bytes.Contains([]byte(got[0].Msg), []byte("synthesised")) {
		t.Fatalf("unknown provenance value should fail naming it, got %v", got)
	}
}

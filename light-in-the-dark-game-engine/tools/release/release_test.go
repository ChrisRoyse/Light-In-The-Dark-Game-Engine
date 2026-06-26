package main

import (
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/updatecheck"
)

// #182 FSV: the release packager checksums artifacts, enforces the size ceiling,
// and emits a manifest the update-check client (#186) consumes — producer and
// consumer share one schema. SoT = the manifest fields vs the known file
// sha256s, the size-gate error, the verify mismatch, and re-run determinism.

func writeArtifact(t *testing.T, dir, name, content string) (string, string) {
	t.Helper()
	p := filepath.Join(dir, name)
	if err := os.WriteFile(p, []byte(content), 0o644); err != nil {
		t.Fatal(err)
	}
	sum := sha256.Sum256([]byte(content))
	return p, hex.EncodeToString(sum[:])
}

// bytesFetcher hands the real producer output to the real consumer (Check).
type bytesFetcher []byte

func (b bytesFetcher) Fetch() ([]byte, error) { return []byte(b), nil }

func TestReleaseBuildManifestFSV(t *testing.T) {
	dir := t.TempDir()
	linPath, linSHA := writeArtifact(t, dir, "litd-linux-amd64.tar.gz", "linux-binary-bytes")
	winPath, winSHA := writeArtifact(t, dir, "litd-windows-amd64.zip", "windows-binary-bytes")

	m, sums, err := BuildManifest("v0.3.0", "https://litd.example/releases", []ArtifactInput{
		{OS: "windows", Path: winPath}, // intentionally out of order — must sort
		{OS: "linux", Path: linPath},
	}, MaxArtifactBytes)
	if err != nil {
		t.Fatalf("BuildManifest: %v", err)
	}
	t.Logf("AFTER: manifest=%+v\nsums:\n%s", m, sums)

	// SoT: artifacts sorted by OS, sha256 == the known file hashes, URL = base+name.
	if len(m.Artifacts) != 2 || m.Artifacts[0].OS != "linux" || m.Artifacts[1].OS != "windows" {
		t.Fatalf("artifacts = %+v, want [linux, windows]", m.Artifacts)
	}
	if m.Artifacts[0].SHA256 != linSHA || m.Artifacts[1].SHA256 != winSHA {
		t.Fatalf("sha mismatch: got %s/%s want %s/%s", m.Artifacts[0].SHA256, m.Artifacts[1].SHA256, linSHA, winSHA)
	}
	if m.Artifacts[0].URL != "https://litd.example/releases/litd-linux-amd64.tar.gz" {
		t.Fatalf("linux URL = %s", m.Artifacts[0].URL)
	}
	if !strings.Contains(sums, linSHA+"  litd-linux-amd64.tar.gz") {
		t.Fatalf("sums missing linux line:\n%s", sums)
	}

	// Loop closure: the emitted manifest is consumable by the #186 client. An
	// older build sees this v0.3.0 release as an available update with the linux link.
	raw, _ := json.Marshal(m)
	res := updatecheck.Check("v0.1.0", "linux", bytesFetcher(raw))
	t.Logf("loop: updatecheck.Check(v0.1.0) on the emitted manifest → %s artifact=%+v", res.Outcome, res.Artifact)
	if res.Outcome != updatecheck.OutcomeUpdateAvailable || res.Artifact == nil || res.Artifact.SHA256 != linSHA {
		t.Fatalf("emitted manifest not consumable by client: %+v", res)
	}
}

// Edge 1: an artifact over the ceiling fails the size gate.
func TestReleaseSizeGateFSV(t *testing.T) {
	dir := t.TempDir()
	fat, _ := writeArtifact(t, dir, "fat.bin", "0123456789AB") // 12 bytes
	_, _, err := BuildManifest("v0.3.0", "https://x/r", []ArtifactInput{{OS: "linux", Path: fat}}, 10)
	t.Logf("AFTER: 12-byte artifact, ceiling 10 → err=%v", err)
	if err == nil || !strings.Contains(err.Error(), "exceeds") {
		t.Fatalf("oversized artifact must fail the gate; got err=%v", err)
	}
}

// Edge 2: a tampered file fails verification against the published sum.
func TestReleaseVerifyMismatchFSV(t *testing.T) {
	dir := t.TempDir()
	p, sum := writeArtifact(t, dir, "a.bin", "original-bytes")
	if err := VerifyArtifact(p, sum); err != nil {
		t.Fatalf("clean verify should pass: %v", err)
	}
	if err := os.WriteFile(p, []byte("TAMPERED-bytes"), 0o644); err != nil {
		t.Fatal(err)
	}
	err := VerifyArtifact(p, sum)
	t.Logf("AFTER: tampered file vs published sum → err=%v", err)
	if err == nil || !strings.Contains(err.Error(), "mismatch") {
		t.Fatalf("tampered artifact must fail verify; got %v", err)
	}
}

// Edge 3: re-running the same inputs yields a byte-identical manifest.
func TestReleaseDeterministicFSV(t *testing.T) {
	dir := t.TempDir()
	p1, _ := writeArtifact(t, dir, "linux.tgz", "L")
	p2, _ := writeArtifact(t, dir, "win.zip", "W")
	in := []ArtifactInput{{OS: "linux", Path: p1}, {OS: "windows", Path: p2}}

	m1, s1, err1 := BuildManifest("v1.0.0", "https://x/r", in, MaxArtifactBytes)
	m2, s2, err2 := BuildManifest("v1.0.0", "https://x/r", in, MaxArtifactBytes)
	if err1 != nil || err2 != nil {
		t.Fatalf("errs: %v %v", err1, err2)
	}
	b1, _ := json.Marshal(m1)
	b2, _ := json.Marshal(m2)
	if string(b1) != string(b2) || s1 != s2 {
		t.Fatalf("re-run not byte-identical:\n%s\nvs\n%s", b1, b2)
	}
	t.Logf("AFTER: re-run byte-identical manifest (%d bytes) + sums", len(b1))
}

// A bad version is refused.
func TestReleaseBadVersionFSV(t *testing.T) {
	dir := t.TempDir()
	p, _ := writeArtifact(t, dir, "a.bin", "x")
	_, _, err := BuildManifest("banana", "https://x/r", []ArtifactInput{{OS: "linux", Path: p}}, MaxArtifactBytes)
	t.Logf("AFTER: version 'banana' → err=%v", err)
	if err == nil || !strings.Contains(err.Error(), "valid semver") {
		t.Fatalf("bad version must be refused; got %v", err)
	}
}

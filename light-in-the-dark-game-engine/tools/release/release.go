// Command release packages per-OS download artifacts into the own-site release
// manifest + a SHA256SUMS file (#182; D-2026-06-11-22). It is the headless
// producer half of the update-check loop: it emits exactly the manifest format
// litd/updatecheck consumes (#186), so a published release and the in-client
// update check speak one schema with no second parser.
//
// What is gated vs. built: the original issue framed the deliverable as a GitHub
// Actions release.yml, but this project permanently runs no GitHub Actions
// (operator decision; gate locally). The CI runners, cgo cross-builds, the site
// bucket upload, and the static download page stay deferred on #182. The
// locally-measurable core — checksum every artifact, enforce the per-artifact
// size ceiling, and emit the manifest + sums deterministically — is here and
// testable headlessly.
package main

import (
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"sort"
	"strings"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/updatecheck"
)

// MaxArtifactBytes is the per-target size ceiling (#182: ≤ 300 MB per artifact).
const MaxArtifactBytes int64 = 300 * 1024 * 1024

// ArtifactInput is one built artifact to publish.
type ArtifactInput struct {
	OS   string // "windows" | "linux" | "macos"
	Path string // local path to the built artifact file
}

// BuildManifest checksums each artifact, enforces the size ceiling, and returns
// the release manifest plus the SHA256SUMS body. It fails closed: a missing
// file, an oversized artifact, or a bad version is an error — never a manifest
// with a silently-dropped entry.
func BuildManifest(version, baseURL string, arts []ArtifactInput, maxBytes int64) (updatecheck.Manifest, string, error) {
	m := updatecheck.Manifest{LatestVersion: version}
	if !updatecheck.IsValidVersion(version) {
		return updatecheck.Manifest{}, "", fmt.Errorf("version %q is not a valid semver", version)
	}
	// Stable order so re-runs are byte-identical.
	in := append([]ArtifactInput(nil), arts...)
	sort.Slice(in, func(i, j int) bool { return in[i].OS < in[j].OS })

	var sums strings.Builder
	for _, a := range in {
		fi, err := os.Stat(a.Path)
		if err != nil {
			return updatecheck.Manifest{}, "", fmt.Errorf("stat %s: %w", a.Path, err)
		}
		if fi.Size() > maxBytes {
			return updatecheck.Manifest{}, "", fmt.Errorf("artifact %s is %d bytes, exceeds the %d-byte ceiling", a.Path, fi.Size(), maxBytes)
		}
		sum, err := fileSHA256(a.Path)
		if err != nil {
			return updatecheck.Manifest{}, "", err
		}
		base := filepath.Base(a.Path)
		m.Artifacts = append(m.Artifacts, updatecheck.Artifact{
			OS:     a.OS,
			URL:    strings.TrimRight(baseURL, "/") + "/" + base,
			SHA256: sum,
		})
		fmt.Fprintf(&sums, "%s  %s\n", sum, base)
	}
	return m, sums.String(), nil
}

// VerifyArtifact recomputes a file's sha256 and checks it against the expected
// value — the published verify step a downloader runs against the sums file.
func VerifyArtifact(path, wantSHA string) error {
	got, err := fileSHA256(path)
	if err != nil {
		return err
	}
	if !strings.EqualFold(got, wantSHA) {
		return fmt.Errorf("checksum mismatch for %s: file is %s, expected %s", filepath.Base(path), got, wantSHA)
	}
	return nil
}

func fileSHA256(path string) (string, error) {
	f, err := os.Open(path)
	if err != nil {
		return "", err
	}
	defer f.Close()
	h := sha256.New()
	if _, err := io.Copy(h, f); err != nil {
		return "", err
	}
	return hex.EncodeToString(h.Sum(nil)), nil
}

func main() {
	if len(os.Args) < 2 {
		fmt.Fprintln(os.Stderr, "usage: release -version <v> -base <url> -out <manifest.json> [-sums <SHA256SUMS>] os=path [os=path ...]")
		os.Exit(2)
	}
	var version, baseURL, outPath, sumsPath string
	var arts []ArtifactInput
	args := os.Args[1:]
	for i := 0; i < len(args); i++ {
		switch args[i] {
		case "-version":
			i++
			version = args[i]
		case "-base":
			i++
			baseURL = args[i]
		case "-out":
			i++
			outPath = args[i]
		case "-sums":
			i++
			sumsPath = args[i]
		default:
			osKey, path, ok := strings.Cut(args[i], "=")
			if !ok {
				fmt.Fprintf(os.Stderr, "release: bad artifact spec %q (want os=path)\n", args[i])
				os.Exit(2)
			}
			arts = append(arts, ArtifactInput{OS: osKey, Path: path})
		}
	}
	if version == "" || baseURL == "" || outPath == "" || len(arts) == 0 {
		fmt.Fprintln(os.Stderr, "release: -version, -base, -out, and at least one os=path are required")
		os.Exit(2)
	}
	m, sums, err := BuildManifest(version, baseURL, arts, MaxArtifactBytes)
	if err != nil {
		fmt.Fprintln(os.Stderr, "release:", err)
		os.Exit(1)
	}
	body, _ := json.MarshalIndent(m, "", "  ")
	if err := os.WriteFile(outPath, append(body, '\n'), 0o644); err != nil {
		fmt.Fprintln(os.Stderr, "release:", err)
		os.Exit(1)
	}
	if sumsPath != "" {
		if err := os.WriteFile(sumsPath, []byte(sums), 0o644); err != nil {
			fmt.Fprintln(os.Stderr, "release:", err)
			os.Exit(1)
		}
	}
	fmt.Printf("release: wrote %s (%d artifacts) + sums\n", outPath, len(m.Artifacts))
}

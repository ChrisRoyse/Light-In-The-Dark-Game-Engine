// Package updatecheck is the client-side update-check core (#186; D-2026-06-11-22):
// fetch an own-site release manifest, compare its advertised version to the
// running build, and decide whether to notify. It is notify-only — it never
// downloads or installs anything; it returns the download URL for the menu to
// open. It is also fail-closed and non-fatal: any fetch, parse, or comparison
// problem yields a silent Skip (offline builds and bad manifests must never
// crash or block the menu), never a false "you're up to date".
//
// Version comparison reuses litd/semver (the same matcher the world-archive and
// hub use) — there is no second version parser (issue constraint).
package updatecheck

import (
	"encoding/json"
	"fmt"
	"strings"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/semver"
)

// Artifact is one per-OS download entry in the manifest.
type Artifact struct {
	OS     string `json:"os"`     // "windows" | "linux" | "macos"
	URL    string `json:"url"`    // download page / file URL
	SHA256 string `json:"sha256"` // lowercase hex; the user can verify the download
}

// Manifest is the own-site release manifest at /releases/manifest.json.
type Manifest struct {
	LatestVersion string     `json:"latestVersion"`
	Artifacts     []Artifact `json:"artifacts"`
}

// Outcome is the check verdict.
type Outcome int

const (
	// OutcomeSkipped: no decision could be made (offline, malformed manifest,
	// unstamped/dev build). Non-fatal — the menu loads normally, no notification.
	OutcomeSkipped Outcome = iota
	// OutcomeUpToDate: the running build is >= the advertised latest.
	OutcomeUpToDate
	// OutcomeUpdateAvailable: a newer version is advertised.
	OutcomeUpdateAvailable
)

func (o Outcome) String() string {
	switch o {
	case OutcomeUpToDate:
		return "up-to-date"
	case OutcomeUpdateAvailable:
		return "update-available"
	default:
		return "skipped"
	}
}

// Result is the check outcome. Artifact is non-nil only when an update is
// available AND the manifest carries an entry for the requested OS.
type Result struct {
	Outcome  Outcome
	Latest   string    // the manifest's advertised version (when known)
	Artifact *Artifact // the download for the requested OS (notify-only; never auto-fetched)
	Reason   string    // diagnostics for a Skip, or the comparison note
}

// Fetcher retrieves the raw manifest bytes. The network boundary is injected so
// the decision logic is pure and testable; the production impl is HTTPFetcher.
type Fetcher interface {
	Fetch() ([]byte, error)
}

// ParseManifest decodes and validates a manifest. It fails closed: invalid JSON,
// a missing/invalid latestVersion, or a malformed artifact is an error, never a
// silently-empty manifest.
func ParseManifest(b []byte) (Manifest, error) {
	var m Manifest
	dec := json.NewDecoder(strings.NewReader(string(b)))
	dec.DisallowUnknownFields()
	if err := dec.Decode(&m); err != nil {
		return Manifest{}, fmt.Errorf("decode manifest: %w", err)
	}
	if !validVersion(m.LatestVersion) {
		return Manifest{}, fmt.Errorf("manifest latestVersion %q is not a valid semver", m.LatestVersion)
	}
	for i, a := range m.Artifacts {
		if a.OS == "" || a.URL == "" {
			return Manifest{}, fmt.Errorf("artifact %d missing os or url", i)
		}
	}
	return m, nil
}

// Check fetches the manifest and decides. running is the stamped build version
// (buildinfo.Version); osKey is "windows"/"linux"/"macos". Any failure is a
// non-fatal Skip — the caller (menu) shows a notification only on
// OutcomeUpdateAvailable.
func Check(running, osKey string, f Fetcher) Result {
	if !validVersion(running) {
		// "dev" / unstamped builds cannot be compared — skip silently.
		return Result{Outcome: OutcomeSkipped, Reason: fmt.Sprintf("running version %q not comparable", running)}
	}
	raw, err := f.Fetch()
	if err != nil {
		return Result{Outcome: OutcomeSkipped, Reason: "fetch: " + err.Error()}
	}
	m, err := ParseManifest(raw)
	if err != nil {
		return Result{Outcome: OutcomeSkipped, Reason: err.Error()}
	}
	if !isNewer(m.LatestVersion, running) {
		return Result{Outcome: OutcomeUpToDate, Latest: m.LatestVersion,
			Reason: fmt.Sprintf("running %s >= latest %s", running, m.LatestVersion)}
	}
	return Result{Outcome: OutcomeUpdateAvailable, Latest: m.LatestVersion, Artifact: artifactFor(m, osKey)}
}

// isNewer reports whether latest is strictly greater than running, via semver.
func isNewer(latest, running string) bool {
	return semver.Satisfies(stripV(latest), ">"+stripV(running))
}

// IsValidVersion reports whether v is a comparable semver (optionally v-prefixed).
// Exported for the release packager (#182) so the producer and consumer share one
// version validator.
func IsValidVersion(v string) bool { return validVersion(v) }

// validVersion reports whether v parses as a semver, by reusing semver's range
// validator on a comparator built from v (no separate parser).
func validVersion(v string) bool {
	v = stripV(v)
	if v == "" {
		return false
	}
	return semver.ValidRange(">=" + v)
}

func stripV(v string) string {
	v = strings.TrimSpace(v)
	if len(v) > 0 && (v[0] == 'v' || v[0] == 'V') {
		return v[1:]
	}
	return v
}

// artifactFor returns the manifest's artifact for osKey, or nil if none is
// advertised for this OS (a notify-without-direct-link case).
func artifactFor(m Manifest, osKey string) *Artifact {
	for i := range m.Artifacts {
		if m.Artifacts[i].OS == osKey {
			return &m.Artifacts[i]
		}
	}
	return nil
}

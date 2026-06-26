// Package buildinfo exposes the release identity stamped into the binary at
// link time (#184; the process is docs/release/versioning.md). There are NO
// version constants here on purpose: the single source of version truth is the
// git tag, injected at build time via
//
//	-ldflags "-X .../litd/buildinfo.version=$(git describe --tags) \
//	          -X .../litd/buildinfo.commit=$(git rev-parse HEAD)[-dirty] \
//	          -X .../litd/buildinfo.date=$(date -u +%Y-%m-%dT%H:%M:%SZ)"
//
// An unstamped build (plain `go build`/`go test`) reports a dev identity — and
// reads the VCS commit from the Go build info so it still names its commit —
// but Publishable() is false: dev/dirty builds are never released.
package buildinfo

import (
	"fmt"
	"runtime/debug"
	"strings"
)

// Set by the linker via -ldflags -X. Lowercase + unexported so nothing in-tree
// can assign them except the release build; tests construct Info literals.
var (
	version string
	commit  string
	date    string
)

// Info is the resolved build identity a binary reports via --version.
type Info struct {
	Version string // semver from the git tag (e.g. "v0.4.2"), or "dev"
	Commit  string // full SHA, with a "-dirty" suffix on an unclean tree
	Date    string // RFC3339 build timestamp, or "unknown"
}

// Get returns the resolved build identity. With no -ldflags stamping it falls
// back to a dev identity, reading the VCS revision/dirty flag from the embedded
// Go build info so even a plain `go build` names its commit.
func Get() Info {
	v, c, d := version, commit, date
	if v == "" {
		v = "dev"
	}
	if c == "" {
		c = vcsCommit()
	}
	if c == "" {
		c = "unknown"
	}
	if d == "" {
		d = "unknown"
	}
	return Info{Version: v, Commit: c, Date: d}
}

// Publishable reports whether this build may be released: a stamped, non-dev,
// clean build. A dev version or a dirty/unknown commit is never publishable
// (docs/release/versioning.md, "Build stamping").
func (i Info) Publishable() bool {
	if i.Version == "" || i.Version == "dev" {
		return false
	}
	if i.Commit == "" || i.Commit == "unknown" || strings.HasSuffix(i.Commit, "-dirty") {
		return false
	}
	return true
}

// String renders the identity as `version commit builddate` — the --version
// line.
func (i Info) String() string {
	return fmt.Sprintf("%s %s %s", i.Version, i.Commit, i.Date)
}

// vcsCommit reads the commit SHA (and dirty flag) the Go toolchain embedded in
// the binary's build info. Returns "" when unavailable (e.g. `go test` caches).
func vcsCommit() string {
	bi, ok := debug.ReadBuildInfo()
	if !ok {
		return ""
	}
	var rev string
	var dirty bool
	for _, s := range bi.Settings {
		switch s.Key {
		case "vcs.revision":
			rev = s.Value
		case "vcs.modified":
			dirty = s.Value == "true"
		}
	}
	if rev != "" && dirty {
		rev += "-dirty"
	}
	return rev
}
